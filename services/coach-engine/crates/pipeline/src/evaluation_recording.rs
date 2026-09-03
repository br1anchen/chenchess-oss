use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{de, Deserialize, Deserializer, Serialize};
use sha2::Digest;
use shakmaty::{fen::Fen, uci::UciMove, CastlingMode, Chess, EnPassantMode, Position};
use ts_rs::TS;

use crate::{pgn::parse_pgn, review_session_contract::*};

pub const REVIEW_SESSION_CAPTURE_VERSION: &str = "review-session-provider-capture/v1";
pub const PROVIDER_RECORDING_SCHEMA_VERSION: u8 = 1;
pub const PINNED_STOCKFISH_VERSION: &str = "18";
pub const PINNED_STOCKFISH_DEPTH: u8 = 16;
pub const PINNED_STOCKFISH_THREADS: u8 = 1;
pub const PINNED_STOCKFISH_HASH_MIB: u16 = 16;
pub const PINNED_STOCKFISH_BINARY_DIGEST: &str =
    "sha256:bc0cac905ecdf2147fe22055c733bcd999b1e3f7c399fbaf7fb9055786563590";
pub const PINNED_STOCKFISH_LINUX_X86_64_BINARY_DIGEST: &str =
    "sha256:7a44d64fd877ee888a5160349827563444e1935ca6c1095d0f8e0859d57101c7";
pub const PINNED_STOCKFISH_LINUX_X86_64_AVX2_BINARY_DIGEST: &str =
    "sha256:6b087694916228c905a5e14db74cca8c7e5643602226af1fa5d42353c455b9f9";
pub const PINNED_STOCKFISH_BINARY_DIGESTS: [&str; 3] = [
    PINNED_STOCKFISH_BINARY_DIGEST,
    PINNED_STOCKFISH_LINUX_X86_64_BINARY_DIGEST,
    PINNED_STOCKFISH_LINUX_X86_64_AVX2_BINARY_DIGEST,
];
pub const PINNED_MAIA_PACKAGE: &str = "maia2==0.11.0";
pub const PINNED_MAIA_MODEL: &str = "rapid";
pub const PINNED_MAIA_ELO: u16 = 1246;
pub const PINNED_MAIA_CANDIDATE_LIMIT: u8 = 5;
pub const PINNED_MAIA_IMAGE: &str =
    "maia-runtime@sha256:ab3b6dc16b75c3602f2e6c4002dc0f99ef77c8c042641cffea66fc1c23482972";
pub const PINNED_MAIA_MODEL_DIGEST: &str =
    "sha256:65aae8465eed5e65df66a24ea7370715579f9e5435098d06fe18bdb1e267e997";
pub const PINNED_MAIA_CONFIG_DIGEST: &str =
    "sha256:4b06a5e6917dba8a55defaf3947ce97a73edca3ae2c9d225779a620353c1371b";
/// The workspace lockfile the stored recording was captured under.
///
/// It pins the chess and canonicalisation crates that decide move legality and
/// digest bytes, so a recording made under different resolutions is not
/// comparable to this one. It therefore has to move whenever the recording is
/// re-captured, and only then.
pub const PINNED_CARGO_LOCK_DIGEST: &str =
    "sha256:358337356ae8963f93900e4d03d28c84fb492d408f8be1fb1cc2ff9f136fc8d3";
pub const CANONICAL_GAME_PGN_DIGEST: &str =
    "sha256:7eb4b0803c2b4fca8d80b3968928fe856bf15999626a402d9651694c0e80c799";
/// Digest of `packages/coach-engine-sdk/fixtures/imported-game.json`, which the
/// recording capture reads as the canonical Imported Game.
///
/// It moves whenever the `ImportedGame` contract changes shape, because it
/// digests the struct rather than the Game. The Game itself is pinned
/// separately by [`CANONICAL_GAME_PGN_DIGEST`], which is what actually says
/// "this is still Synthet1"; this one says "and it still serialises the way the
/// stored recording was written from".
pub const CANONICAL_IMPORTED_GAME_DIGEST: &str =
    "sha256:817cc80c69e2bde653e730dae696e6dd510f2d51c6cadaca386689f88e2e86a0";
