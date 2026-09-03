use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum OpeningMetadata {
    Present {
        eco: String,
        name: String,
        provenance: OpeningIdentificationProvenance,
    },
    Absent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum OpeningIdentificationProvenance {
    Service {
        provider: OpeningServiceProvider,
        attribution: OpeningServiceAttribution,
    },
    Catalog {
        catalog_version: OpeningCatalogVersion,
        #[schemars(range(min = 1))]
        matched_ply: u16,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum OpeningServiceProvider {
    Lichess,
    ChessCom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum OpeningServiceAttribution {
    DirectImport,
    PgnUrl { canonical_url: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
pub enum OpeningCatalogVersion {
    #[serde(rename = "chess-openings/2026.04.16")]
    V2026_04_16,
}
