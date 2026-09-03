use schemars::JsonSchema;
use serde::{de, Deserialize, Deserializer, Serialize};
use ts_rs::TS;

use super::{model::canonical_sha256, *};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewSessionEvidencePacket {
    pub entries: Vec<EvidenceEntry>,
}

impl ReviewSessionEvidencePacket {
    pub fn appended(&self, entries: impl IntoIterator<Item = EvidenceEntry>) -> Self {
        let mut packet = self.clone();
        packet.entries.extend(entries);
        packet
    }

    pub fn contains(&self, evidence_id: &EvidenceId) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.metadata().evidence_id == *evidence_id)
    }

    pub fn objective_branch_evidence(&self) -> Self {
        Self {
            entries: self
                .entries
                .iter()
                .filter(|entry| {
                    matches!(
                        entry,
                        EvidenceEntry::Position { .. }
                            | EvidenceEntry::EngineAnalysis { .. }
                            | EvidenceEntry::HumanMoveModel { .. }
                            | EvidenceEntry::Branch { .. }
                    )
                })
                .cloned()
                .collect(),
        }
    }

    /// Evidence safe to carry across a Review Session restart.
    ///
    /// Human-model candidates are authoring intermediates. The durable session
    /// needs only the position/engine/branch facts required to resume objective
    /// exploration plus ordinary provenance facts.
    pub fn durable_review_session_evidence(&self) -> Self {
        Self {
            entries: self
                .entries
                .iter()
                .filter(|entry| {
                    matches!(
                        entry,
                        EvidenceEntry::Position { .. }
                            | EvidenceEntry::EngineAnalysis { .. }
                            | EvidenceEntry::Branch { .. }
                            | EvidenceEntry::Provenance { .. }
                    )
                })
                .cloned()
                .collect(),
        }
    }

    pub fn position_evidence_id(&self, position_ref: &PositionRef) -> Option<EvidenceId> {
        self.position(position_ref)
            .map(|(metadata, _)| metadata.evidence_id.clone())
    }

    pub fn position(
        &self,
        position_ref: &PositionRef,
    ) -> Option<(&EvidenceMetadata, &PositionSnapshot)> {
        self.entries.iter().find_map(|entry| match entry {
            EvidenceEntry::Position { metadata, position }
                if &position.position_ref == position_ref =>
            {
                Some((metadata, position))
            }
            _ => None,
        })
    }

    pub fn engine_analysis(
        &self,
        position_ref: &PositionRef,
    ) -> Option<(&EvidenceMetadata, &EngineAnalysisEvidence)> {
        self.entries.iter().find_map(|entry| match entry {
            EvidenceEntry::EngineAnalysis {
                metadata,
                position_ref: recorded,
                analysis,
            } if recorded == position_ref => Some((metadata, analysis)),
            _ => None,
        })
    }

    pub fn human_move_model(
        &self,
        position_ref: &PositionRef,
    ) -> Option<(&EvidenceMetadata, &HumanMoveModelEvidence)> {
        self.entries.iter().find_map(|entry| match entry {
            EvidenceEntry::HumanMoveModel {
                metadata,
                position_ref: recorded,
                analysis,
            } if recorded == position_ref => Some((metadata, analysis)),
            _ => None,
        })
    }

    pub fn branch(&self, branch_ref: &BranchRef) -> Option<(&EvidenceMetadata, &BranchEvidence)> {
        self.entries.iter().find_map(|entry| match entry {
            EvidenceEntry::Branch { metadata, branch } if &branch.branch_ref == branch_ref => {
                Some((metadata, branch))
            }
            _ => None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum EvidenceEntry {
    Position {
        metadata: EvidenceMetadata,
        position: PositionSnapshot,
    },
    EngineAnalysis {
        metadata: EvidenceMetadata,
        position_ref: PositionRef,
        analysis: EngineAnalysisEvidence,
    },
    HumanMoveModel {
        metadata: EvidenceMetadata,
        position_ref: PositionRef,
        analysis: HumanMoveModelEvidence,
    },
    Branch {
        metadata: EvidenceMetadata,
        branch: BranchEvidence,
    },
    Provenance {
        metadata: EvidenceMetadata,
        fact: ProvenanceFact,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceKind {
    Position,
    EngineAnalysis,
    HumanMoveModel,
    Branch,
    Provenance,
}

impl EvidenceKind {
    pub fn is_cached(self) -> bool {
        matches!(
            self,
            Self::Position | Self::EngineAnalysis | Self::HumanMoveModel
        )
    }
}

/// Common identity and dependency behavior shared by every evidence payload.
///
/// Keep payload-specific construction in the constructor that names that payload; callers
/// should not manufacture placeholder identities or reimplement packet lookups.
pub trait EvidencePayload {
    fn kind(&self) -> EvidenceKind;
    fn metadata(&self) -> &EvidenceMetadata;
    fn metadata_mut(&mut self) -> &mut EvidenceMetadata;

    fn dependencies(&self) -> &[EvidenceId] {
        &self.metadata().dependencies
    }
}

impl EvidencePayload for EvidenceEntry {
    fn kind(&self) -> EvidenceKind {
        match self {
            Self::Position { .. } => EvidenceKind::Position,
            Self::EngineAnalysis { .. } => EvidenceKind::EngineAnalysis,
            Self::HumanMoveModel { .. } => EvidenceKind::HumanMoveModel,
            Self::Branch { .. } => EvidenceKind::Branch,
            Self::Provenance { .. } => EvidenceKind::Provenance,
        }
    }

    fn metadata(&self) -> &EvidenceMetadata {
        match self {
            Self::Position { metadata, .. }
            | Self::EngineAnalysis { metadata, .. }
            | Self::HumanMoveModel { metadata, .. }
            | Self::Branch { metadata, .. }
            | Self::Provenance { metadata, .. } => metadata,
        }
    }

    fn metadata_mut(&mut self) -> &mut EvidenceMetadata {
        match self {
            Self::Position { metadata, .. }
            | Self::EngineAnalysis { metadata, .. }
            | Self::HumanMoveModel { metadata, .. }
            | Self::Branch { metadata, .. }
            | Self::Provenance { metadata, .. } => metadata,
        }
    }
}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum EvidenceEntryWire {
    Position {
        metadata: EvidenceMetadata,
        position: PositionSnapshot,
    },
    EngineAnalysis {
        metadata: EvidenceMetadata,
        position_ref: PositionRef,
        analysis: EngineAnalysisEvidence,
    },
    HumanMoveModel {
        metadata: EvidenceMetadata,
        position_ref: PositionRef,
        analysis: HumanMoveModelEvidence,
    },
    Branch {
        metadata: EvidenceMetadata,
        branch: BranchEvidence,
    },
    Provenance {
        metadata: EvidenceMetadata,
        fact: ProvenanceFact,
    },
}

impl<'de> Deserialize<'de> for EvidenceEntry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let entry = match EvidenceEntryWire::deserialize(deserializer)? {
            EvidenceEntryWire::Position { metadata, position } => {
                Self::Position { metadata, position }
            }
            EvidenceEntryWire::EngineAnalysis {
                metadata,
                position_ref,
                analysis,
            } => Self::EngineAnalysis {
                metadata,
                position_ref,
                analysis,
            },
            EvidenceEntryWire::HumanMoveModel {
                metadata,
                position_ref,
                analysis,
            } => Self::HumanMoveModel {
                metadata,
                position_ref,
                analysis,
            },
            EvidenceEntryWire::Branch { metadata, branch } => Self::Branch { metadata, branch },
            EvidenceEntryWire::Provenance { metadata, fact } => Self::Provenance { metadata, fact },
        };
        if entry.has_valid_entry() {
            Ok(entry)
        } else {
            Err(de::Error::custom(
                "EvidenceEntry identity or contentDigest does not match its content",
            ))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvidenceMetadata {
    pub evidence_id: EvidenceId,
    pub dependencies: Vec<EvidenceId>,
    pub content_digest: ArtifactDigest,
    pub provenance: EvidenceProvenance,
}

impl EvidenceMetadata {
    pub fn pending(dependencies: Vec<EvidenceId>, provenance: EvidenceProvenance) -> Self {
        let zero_digest = format!("sha256:{}", "0".repeat(64));
        Self {
            evidence_id: EvidenceId::try_from(zero_digest.clone())
                .expect("the zero digest is a valid Evidence ID"),
            dependencies,
            content_digest: ArtifactDigest::try_from(zero_digest)
                .expect("the zero digest is a valid artifact digest"),
            provenance,
        }
    }

    pub fn derived(producer: &str, mut dependencies: Vec<EvidenceId>) -> Self {
        dependencies.sort();
        dependencies.dedup();
        Self::pending(
            dependencies,
            EvidenceProvenance::Derived {
                producer: producer.to_string(),
            },
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum EvidenceProvenance {
    Derived {
        producer: String,
    },
    Stockfish {
        version: String,
        binary_digest: ArtifactDigest,
        depth: u8,
        threads: u8,
        hash_mib: u16,
    },
    Maia {
        package: String,
        model: String,
        image: String,
        model_digest: ArtifactDigest,
        config_digest: ArtifactDigest,
        player_elo: EloRating,
        opponent_elo: EloRating,
        candidate_limit: u8,
    },
    Player,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EngineAnalysisEvidence {
    pub evaluation: EngineEvaluation,
    pub best_move_uci: String,
    pub principal_variation: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum EngineEvaluation {
    Centipawns {
        value: i32,
        perspective: Color,
    },
    Mate {
        outcome: MateOutcome,
        distance_plies: u16,
        perspective: Color,
    },
}

impl EngineEvaluation {
    /// The side the score is stated for. Two evaluations are only comparable
    /// when this agrees.
    pub fn perspective(&self) -> Color {
        match self {
            Self::Centipawns { perspective, .. } | Self::Mate { perspective, .. } => *perspective,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum MateOutcome {
    Win,
    Loss,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HumanMoveModelEvidence {
    pub player_elo: EloRating,
    pub opponent_elo: EloRating,
    pub candidates: Vec<HumanMoveCandidateEvidence>,
    pub win_probability: ProbabilityState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HumanMoveCandidateEvidence {
    pub uci: String,
    pub probability: Probability,
    #[schemars(range(min = 1))]
    pub rank: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, JsonSchema, TS)]
#[serde(transparent)]
#[schemars(transparent)]
pub struct Probability(#[schemars(range(min = 0.0, max = 1.0))] f64);

impl TryFrom<f64> for Probability {
    type Error = ContractValueError;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        if value.is_finite() && (0.0..=1.0).contains(&value) {
            Ok(Self(value))
        } else {
            Err(ContractValueError::new(
                "Probability",
                "must be finite and between zero and one",
            ))
        }
    }
}

impl<'de> Deserialize<'de> for Probability {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::try_from(f64::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

impl Probability {
    pub fn value(self) -> f64 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ProbabilityState {
    Available { probability: Probability },
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BranchEvidence {
    pub branch_ref: BranchRef,
    pub parent: BranchParent,
    pub source_position_ref: PositionRef,
    pub move_uci: String,
    pub resulting_position_ref: PositionRef,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum BranchParent {
    Root { position_ref: PositionRef },
    Move { branch_ref: BranchRef },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ProvenanceFact {
    Import { source: ImportProvenance },
    Provider { provider: EvidenceProvenance },
}

impl EvidenceEntry {
    pub fn position(metadata: EvidenceMetadata, position: PositionSnapshot) -> Self {
        Self::Position { metadata, position }.with_computed_identity()
    }

    pub fn engine_analysis(
        metadata: EvidenceMetadata,
        position_ref: PositionRef,
        analysis: EngineAnalysisEvidence,
    ) -> Self {
        Self::EngineAnalysis {
            metadata,
            position_ref,
            analysis,
        }
        .with_computed_identity()
    }

    pub fn human_move_model(
        metadata: EvidenceMetadata,
        position_ref: PositionRef,
        analysis: HumanMoveModelEvidence,
    ) -> Self {
        Self::HumanMoveModel {
            metadata,
            position_ref,
            analysis,
        }
        .with_computed_identity()
    }

    pub fn branch(metadata: EvidenceMetadata, branch: BranchEvidence) -> Self {
        Self::Branch { metadata, branch }.with_computed_identity()
    }

    pub fn provenance(metadata: EvidenceMetadata, fact: ProvenanceFact) -> Self {
        Self::Provenance { metadata, fact }.with_computed_identity()
    }

    pub fn kind(&self) -> EvidenceKind {
        EvidencePayload::kind(self)
    }

    pub fn metadata(&self) -> &EvidenceMetadata {
        EvidencePayload::metadata(self)
    }

    pub fn metadata_mut(&mut self) -> &mut EvidenceMetadata {
        EvidencePayload::metadata_mut(self)
    }

    pub fn with_computed_identity(mut self) -> Self {
        self.normalize_dependencies();
        let content_digest = self.computed_content_digest();
        let evidence_id = EvidenceId::try_from(content_digest.as_str().to_string())
            .expect("an artifact digest is a valid evidence ID");
        let metadata = self.metadata_mut();
        metadata.evidence_id = evidence_id;
        metadata.content_digest = content_digest;
        self
    }

    pub fn computed_content_digest(&self) -> ArtifactDigest {
        #[derive(Serialize)]
        #[serde(
            tag = "kind",
            rename_all = "camelCase",
            rename_all_fields = "camelCase"
        )]
        enum EvidenceContent<'a> {
            Position {
                dependencies: Vec<&'a EvidenceId>,
                provenance: &'a EvidenceProvenance,
                position: &'a PositionSnapshot,
            },
            EngineAnalysis {
                dependencies: Vec<&'a EvidenceId>,
                provenance: &'a EvidenceProvenance,
                position_ref: &'a PositionRef,
                analysis: &'a EngineAnalysisEvidence,
            },
            HumanMoveModel {
                dependencies: Vec<&'a EvidenceId>,
                provenance: &'a EvidenceProvenance,
                position_ref: &'a PositionRef,
                analysis: &'a HumanMoveModelEvidence,
            },
            Branch {
                dependencies: Vec<&'a EvidenceId>,
                provenance: &'a EvidenceProvenance,
                branch: &'a BranchEvidence,
            },
            Provenance {
                dependencies: Vec<&'a EvidenceId>,
                provenance: &'a EvidenceProvenance,
                fact: &'a ProvenanceFact,
            },
        }

        fn normalized(metadata: &EvidenceMetadata) -> (Vec<&EvidenceId>, &EvidenceProvenance) {
            let mut dependencies = metadata.dependencies.iter().collect::<Vec<_>>();
            dependencies.sort();
            (dependencies, &metadata.provenance)
        }
        let content = match self {
            Self::Position { metadata, position } => {
                let (dependencies, provenance) = normalized(metadata);
                EvidenceContent::Position {
                    dependencies,
                    provenance,
                    position,
                }
            }
            Self::EngineAnalysis {
                metadata,
                position_ref,
                analysis,
            } => {
                let (dependencies, provenance) = normalized(metadata);
                EvidenceContent::EngineAnalysis {
                    dependencies,
                    provenance,
                    position_ref,
                    analysis,
                }
            }
            Self::HumanMoveModel {
                metadata,
                position_ref,
                analysis,
            } => {
                let (dependencies, provenance) = normalized(metadata);
                EvidenceContent::HumanMoveModel {
                    dependencies,
                    provenance,
                    position_ref,
                    analysis,
                }
            }
            Self::Branch { metadata, branch } => {
                let (dependencies, provenance) = normalized(metadata);
                EvidenceContent::Branch {
                    dependencies,
                    provenance,
                    branch,
                }
            }
            Self::Provenance { metadata, fact } => {
                let (dependencies, provenance) = normalized(metadata);
                EvidenceContent::Provenance {
                    dependencies,
                    provenance,
                    fact,
                }
            }
        };
        ArtifactDigest::try_from(canonical_sha256(&content))
            .expect("SHA-256 output is a valid artifact digest")
    }

    pub fn has_valid_entry(&self) -> bool {
        self.has_normalized_dependencies()
            && self.has_valid_semantic_ranges()
            && self.has_valid_content_digest()
            && self.identity_matches_content_digest()
    }

    fn has_valid_semantic_ranges(&self) -> bool {
        match self {
            Self::HumanMoveModel { analysis, .. } => analysis
                .candidates
                .iter()
                .all(|candidate| candidate.rank > 0),
            Self::Position { .. }
            | Self::EngineAnalysis { .. }
            | Self::Branch { .. }
            | Self::Provenance { .. } => true,
        }
    }

    fn has_valid_content_digest(&self) -> bool {
        self.metadata().content_digest == self.computed_content_digest()
    }

    fn identity_matches_content_digest(&self) -> bool {
        self.metadata().evidence_id.as_str() == self.metadata().content_digest.as_str()
    }

    fn has_normalized_dependencies(&self) -> bool {
        self.dependencies().windows(2).all(|pair| pair[0] < pair[1])
    }

    fn normalize_dependencies(&mut self) {
        let dependencies = &mut self.metadata_mut().dependencies;
        dependencies.sort();
        dependencies.dedup();
    }
}

impl EngineEvaluation {
    pub fn has_valid_mate_zero_context(&self, status: &PositionStatus) -> bool {
        !matches!(
            self,
            Self::Mate {
                distance_plies: 0,
                ..
            }
        ) || matches!(status, PositionStatus::Checkmate { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_entry_construction_normalizes_and_validates_identity() {
        let first = EvidenceId::try_from(format!("sha256:{}", "1".repeat(64))).unwrap();
        let second = EvidenceId::try_from(format!("sha256:{}", "2".repeat(64))).unwrap();
        let position = build_position_snapshot(STANDARD_STARTING_FEN, &[]).unwrap();
        let entry = EvidenceEntry::position(
            EvidenceMetadata::derived("test", vec![second.clone(), first.clone(), second.clone()]),
            position.clone(),
        );

        assert_eq!(entry.kind(), EvidenceKind::Position);
        assert_eq!(entry.dependencies(), [first, second]);
        assert_eq!(
            entry.metadata().evidence_id.as_str(),
            entry.metadata().content_digest.as_str()
        );

        let encoded = serde_json::to_string(&entry).unwrap();
        let decoded = serde_json::from_str::<EvidenceEntry>(&encoded).unwrap();
        assert_eq!(decoded, entry);

        let packet = ReviewSessionEvidencePacket {
            entries: vec![entry.clone()],
        };
        assert_eq!(
            packet.position_evidence_id(&position.position_ref),
            Some(entry.metadata().evidence_id.clone())
        );
        assert!(packet.contains(&entry.metadata().evidence_id));
    }
}
