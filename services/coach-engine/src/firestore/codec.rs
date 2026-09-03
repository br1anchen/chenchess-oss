use std::io::{Read, Write};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::{DateTime, Utc};
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use serde::{
    de::{self, DeserializeOwned},
    Deserialize, Deserializer, Serialize, Serializer,
};
use serde_json::{Map, Number, Value};

use super::FirestoreError;

/// Marks a payload string as gzipped canonical JSON in Base64.
///
/// A JSON document opens with `{`, `[`, `"`, `-`, a digit, or one of `true`,
/// `false` and `null`, so no uncompressed payload can begin with this prefix.
/// That is what lets one field hold both encodings without a schema version to
/// negotiate.
const COMPRESSED_PREFIX: &str = "gzip:";

/// A queryless Firestore field whose wire value is gzipped canonical JSON.
///
/// Firestore sees one string value, while callers keep a typed Rust value.
/// Records that need TTL or query projections put those fields alongside this
/// payload in their document type.
///
/// The value is compressed because Firestore caps a single field at 1,048,487
/// bytes and a whole document at 1 MiB, and a Game Review's analysis reaches
/// that ceiling: a 155-ply Game measured 976,876 bytes of canonical JSON, and
/// 180 plies exceeded it outright, so long Games could not be stored at all.
/// Splitting the payload across fields cannot help, because the document cap
/// binds first. Measured Game Analysis payloads compress about 5.5 to 6.5
/// times, so Base64-wrapped gzip leaves the largest legal Game far inside the
/// limit while keeping the field one queryless string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DurablePayload<T>(T);

impl<T> DurablePayload<T> {
    pub(crate) const fn new(value: T) -> Self {
        Self(value)
    }

    pub(crate) fn into_inner(self) -> T {
        self.0
    }

    pub(crate) const fn as_ref(&self) -> &T {
        &self.0
    }
}

impl<T: Serialize> Serialize for DurablePayload<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let canonical =
            serde_json_canonicalizer::to_string(&self.0).map_err(serde::ser::Error::custom)?;
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(canonical.as_bytes())
            .map_err(serde::ser::Error::custom)?;
        let compressed = encoder.finish().map_err(serde::ser::Error::custom)?;
        format!("{COMPRESSED_PREFIX}{}", BASE64.encode(compressed)).serialize(serializer)
    }
}

impl<'de, T> Deserialize<'de> for DurablePayload<T>
where
    T: DeserializeOwned,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        // Documents written before compression hold bare canonical JSON. Every
        // durable store reachable here has such documents in staging today, so
        // this branch reads live data rather than guarding a hypothetical.
        // Delete it once no `payload` field without the prefix remains.
        let Some(base64) = encoded.strip_prefix(COMPRESSED_PREFIX) else {
            return serde_json::from_str(&encoded)
                .map(DurablePayload)
                .map_err(de::Error::custom);
        };
        let compressed = BASE64.decode(base64).map_err(de::Error::custom)?;
        let mut canonical = String::new();
        GzDecoder::new(compressed.as_slice())
            .read_to_string(&mut canonical)
            .map_err(de::Error::custom)?;
        serde_json::from_str(&canonical)
            .map(DurablePayload)
            .map_err(de::Error::custom)
    }
}

/// Why a stored document could not become its typed value.
///
/// Distinct from `FirestoreError` so the reason survives the hop to a caller.
/// Every caller answers `InvalidDocument` in the end, and discarding serde's
/// message on the way is what let a retired contract variant read as a missing
/// document rather than a broken one.
#[derive(Debug)]
pub(super) struct DocumentDecodeError(String);

impl DocumentDecodeError {
    pub(super) fn reason(&self) -> &str {
        &self.0
    }
}

impl From<DocumentDecodeError> for FirestoreError {
    fn from(_: DocumentDecodeError) -> Self {
        Self::InvalidDocument
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct FirestoreDocument {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) update_time: Option<String>,
    fields: Map<String, Value>,
}

impl FirestoreDocument {
    pub(super) fn encode<T: Serialize>(
        value: &T,
        timestamps: &[(&str, DateTime<Utc>)],
    ) -> Result<Self, FirestoreError> {
        let Value::Object(mut object) =
            serde_json::to_value(value).map_err(|_| FirestoreError::InvalidDocument)?
        else {
            return Err(FirestoreError::InvalidDocument);
        };
        for (name, timestamp) in timestamps {
            object.insert(
                (*name).to_string(),
                tagged(
                    "timestampValue",
                    Value::String(timestamp.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true)),
                ),
            );
        }
        let fields = object
            .into_iter()
            .map(|(name, value)| {
                let value = if timestamps
                    .iter()
                    .any(|(timestamp_name, _)| *timestamp_name == name)
                {
                    value
                } else {
                    json_to_firestore(value)
                };
                (name, value)
            })
            .collect();
        Ok(Self {
            name: None,
            update_time: None,
            fields,
        })
    }