const CANONICAL_ROOT_POSITION_REF: &str =
    "sha256:2610ba70a678561b980caacafd81655b5a63429ec573014191fd5520631598ec";
const CANONICAL_PROJECTED_PLAN_PREFIX: &str = "branch:projected-plan:a8a7";
const CANONICAL_ALTERNATIVE_BRANCHES: [(&str, &str); 4] = [
    ("branch:capture:d7b6", "d7b6"),
    ("branch:capture:d7b6:c4e2", "c4e2"),
    ("branch:capture:e7e6", "e7e6"),
    ("branch:capture:e7e6:g5f3", "g5f3"),
];

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewSessionProviderRecording {
    pub schema_version: u8,
    pub content: ProviderRecordingContent,
    pub content_digest: ArtifactDigest,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProviderRecordingWire {
    schema_version: u8,
    content: ProviderRecordingContent,
    content_digest: ArtifactDigest,
}

impl<'de> Deserialize<'de> for ReviewSessionProviderRecording {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ProviderRecordingWire::deserialize(deserializer)?;
        let recording = Self {
            schema_version: wire.schema_version,
            content: wire.content,
            content_digest: wire.content_digest,
        };
        recording.verify().map_err(de::Error::custom)?;
        Ok(recording)
    }
}

impl ReviewSessionProviderRecording {
    pub fn from_content(content: ProviderRecordingContent) -> Result<Self, RecordingError> {
        let content_digest = ArtifactDigest::try_from(canonical_sha256(&content))
            .expect("canonical recording content has a valid SHA-256 digest");
        let recording = Self {
            schema_version: PROVIDER_RECORDING_SCHEMA_VERSION,
            content,
            content_digest,
        };
        recording.verify()?;
        Ok(recording)
    }

    pub fn verify(&self) -> Result<(), RecordingError> {
        require(
            self.schema_version == PROVIDER_RECORDING_SCHEMA_VERSION,
            RecordingError::ProvenanceDrift("schema version"),
        )?;
        let expected = ArtifactDigest::try_from(canonical_sha256(&self.content))
            .expect("canonical recording content has a valid SHA-256 digest");
        require(
            self.content_digest == expected,
            RecordingError::DigestMismatch,
        )?;
        verify_identity(&self.content)?;
        verify_entries(&self.content)
    }

