use schemars::JsonSchema;
use serde::{de, Deserialize, Deserializer, Serialize};
use ts_rs::TS;

use super::super::{canonical_sha256, ContractValueError};

macro_rules! decision_digest_ref {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, JsonSchema, TS)]
        #[serde(transparent)]
        #[schemars(transparent)]
        pub struct $name(#[schemars(regex(pattern = r"^sha256:[0-9a-f]{64}$"))] String);

        impl $name {
            pub fn from_content(value: &impl Serialize) -> Self {
                Self(canonical_sha256(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = ContractValueError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                let valid = value.strip_prefix("sha256:").is_some_and(|digest| {
                    digest.len() == 64
                        && digest
                            .bytes()
                            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                });
                if valid {
                    Ok(Self(value))
                } else {
                    Err(ContractValueError::new(
                        stringify!($name),
                        "must be sha256 followed by 64 lowercase hexadecimal characters",
                    ))
                }
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                Self::try_from(String::deserialize(deserializer)?).map_err(de::Error::custom)
            }
        }
    };
}

decision_digest_ref!(DecisionExplanationRef);
decision_digest_ref!(DecisionCandidateRef);
decision_digest_ref!(DecisionPositionSnapshotRef);
decision_digest_ref!(LineStepRef);
decision_digest_ref!(AtomicFactRef);
decision_digest_ref!(ExplanationPathRef);
decision_digest_ref!(EngineAssessmentRef);
decision_digest_ref!(SemanticOutcomeRef);
decision_digest_ref!(KnowledgeNodeRef);
decision_digest_ref!(KnowledgeRuleRef);
