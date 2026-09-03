//! Crate-private HTTP `Retry-After` and `google.rpc.RetryInfo` parsers.
//!
//! Untrusted provider bytes become a trusted [`Duration`] at this boundary.
//! Callers honour that duration; they do not re-parse.

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde_json::Value;

/// Longest wait honoured from an untrusted provider signal. Matches the
/// Lichess import cooldown cap: long enough to back off, short enough that
/// a bad header cannot take a replica down for a day.
pub(crate) const MAX_HONORED_RETRY_AFTER: Duration = Duration::from_secs(15 * 60);

/// RFC 9110 `Retry-After`: delta-seconds or IMF-fixdate. Invalid input is
/// `None` so the admission floor applies. A past HTTP-date is zero, which
/// the floor also replaces. Digit strings that overflow `u64` saturate to
/// the honour cap rather than falling through as unusable.
pub(crate) fn parse_retry_after(value: &str, now: DateTime<Utc>) -> Option<Duration> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Some(saturating_delta_seconds(value));
    }
    let retry_at = DateTime::parse_from_rfc2822(value)
        .ok()?
        .with_timezone(&Utc);
    let remaining = retry_at.signed_duration_since(now);
    if remaining <= chrono::TimeDelta::zero() {
        return Some(Duration::ZERO);
    }
    remaining
        .to_std()
        .ok()
        .map(|wait| wait.min(MAX_HONORED_RETRY_AFTER))
}

/// `google.rpc.RetryInfo.retryDelay` from a provider JSON body. Walks the
/// parsed value for a RetryInfo detail or a `retryDelay` / `retry_delay`
/// field, including OpenRouter's `metadata.raw` wrapper.
pub(crate) fn retry_delay_from_error_body(parsed: &Value) -> Option<Duration> {
    find_retry_delay(parsed)
}

fn saturating_delta_seconds(digits: &str) -> Duration {
    let seconds = parse_u64_saturating(digits);
    Duration::from_secs(seconds).min(MAX_HONORED_RETRY_AFTER)
}

fn parse_u64_saturating(digits: &str) -> u64 {
    let mut seconds = 0u64;
    for byte in digits.bytes() {
        let digit = u64::from(byte - b'0');
        seconds = seconds.saturating_mul(10).saturating_add(digit);
    }
    seconds
}

fn find_retry_delay(value: &Value) -> Option<Duration> {
    match value {
        Value::Object(map) => {
            if let Some(delay) = map
                .get("retryDelay")
                .or_else(|| map.get("retry_delay"))
                .and_then(parse_proto_duration)
            {
                return Some(delay);
            }
            if let Some(raw) = map.get("raw").and_then(Value::as_str) {
                if let Ok(nested) = serde_json::from_str::<Value>(raw) {
                    if let Some(delay) = find_retry_delay(&nested) {
                        return Some(delay);
                    }
                }
            }
            map.values().find_map(find_retry_delay)
        }
        Value::Array(items) => items.iter().find_map(find_retry_delay),
        _ => None,
    }
}

/// Protobuf JSON `Duration`: `"8s"`, `"1.500s"`, or `{seconds, nanos}`.
fn parse_proto_duration(value: &Value) -> Option<Duration> {
    match value {
        Value::String(text) => parse_proto_duration_string(text),
        Value::Object(map) => {
            let seconds = map.get("seconds").and_then(json_u64).unwrap_or(0);
            let nanos = map.get("nanos").and_then(json_u64).unwrap_or(0);
            duration_from_seconds_nanos(seconds, nanos)
        }
        Value::Number(number) => number
            .as_u64()
            .map(|seconds| Duration::from_secs(seconds).min(MAX_HONORED_RETRY_AFTER)),
        _ => None,
    }
}