    pub fn replay(&self) -> Result<VerifiedProviderReplay<'_>, RecordingError> {
        self.verify()?;
        Ok(VerifiedProviderReplay {
            content: &self.content,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderRecordingContent {
    pub captured_at: String,
    pub canonical_game_id: CanonicalGameId,
    pub canonical_source_url: String,
    pub game_ref: GameRef,
    pub pgn_digest: ArtifactDigest,
    pub imported_game_digest: ImportedGameDigest,
    pub runtime: ProviderRecordingRuntime,
    pub entries: Vec<EvidenceEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderRecordingRuntime {
    pub stockfish: StockfishRecordingRuntime,
    pub maia: MaiaRecordingRuntime,
    pub dependencies: Vec<RecordingDependency>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StockfishRecordingRuntime {
    pub version: String,
    pub binary_digest: ArtifactDigest,
    pub depth: u8,
    pub threads: u8,
    pub hash_mib: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MaiaRecordingRuntime {
    pub package: String,
    pub model: String,
    pub image: String,
    pub model_digest: ArtifactDigest,
    pub config_digest: ArtifactDigest,
    pub player_elo: EloRating,
    pub opponent_elo: EloRating,
    pub candidate_limit: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecordingDependency {
    pub name: String,
    pub version: String,
    pub digest: DependencyDigest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum DependencyDigest {
    Artifact { digest: ArtifactDigest },
    Lockfile { digest: ArtifactDigest },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RecordingError {
    #[error("recording content digest does not match its content")]
    DigestMismatch,
    #[error("recording schema, source, or provider identity drifted: {0}")]
    ProvenanceDrift(&'static str),
    #[error("recording contains conflicting or missing evidence: {0}")]
    EvidenceConflict(&'static str),
    #[error("recording contains an invalid Position: {0}")]
    InvalidPosition(&'static str),
    #[error("recording contains an illegal move or line: {0}")]
    IllegalMove(&'static str),
    #[error("recording contains an invalid probability or rank")]
    InvalidProbability,
}

pub struct VerifiedProviderReplay<'a> {
    content: &'a ProviderRecordingContent,
}

pub struct VerifiedLichessReplay {
    pgn: String,
}

impl VerifiedLichessReplay {
    pub fn from_canonical_pgn(bytes: &[u8]) -> Result<Self, RecordingError> {
        let digest = format!("sha256:{:x}", sha2::Sha256::digest(bytes));
        require(
            digest == CANONICAL_GAME_PGN_DIGEST,
            RecordingError::ProvenanceDrift("Lichess export digest"),
        )?;
        let pgn = std::str::from_utf8(bytes)
            .map_err(|_| RecordingError::ProvenanceDrift("Lichess export encoding"))?
            .to_string();
        let game =
            parse_pgn(&pgn).map_err(|_| RecordingError::ProvenanceDrift("Lichess export Game"))?;
        require(
            game.moves.len() == 90 && game.is_terminal && game.result.as_deref() == Some("0-1"),
            RecordingError::ProvenanceDrift("Lichess export Game facts"),
        )?;
        Ok(Self { pgn })
    }

    pub fn respond(&self) -> &str {
        self.pgn.as_str()
    }
}

impl VerifiedProviderReplay<'_> {
    pub fn stockfish(
        &self,
        position_ref: &PositionRef,
    ) -> Result<&EngineAnalysisEvidence, ProviderReplayError> {
        self.content
            .entries
            .iter()
            .find_map(|entry| match entry {
                EvidenceEntry::EngineAnalysis {
                    position_ref: recorded_ref,
                    analysis,
                    ..
                } if recorded_ref == position_ref => Some(analysis),
                _ => None,
            })
            .ok_or(ProviderReplayError::MissingPosition)
    }

    pub fn maia(
        &self,
        position_ref: &PositionRef,
        elo: EloRating,
    ) -> Result<&HumanMoveModelEvidence, ProviderReplayError> {
        self.content
            .entries
            .iter()
            .find_map(|entry| match entry {
                EvidenceEntry::HumanMoveModel {
                    position_ref: recorded_ref,
                    analysis,
                    ..
                } if recorded_ref == position_ref
                    && analysis.player_elo == elo
                    && analysis.opponent_elo == elo =>
                {
                    Some(analysis)
                }
                _ => None,
            })
            .ok_or(ProviderReplayError::MissingPosition)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProviderReplayError {
    #[error("verified recording has no observation for the requested Position")]
    MissingPosition,
}

fn verify_identity(content: &ProviderRecordingContent) -> Result<(), RecordingError> {
    /* One `require` per field, because the answer a re-capture needs is *which*
    identity moved. Folded into a single conjunction this reported only
    "canonical source", which is true of six different failures and actionable
    for none of them. */
    require(
        content.canonical_game_id.as_str() == "Synthet1",
        RecordingError::ProvenanceDrift("canonical game id"),
    )?;
    require(
        content.canonical_source_url == "https://lichess.org/Synthet1",
        RecordingError::ProvenanceDrift("canonical source URL"),
    )?;
    require(
        content.pgn_digest.as_str() == CANONICAL_GAME_PGN_DIGEST,
        RecordingError::ProvenanceDrift("canonical PGN digest"),
    )?;
    require(
        content.game_ref.as_str() == CANONICAL_GAME_PGN_DIGEST,
        RecordingError::ProvenanceDrift("canonical Game ref"),
    )?;
    require(
        content.imported_game_digest.as_str() == CANONICAL_IMPORTED_GAME_DIGEST,
        RecordingError::ProvenanceDrift("canonical Imported Game digest"),
    )?;
    require(
        is_canonical_capture_timestamp(&content.captured_at),
        RecordingError::ProvenanceDrift("capture timestamp"),
    )?;
    let stockfish = &content.runtime.stockfish;
    require(
        stockfish.version == PINNED_STOCKFISH_VERSION
            && stockfish.depth == PINNED_STOCKFISH_DEPTH
            && stockfish.threads == PINNED_STOCKFISH_THREADS
            && stockfish.hash_mib == PINNED_STOCKFISH_HASH_MIB
            && stockfish.binary_digest.as_str() == PINNED_STOCKFISH_BINARY_DIGEST,
        RecordingError::ProvenanceDrift("Stockfish identity or settings"),
    )?;
    let maia = &content.runtime.maia;
    require(
        maia.package == PINNED_MAIA_PACKAGE
            && maia.model == PINNED_MAIA_MODEL
            && maia.image == PINNED_MAIA_IMAGE
            && maia.model_digest.as_str() == PINNED_MAIA_MODEL_DIGEST
            && maia.config_digest.as_str() == PINNED_MAIA_CONFIG_DIGEST
            && maia.player_elo.value() == PINNED_MAIA_ELO
            && maia.opponent_elo.value() == PINNED_MAIA_ELO
            && maia.candidate_limit == PINNED_MAIA_CANDIDATE_LIMIT,
        RecordingError::ProvenanceDrift("Maia identity, Elo, or settings"),
    )?;
    let dependency_names = content
        .runtime
        .dependencies
        .iter()
        .map(|dependency| dependency.name.as_str())
        .collect::<BTreeSet<_>>();
    require(
        dependency_names.len() == content.runtime.dependencies.len()
            && content
                .runtime
                .dependencies
                .windows(2)
                .all(|pair| pair[0].name < pair[1].name)
            && dependency_names.contains("shakmaty")
            && dependency_names.contains("serde_json_canonicalizer")
            && content.runtime.dependencies.iter().all(|dependency| {
                let version_matches = match dependency.name.as_str() {
                    "shakmaty" => dependency.version == "0.27",
                    "serde_json_canonicalizer" => dependency.version == "0.3.2",
                    _ => false,
                };
                version_matches
                    && matches!(
                        &dependency.digest,
                        DependencyDigest::Lockfile { digest }
                            if digest.as_str() == PINNED_CARGO_LOCK_DIGEST
                    )
            }),
        RecordingError::ProvenanceDrift("capture dependencies"),
    )
}

fn is_canonical_capture_timestamp(value: &str) -> bool {
    chrono::DateTime::parse_from_rfc3339(value).is_ok_and(|timestamp| {
        timestamp
            .to_utc()
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
            == value
    })
}

fn verify_entries(content: &ProviderRecordingContent) -> Result<(), RecordingError> {
    let mut positions = BTreeMap::new();
    let mut position_evidence = BTreeMap::new();
    let mut observation_keys = BTreeSet::new();
    let mut branches = BTreeMap::new();
    let evidence_ids = content
        .entries
        .iter()
        .map(|entry| &entry.metadata().evidence_id)
        .collect::<BTreeSet<_>>();
    for entry in &content.entries {
        let value = serde_json::to_value(entry)
            .map_err(|_| RecordingError::EvidenceConflict("unserializable evidence"))?;
        serde_json::from_value::<EvidenceEntry>(value).map_err(|_| match entry {
            EvidenceEntry::Position { .. } => {
                RecordingError::InvalidPosition("Position fields contradict their FEN")
            }
            _ => RecordingError::EvidenceConflict("evidence identity or semantics"),
        })?;
        if let EvidenceEntry::Position { metadata, position } = entry {
            require(
                positions
                    .insert(position.position_ref.clone(), position)
                    .is_none(),
                RecordingError::EvidenceConflict("duplicate Position identity"),
            )?;
            position_evidence.insert(position.position_ref.clone(), &metadata.evidence_id);
        }
        if let EvidenceEntry::Branch { branch, .. } = entry {
            require(
                branches.insert(branch.branch_ref.clone(), branch).is_none(),
                RecordingError::EvidenceConflict("duplicate branch identity"),
            )?;
        }
    }
    require(
        evidence_ids.len() == content.entries.len()
            && content.entries.iter().all(|entry| {
                entry.metadata().dependencies.iter().all(|dependency| {
                    dependency != &entry.metadata().evidence_id && evidence_ids.contains(dependency)
                })
            }),
        RecordingError::EvidenceConflict("evidence identity or dependencies"),
    )?;
    for entry in &content.entries {
        match entry {
            EvidenceEntry::Position { metadata, position } => {
                parse_position(position)?;
                require(
                    matches!(
                        &metadata.provenance,
                        EvidenceProvenance::Derived {
                            producer,
                        } if producer == REVIEW_SESSION_CAPTURE_VERSION
                    ),
                    RecordingError::ProvenanceDrift("Position evidence"),
                )?;
            }
            EvidenceEntry::EngineAnalysis {
                metadata,
                position_ref,
                analysis,
            } => {
                require_unique_observation(&mut observation_keys, "stockfish", position_ref)?;
                let position = required_position(&positions, position_ref)?;
                require_position_dependency(metadata, &position_evidence, position_ref)?;
                verify_stockfish_provenance(metadata, content)?;
                verify_engine_analysis(position, analysis)?;
            }
            EvidenceEntry::HumanMoveModel {
                metadata,
                position_ref,
                analysis,
            } => {
                require_unique_observation(&mut observation_keys, "maia", position_ref)?;
                let position = required_position(&positions, position_ref)?;
                require_position_dependency(metadata, &position_evidence, position_ref)?;
                verify_maia_provenance(metadata, content)?;
                verify_human_analysis(position, analysis, &content.runtime.maia)?;
            }
            EvidenceEntry::Branch { metadata, branch } => {
                verify_branch(metadata, branch, &positions, &position_evidence, &branches)?;
            }
            EvidenceEntry::Provenance { .. } => {
                return Err(RecordingError::EvidenceConflict(
                    "provider recordings may contain only Position, provider, and branch evidence",
                ));
            }
        }
    }
    verify_canonical_capture_set(&positions, &branches)?;
    require(
        observation_keys
            .iter()
            .any(|(provider, _)| *provider == "stockfish")
            && observation_keys
                .iter()
                .any(|(provider, _)| *provider == "maia")
            && observation_keys.len() == positions.len() * 2
            && !branches.is_empty(),
        RecordingError::EvidenceConflict("recording is missing a provider or branch"),
    )?;
    verify_branch_graph(&branches)
}

fn verify_canonical_capture_set(
    positions: &BTreeMap<PositionRef, &PositionSnapshot>,
    branches: &BTreeMap<BranchRef, &BranchEvidence>,
) -> Result<(), RecordingError> {
    let branch_positions = branches
        .values()
        .map(|branch| &branch.resulting_position_ref)
        .collect::<BTreeSet<_>>();
    require(
        positions.len() == 18
            && positions
                .keys()
                .any(|position_ref| position_ref.as_str() == CANONICAL_ROOT_POSITION_REF)
            && branch_positions.len() == positions.len() - 1
            && positions.keys().all(|position_ref| {
                position_ref.as_str() == CANONICAL_ROOT_POSITION_REF
                    || branch_positions.contains(position_ref)
            }),
        RecordingError::ProvenanceDrift("canonical Position set"),
    )?;
    let projected_plan_depths = branches
        .values()
        .filter(|branch| is_canonical_projected_plan_branch(branch.branch_ref.as_str()))
        .map(|branch| branch.branch_ref.as_str().split(':').count() - 2)
        .fold(BTreeMap::new(), |mut counts, depth| {
            *counts.entry(depth).or_insert(0usize) += 1;
            counts
        });
    let reviewed_move = branches.get(
        &BranchRef::try_from(CANONICAL_PROJECTED_PLAN_PREFIX.to_string()).expect("valid branch ID"),
    );
    require(
        branches.len() == 17
            && branches.values().all(|branch| {
                is_canonical_projected_plan_branch(branch.branch_ref.as_str())
                    || CANONICAL_ALTERNATIVE_BRANCHES
                        .contains(&(branch.branch_ref.as_str(), branch.move_uci.as_str()))
            })
            && reviewed_move.is_some_and(|branch| {
                branch.move_uci == "a8a7"
                    && matches!(
                        &branch.parent,
                        BranchParent::Root { position_ref }
                            if position_ref.as_str() == CANONICAL_ROOT_POSITION_REF
                    )
            })
            && projected_plan_depths == BTreeMap::from([(1, 1), (2, 3), (3, 3), (4, 3), (5, 3)]),
        RecordingError::ProvenanceDrift("Projected Plan branch set"),
    )
}

fn is_canonical_projected_plan_branch(branch_ref: &str) -> bool {
    branch_ref == CANONICAL_PROJECTED_PLAN_PREFIX
        || branch_ref
            .strip_prefix(CANONICAL_PROJECTED_PLAN_PREFIX)
            .is_some_and(|suffix| suffix.starts_with(':'))
}

fn verify_stockfish_provenance(
    metadata: &EvidenceMetadata,
    content: &ProviderRecordingContent,
) -> Result<(), RecordingError> {
    let runtime = &content.runtime.stockfish;
    let matches = matches!(
        &metadata.provenance,
        EvidenceProvenance::Stockfish {
            version,
            binary_digest,
            depth,
            threads,
            hash_mib,
        } if version == &runtime.version
            && binary_digest == &runtime.binary_digest
            && depth == &runtime.depth
            && threads == &runtime.threads
            && hash_mib == &runtime.hash_mib
    );
    require(
        matches,
        RecordingError::ProvenanceDrift("Stockfish evidence"),
    )
}

fn verify_maia_provenance(
    metadata: &EvidenceMetadata,
    content: &ProviderRecordingContent,
) -> Result<(), RecordingError> {
    let runtime = &content.runtime.maia;
    let matches = matches!(
        &metadata.provenance,
        EvidenceProvenance::Maia {
            package,
            model,
            image,
            model_digest,
            config_digest,
            player_elo,
            opponent_elo,
            candidate_limit,
        } if package == &runtime.package
            && model == &runtime.model
            && image == &runtime.image
            && model_digest == &runtime.model_digest
            && config_digest == &runtime.config_digest
            && player_elo == &runtime.player_elo
            && opponent_elo == &runtime.opponent_elo
            && candidate_limit == &runtime.candidate_limit
    );
    require(matches, RecordingError::ProvenanceDrift("Maia evidence"))
}

fn verify_engine_analysis(
    position: &PositionSnapshot,
    analysis: &EngineAnalysisEvidence,
) -> Result<(), RecordingError> {
    let chess = parse_position(position)?;
    require(
        evaluation_perspective(&analysis.evaluation) == position.side_to_move,
        RecordingError::ProvenanceDrift("Stockfish evaluation perspective"),
    )?;
    require(
        analysis
            .evaluation
            .has_valid_mate_zero_context(&position.status),
        RecordingError::ProvenanceDrift("Stockfish mate distance"),
    )?;
    if !matches!(position.status, PositionStatus::Ongoing { .. }) {
        return verify_terminal_analysis(position, analysis);
    }
    require(
        analysis.principal_variation.first() == Some(&analysis.best_move_uci),
        RecordingError::IllegalMove("best move is not first in principal variation"),
    )?;
    verify_legal_line(chess, &analysis.principal_variation)
}

fn verify_terminal_analysis(
    position: &PositionSnapshot,
    analysis: &EngineAnalysisEvidence,
) -> Result<(), RecordingError> {
    require(
        analysis.best_move_uci == "0000" && analysis.principal_variation.is_empty(),
        RecordingError::IllegalMove("terminal Position exposes a reply"),
    )?;
    let expected = match position.status {
        PositionStatus::Checkmate { .. } => EngineEvaluation::Mate {
            outcome: MateOutcome::Loss,
            distance_plies: 0,
            perspective: position.side_to_move,
        },
        PositionStatus::Draw { .. } => EngineEvaluation::Centipawns {
            value: 0,
            perspective: position.side_to_move,
        },
        PositionStatus::Ongoing { .. } => unreachable!("caller checked terminal status"),
    };
    require(
        analysis.evaluation == expected,
        RecordingError::ProvenanceDrift("terminal Stockfish evaluation"),
    )
}

fn verify_human_analysis(
    position: &PositionSnapshot,
    analysis: &HumanMoveModelEvidence,
    runtime: &MaiaRecordingRuntime,
) -> Result<(), RecordingError> {
    require(
        analysis.player_elo == runtime.player_elo
            && analysis.opponent_elo == runtime.opponent_elo
            && !analysis.candidates.is_empty()
            && analysis.candidates.len() <= usize::from(runtime.candidate_limit),
        RecordingError::InvalidProbability,
    )?;
    let chess = parse_position(position)?;
    let mut previous_probability = 1.0;
    let mut moves = BTreeSet::new();
    for (index, candidate) in analysis.candidates.iter().enumerate() {
        let probability = candidate.probability.value();
        require(
            candidate.rank == u8::try_from(index + 1).unwrap_or(u8::MAX)
                && probability <= previous_probability
                && moves.insert(candidate.uci.as_str()),
            RecordingError::InvalidProbability,
        )?;
        previous_probability = probability;
        verify_legal_move(&chess, &candidate.uci)?;
    }
    Ok(())
}

fn verify_branch<'a>(
    metadata: &EvidenceMetadata,
    branch: &BranchEvidence,
    positions: &BTreeMap<PositionRef, &'a PositionSnapshot>,
    position_evidence: &BTreeMap<PositionRef, &'a EvidenceId>,
    branches: &BTreeMap<BranchRef, &'a BranchEvidence>,
) -> Result<(), RecordingError> {
    let source = required_position(positions, &branch.source_position_ref)?;
    let resulting = required_position(positions, &branch.resulting_position_ref)?;
    let source_evidence = position_evidence
        .get(&branch.source_position_ref)
        .ok_or(RecordingError::EvidenceConflict("branch source evidence"))?;
    let resulting_evidence = position_evidence
        .get(&branch.resulting_position_ref)
        .ok_or(RecordingError::EvidenceConflict("branch result evidence"))?;
    require(
        metadata.dependencies.contains(source_evidence)
            && metadata.dependencies.contains(resulting_evidence),
        RecordingError::EvidenceConflict("branch Position dependencies"),
    )?;
    match &branch.parent {
        BranchParent::Root { position_ref } => require(
            position_ref == &branch.source_position_ref,
            RecordingError::EvidenceConflict("branch root Position"),
        )?,
        BranchParent::Move { branch_ref } => {
            let parent = branches
                .get(branch_ref)
                .ok_or(RecordingError::EvidenceConflict("branch parent"))?;
            require(
                branch_ref != &branch.branch_ref
                    && parent.resulting_position_ref == branch.source_position_ref,
                RecordingError::EvidenceConflict("branch parent Position"),
            )?;
        }
    }
    require(
        matches!(
            &metadata.provenance,
            EvidenceProvenance::Derived {
                producer,
            } if producer == REVIEW_SESSION_CAPTURE_VERSION
        ),
        RecordingError::ProvenanceDrift("branch evidence"),
    )?;
    let mut chess = parse_position(source)?;
    let chess_move = legal_move(&chess, &branch.move_uci)?;
    chess.play_unchecked(&chess_move);
    let resulting_fen = Fen::from_position(chess, EnPassantMode::Legal).to_string();
    require(
        resulting_fen == resulting.fen,
        RecordingError::InvalidPosition("branch result does not match its move"),
    )
}

fn verify_branch_graph(
    branches: &BTreeMap<BranchRef, &BranchEvidence>,
) -> Result<(), RecordingError> {
    for branch in branches.values() {
        let mut current = *branch;
        let mut visited = BTreeSet::new();
        for depth in 0..=12 {
            require(
                visited.insert(&current.branch_ref),
                RecordingError::EvidenceConflict("cyclic branch graph"),
            )?;
            match &current.parent {
                BranchParent::Root { .. } => break,
                BranchParent::Move { branch_ref } => {
                    current = branches
                        .get(branch_ref)
                        .copied()
                        .ok_or(RecordingError::EvidenceConflict("branch parent"))?;
                    if depth == 12 {
                        return Err(RecordingError::EvidenceConflict(
                            "branch exceeds the 12-ply recording limit",
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

fn require_position_dependency(
    metadata: &EvidenceMetadata,
    position_evidence: &BTreeMap<PositionRef, &EvidenceId>,
    position_ref: &PositionRef,
) -> Result<(), RecordingError> {
    let evidence_id =
        position_evidence
            .get(position_ref)
            .ok_or(RecordingError::EvidenceConflict(
                "provider Position evidence",
            ))?;
    require(
        metadata.dependencies.contains(evidence_id),
        RecordingError::EvidenceConflict("provider Position dependency"),
    )
}

fn required_position<'a>(
    positions: &BTreeMap<PositionRef, &'a PositionSnapshot>,
    position_ref: &PositionRef,
) -> Result<&'a PositionSnapshot, RecordingError> {
    positions
        .get(position_ref)
        .copied()
        .ok_or(RecordingError::EvidenceConflict(
            "unknown Position reference",
        ))
}

fn require_unique_observation(
    keys: &mut BTreeSet<(&'static str, PositionRef)>,
    provider: &'static str,
    position_ref: &PositionRef,
) -> Result<(), RecordingError> {
    require(
        keys.insert((provider, position_ref.clone())),
        RecordingError::EvidenceConflict("duplicate provider observation"),
    )
}

fn parse_position(snapshot: &PositionSnapshot) -> Result<Chess, RecordingError> {
    Fen::from_ascii(snapshot.fen.as_bytes())
        .map_err(|_| RecordingError::InvalidPosition("malformed FEN"))?
        .into_position(CastlingMode::Standard)
        .map_err(|_| RecordingError::InvalidPosition("illegal standard-chess FEN"))
}

fn verify_legal_line(mut position: Chess, line: &[String]) -> Result<(), RecordingError> {
    require(
        !line.is_empty(),
        RecordingError::IllegalMove("empty Stockfish principal variation"),
    )?;
    for uci in line {
        let chess_move = legal_move(&position, uci)?;
        position.play_unchecked(&chess_move);
    }
    Ok(())
}

fn verify_legal_move(position: &Chess, uci: &str) -> Result<(), RecordingError> {
    legal_move(position, uci).map(|_| ())
}

fn legal_move(position: &Chess, uci: &str) -> Result<shakmaty::Move, RecordingError> {
    UciMove::from_ascii(uci.as_bytes())
        .map_err(|_| RecordingError::IllegalMove("malformed UCI"))?
        .to_move(position)
        .map_err(|_| RecordingError::IllegalMove("move is illegal in its recorded Position"))
}

fn evaluation_perspective(evaluation: &EngineEvaluation) -> Color {
    match evaluation {
        EngineEvaluation::Centipawns { perspective, .. }
        | EngineEvaluation::Mate { perspective, .. } => *perspective,
    }
}

fn require(condition: bool, error: RecordingError) -> Result<(), RecordingError> {
    if condition {
        Ok(())
    } else {
        Err(error)
    }
}

fn canonical_sha256(value: &impl Serialize) -> String {
    let encoded = serde_json_canonicalizer::to_vec(value)
        .expect("provider recording content is canonically serializable");
    format!("sha256:{:x}", sha2::Sha256::digest(encoded))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_projected_plan_prefix_requires_a_branch_boundary() {
        assert!(is_canonical_projected_plan_branch(
            CANONICAL_PROJECTED_PLAN_PREFIX
        ));
        assert!(is_canonical_projected_plan_branch(
            "branch:projected-plan:a8a7:b8d7"
        ));
        assert!(!is_canonical_projected_plan_branch(
            "branch:projected-plan:a8a70"
        ));
    }
}