    /// Decodes, and says why when it cannot.
    ///
    /// Every caller turns a failure into an absence — a listed record is
    /// dropped, an addressed one answers as not found — because a Player must
    /// not be told which of someone else's documents exists. That is the right
    /// answer to give and the wrong one to give silently: a contract change
    /// that retires a stored variant makes every document undecodable at once,
    /// and the only trace of it was a Player reporting their own Games
    /// missing. The reason travels out of here rather than being reported
    /// here, because whether one unreadable document deserves a log line
    /// depends on whether the caller asked for that document or was listing
    /// many.
    pub(super) fn decode<T: DeserializeOwned>(self) -> Result<T, DocumentDecodeError> {
        let object = self
            .fields
            .into_iter()
            .map(|(name, value)| firestore_to_json(value).map(|value| (name, value)))
            .collect::<Result<Map<_, _>, _>>()
            .map_err(|_| {
                DocumentDecodeError("document is not a Firestore field map".to_string())
            })?;
        serde_json::from_value(Value::Object(object))
            .map_err(|error| DocumentDecodeError(super::failure::sanitized(&error)))
    }
}

fn json_to_firestore(value: Value) -> Value {
    match value {
        Value::Null => tagged("nullValue", Value::Null),
        Value::Bool(value) => tagged("booleanValue", Value::Bool(value)),
        Value::Number(value) => number_to_firestore(value),
        Value::String(value) => tagged("stringValue", Value::String(value)),
        Value::Array(values) => tagged(
            "arrayValue",
            Value::Object(Map::from_iter([(
                "values".to_string(),
                Value::Array(values.into_iter().map(json_to_firestore).collect()),
            )])),
        ),
        Value::Object(fields) => tagged(
            "mapValue",
            Value::Object(Map::from_iter([(
                "fields".to_string(),
                Value::Object(
                    fields
                        .into_iter()
                        .map(|(name, value)| (name, json_to_firestore(value)))
                        .collect(),
                ),
            )])),
        ),
    }
}

fn number_to_firestore(value: Number) -> Value {
    value.as_i64().map_or_else(
        || {
            tagged(
                "doubleValue",
                Value::Number(
                    Number::from_f64(value.as_f64().expect("JSON numbers are finite"))
                        .expect("finite JSON number is a valid double"),
                ),
            )
        },
        |value| tagged("integerValue", Value::String(value.to_string())),
    )
}

fn firestore_to_json(value: Value) -> Result<Value, FirestoreError> {
    let Value::Object(value) = value else {
        return Err(FirestoreError::InvalidDocument);
    };
    if value.len() != 1 {
        return Err(FirestoreError::InvalidDocument);
    }
    let (kind, value) = value.into_iter().next().expect("one Firestore value");
    match kind.as_str() {
        "nullValue" if value.is_null() => Ok(Value::Null),
        "booleanValue" if value.is_boolean() => Ok(value),
        "stringValue" | "timestampValue" if value.is_string() => Ok(value),
        "integerValue" => value
            .as_str()
            .and_then(|value| value.parse::<i64>().ok())
            .map(Number::from)
            .map(Value::Number)
            .ok_or(FirestoreError::InvalidDocument),
        "doubleValue" if value.is_number() => Ok(value),
        "arrayValue" => {
            let mut value = object(value)?;
            let values = value
                .remove("values")
                .unwrap_or_else(|| Value::Array(Vec::new()));
            let Value::Array(values) = values else {
                return Err(FirestoreError::InvalidDocument);
            };
            Ok(Value::Array(
                values
                    .into_iter()
                    .map(firestore_to_json)
                    .collect::<Result<_, _>>()?,
            ))
        }
        "mapValue" => {
            let mut value = object(value)?;
            let fields = value
                .remove("fields")
                .unwrap_or_else(|| Value::Object(Map::new()));
            let Value::Object(fields) = fields else {
                return Err(FirestoreError::InvalidDocument);
            };
            Ok(Value::Object(
                fields
                    .into_iter()
                    .map(|(name, value)| firestore_to_json(value).map(|value| (name, value)))
                    .collect::<Result<_, _>>()?,
            ))
        }
        _ => Err(FirestoreError::InvalidDocument),
    }
}

fn object(value: Value) -> Result<Map<String, Value>, FirestoreError> {
    match value {
        Value::Object(value) => Ok(value),
        _ => Err(FirestoreError::InvalidDocument),
    }
}

