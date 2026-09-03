#[derive(Clone, Debug)]
pub(crate) struct ReviewSessionTraceId(String);

impl ReviewSessionTraceId {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        let suffix = value.strip_prefix("trace:review-session:")?;
        if suffix.len() != 36
            || !suffix
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
        {
            return None;
        }
        Some(Self(value.to_owned()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::ReviewSessionTraceId;

    #[test]
    fn accepts_only_bounded_review_session_trace_ids() {
        assert!(ReviewSessionTraceId::parse(
            "trace:review-session:123e4567-e89b-42d3-a456-426614174000"
        )
        .is_some());
        assert!(ReviewSessionTraceId::parse("trace:review-session:player@example.com").is_none());
        assert!(
            ReviewSessionTraceId::parse("other:123e4567-e89b-42d3-a456-426614174000").is_none()
        );
    }
}