fn parse_proto_duration_string(value: &str) -> Option<Duration> {
    let value = value.trim();
    let seconds_part = value.strip_suffix('s')?;
    if seconds_part.is_empty() {
        return None;
    }
    if let Some((whole, frac)) = seconds_part.split_once('.') {
        let seconds = if whole.is_empty() {
            0
        } else if whole.bytes().all(|byte| byte.is_ascii_digit()) {
            parse_u64_saturating(whole)
        } else {
            return None;
        };
        if frac.is_empty() || !frac.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        let nanos = frac_to_nanos(frac)?;
        duration_from_seconds_nanos(seconds, nanos)
    } else if seconds_part.bytes().all(|byte| byte.is_ascii_digit()) {
        Some(saturating_delta_seconds(seconds_part))
    } else {
        None
    }
}

fn frac_to_nanos(frac: &str) -> Option<u64> {
    let mut digits = frac.as_bytes();
    if digits.len() > 9 {
        digits = &digits[..9];
    }
    let mut nanos = parse_u64_saturating(std::str::from_utf8(digits).ok()?);
    for _ in digits.len()..9 {
        nanos = nanos.saturating_mul(10);
    }
    Some(nanos)
}

fn duration_from_seconds_nanos(seconds: u64, nanos: u64) -> Option<Duration> {
    let wait = Duration::from_secs(seconds).saturating_add(Duration::from_nanos(nanos));
    if wait.is_zero() {
        return None;
    }
    Some(wait.min(MAX_HONORED_RETRY_AFTER))
}

fn json_u64(value: &Value) -> Option<u64> {
    match value {
        Value::Number(number) => number.as_u64(),
        Value::String(text) if text.bytes().all(|byte| byte.is_ascii_digit()) => {
            Some(parse_u64_saturating(text))
        }
        _ => None,
    }
}

pub(crate) fn retry_after_seconds_u32(value: &str, now: DateTime<Utc>) -> Option<u32> {
    let wait = parse_retry_after(value, now)?;
    let secs = wait
        .as_secs()
        .saturating_add(u64::from(wait.subsec_nanos() > 0));
    Some(u32::try_from(secs).unwrap_or(u32::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-08-21T15:00:00Z")
            .expect("fixture now")
            .with_timezone(&Utc)
    }

    #[test]
    fn retry_after_parses_delta_seconds_and_http_date() {
        let now = now();
        assert_eq!(parse_retry_after("7", now), Some(Duration::from_secs(7)));
        assert_eq!(
            parse_retry_after(" 12 ", now),
            Some(Duration::from_secs(12))
        );
        assert_eq!(parse_retry_after("", now), None);
        assert_eq!(parse_retry_after("soon", now), None);
        assert_eq!(
            parse_retry_after("Fri, 21 Aug 2026 15:00:05 GMT", now),
            Some(Duration::from_secs(5))
        );
        assert_eq!(
            parse_retry_after("Fri, 21 Aug 2026 14:59:00 GMT", now),
            Some(Duration::ZERO)
        );
        assert_eq!(
            parse_retry_after(&(u64::from(u32::MAX) + 1).to_string(), now),
            Some(MAX_HONORED_RETRY_AFTER)
        );
        assert_eq!(
            parse_retry_after("99999999999999999999999", now),
            Some(MAX_HONORED_RETRY_AFTER)
        );
    }

    #[test]
    fn retry_info_reads_retry_delay_from_the_body() {
        let vertex = json!({
            "error": {
                "code": 429,
                "status": "RESOURCE_EXHAUSTED",
                "details": [{
                    "@type": "type.googleapis.com/google.rpc.RetryInfo",
                    "retryDelay": "8s"
                }]
            }
        });
        assert_eq!(
            retry_delay_from_error_body(&vertex),
            Some(Duration::from_secs(8))
        );

        let wrapped = json!({
            "error": {
                "code": 429,
                "metadata": {
                    "raw": r#"{"error":{"details":[{"@type":"type.googleapis.com/google.rpc.RetryInfo","retry_delay":"1.5s"}]}}"#
                }
            }
        });
        assert_eq!(
            retry_delay_from_error_body(&wrapped),
            Some(Duration::from_millis(1500))
        );

        let object = json!({"retryDelay": {"seconds": 4, "nanos": 0}});
        assert_eq!(
            retry_delay_from_error_body(&object),
            Some(Duration::from_secs(4))
        );
        assert_eq!(retry_delay_from_error_body(&json!({})), None);
    }
}