fn tagged(name: &str, value: Value) -> Value {
    Value::Object(Map::from_iter([(name.to_string(), value)]))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct PayloadFixture {
        label: String,
        values: BTreeMap<String, u8>,
    }

    #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct DurableFixture {
        schema_version: u8,
        payload: DurablePayload<PayloadFixture>,
    }

    #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct LongPayloadFixture {
        values: BTreeMap<String, String>,
    }

    #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct DurableLongFixture {
        payload: DurablePayload<LongPayloadFixture>,
    }

    /// Firestore's own ceiling: 1 MiB less 89 bytes for a single field value.
    /// A whole document is capped at 1 MiB, so spreading a payload across
    /// sibling fields does not raise it.
    const FIRESTORE_FIELD_LIMIT_BYTES: usize = 1_048_487;

    fn payload_string(document: &FirestoreDocument) -> &str {
        document.fields["payload"]["stringValue"]
            .as_str()
            .expect("a durable payload is one string field")
    }

    #[test]
    fn durable_payload_is_one_compressed_canonical_json_string_and_round_trips_typed_data() {
        let fixture = DurableFixture {
            schema_version: 1,
            payload: DurablePayload::new(PayloadFixture {
                label: "fixture".to_string(),
                values: BTreeMap::from([("zeta".to_string(), 2), ("alpha".to_string(), 1)]),
            }),
        };

        let document = FirestoreDocument::encode(&fixture, &[]).unwrap();

        let stored = payload_string(&document);
        let compressed = BASE64
            .decode(stored.strip_prefix(COMPRESSED_PREFIX).unwrap())
            .unwrap();
        let mut canonical = String::new();
        GzDecoder::new(compressed.as_slice())
            .read_to_string(&mut canonical)
            .unwrap();

        assert_eq!(
            canonical,
            r#"{"label":"fixture","values":{"alpha":1,"zeta":2}}"#
        );
        assert_eq!(document.fields["schemaVersion"]["integerValue"], "1");
        assert_eq!(
            document.clone().decode::<DurableFixture>().unwrap(),
            fixture
        );
    }

    #[test]
    fn durable_payload_reads_a_document_written_before_compression() {
        let document = FirestoreDocument {
            name: None,
            update_time: None,
            fields: Map::from_iter([
                (
                    "schemaVersion".to_string(),
                    tagged("integerValue", "1".into()),
                ),
                (
                    "payload".to_string(),
                    tagged(
                        "stringValue",
                        r#"{"label":"fixture","values":{"alpha":1,"zeta":2}}"#.into(),
                    ),
                ),
            ]),
        };

        assert_eq!(
            document.decode::<DurableFixture>().unwrap(),
            DurableFixture {
                schema_version: 1,
                payload: DurablePayload::new(PayloadFixture {
                    label: "fixture".to_string(),
                    values: BTreeMap::from([("zeta".to_string(), 2), ("alpha".to_string(), 1)]),
                }),
            }
        );
    }

    #[test]
    fn a_long_game_analysis_payload_fits_the_firestore_field_limit() {
        // Shaped like the analysis a long Game stores: one entry per ply, each
        // holding a distinct position and its principal variation. A 155-ply
        // Game measured 976,876 bytes of canonical JSON against the limit
        // below, and 180 plies exceeded it, so the fixture is sized to clear
        // the limit uncompressed the way a real long Game does.
        let squares = ["a1", "b3", "c5", "d7", "e2", "f4", "g6", "h8"];
        let roles = ["p", "n", "b", "r", "q", "k"];
        let values = (0..9_000u32)
            .map(|index| {
                let position = format!(
                    "{}/{}{} w KQkq - {} {}",
                    roles[index as usize % roles.len()].repeat(8),
                    squares[index as usize % squares.len()],
                    index,
                    index % 50,
                    index / 2
                );
                let variation = (0..12)
                    .map(|step| {
                        format!(
                            "{}{}{}",
                            squares[(index as usize + step) % squares.len()],
                            squares[(index as usize + step * 3) % squares.len()],
                            (index as usize + step) % 97
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                (format!("ply-{index:05}-{position}"), variation)
            })
            .collect::<BTreeMap<_, _>>();
        let fixture = LongPayloadFixture { values };

        let canonical = serde_json_canonicalizer::to_string(&fixture).unwrap();
        let document = FirestoreDocument::encode(
            &DurableLongFixture {
                payload: DurablePayload::new(fixture),
            },
            &[],
        )
        .unwrap();
        let stored = payload_string(&document).len();

        assert!(
            canonical.len() > FIRESTORE_FIELD_LIMIT_BYTES,
            "the fixture must be too large to store uncompressed, was {} bytes",
            canonical.len()
        );
        assert!(
            stored < FIRESTORE_FIELD_LIMIT_BYTES,
            "the compressed payload must fit, was {stored} bytes"
        );
        assert_eq!(
            document.decode::<DurableLongFixture>().unwrap().payload.0,
            serde_json::from_str::<LongPayloadFixture>(&canonical).unwrap()
        );
    }

    #[test]
    fn durable_payload_rejects_malformed_or_wrongly_typed_json() {
        for payload in ["not-json", r#"{"label":"fixture","values":[]}"#] {
            let document = FirestoreDocument {
                name: None,
                update_time: None,
                fields: Map::from_iter([
                    (
                        "schemaVersion".to_string(),
                        tagged("integerValue", "1".into()),
                    ),
                    (
                        "payload".to_string(),
                        tagged("stringValue", payload.to_string().into()),
                    ),
                ]),
            };

            assert!(document.decode::<DurableFixture>().is_err());
        }
    }
}
