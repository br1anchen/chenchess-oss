use std::fmt;

use schemars::JsonSchema;
use serde::{de, Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;

use super::evidence::ReviewSessionEvidencePacket;
use super::learning::ReviewMomentLearningMaterial;
use super::opening::OpeningMetadata;

macro_rules! semantic_id {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, JsonSchema, TS)]
        #[serde(transparent)]
        #[schemars(transparent)]
        pub struct $name(
            #[schemars(regex(pattern = r"^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$"))] String,
        );

        impl TryFrom<String> for $name {
            type Error = ContractValueError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                validate_semantic_id(stringify!($name), &value)?;
                Ok(Self(value))
            }
        }

        impl $name {
            pub fn as_str(&self) -> &str {
                &self.0
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

macro_rules! content_digest {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, JsonSchema, TS)]
        #[serde(transparent)]
        #[schemars(transparent)]
        pub struct $name(#[schemars(regex(pattern = r"^sha256:[0-9a-f]{64}$"))] String);

        impl TryFrom<String> for $name {
            type Error = ContractValueError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                validate_content_digest(stringify!($name), &value)?;
                Ok(Self(value))
            }
        }

        impl $name {
            pub fn as_str(&self) -> &str {
                &self.0
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

semantic_id!(RequestId);
semantic_id!(OperationId);
semantic_id!(CoachTurnId);
semantic_id!(AlternativeMoveId);
semantic_id!(BranchRef);
semantic_id!(CriticalMomentId);
semantic_id!(GameImportId);
semantic_id!(CanonicalGameId);
semantic_id!(IdempotencyKey);
semantic_id!(MoveSequenceRef);
semantic_id!(LearningResourceId);
semantic_id!(LearningResourceMappingId);
semantic_id!(LearningPathRef);

content_digest!(PositionRef);
content_digest!(EvidenceId);
content_digest!(GameRef);
content_digest!(ImportedGameDigest);
content_digest!(ArtifactDigest);
content_digest!(
    /// Identifies exactly what a Game Review read would answer with.
    ///
    /// A client cache holds bytes it cannot otherwise date: the frozen review
    /// is immutable, but the *derivation* over it is not, and neither is the
    /// comment template a Review Moment's prose was written from. This digest
    /// folds both, so a caller can offer what it already holds and be told it
    /// is still current instead of being sent the payload again.
    ReviewContentDigest
);

impl ReviewContentDigest {
    /// Digests exactly the content a read would answer with.
    ///
    /// Deliberately taken over the answer rather than folded from the inputs
    /// that produce it. Folding inputs cannot see a Review Moment Comment: the
    /// wire comment is `{ text }` and carries no identity, so a comment
    /// published after a caller cached the moment would leave every input
    /// digest unchanged and the caller would hold an empty comment forever.
    /// Digesting the answer also cannot drift from what is actually sent, and
    /// it cannot over-invalidate — editing the comment prompt does not disturb
    /// a cached Game Review snapshot, which carries no prose at all.
    ///
    /// The engine has to build the answer before it can digest it, so what a
    /// revalidation saves is transfer and client parsing, never the work.
    pub fn of_answer(answer: &impl Serialize) -> Self {
        Self::try_from(canonical_sha256(answer))
            .expect("a canonical sha256 is a valid content digest")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, JsonSchema, TS)]
#[serde(transparent)]
#[schemars(transparent)]
pub struct PlayerId(#[schemars(length(min = 1, max = 128))] String);

impl TryFrom<String> for PlayerId {
    type Error = ContractValueError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.trim().is_empty() || value.chars().count() > 128 {
            Err(ContractValueError::new(
                "PlayerId",
                "must be the 1-128 character Firebase subject",
            ))
        } else {
            Ok(Self(value))
        }
    }
}

impl PlayerId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for PlayerId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::try_from(String::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

impl CriticalMomentId {
    pub fn for_imported_game(game_ref: &GameRef, ply: u16) -> Self {
        let digest = game_ref
            .as_str()
            .strip_prefix("sha256:")
            .expect("Game references are validated SHA-256 digests");
        Self::try_from(format!("review-moment:{digest}:{ply}"))
            .expect("a digest-derived Review Moment ID is a valid semantic ID")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewSessionCoreContract {
    pub request_id: RequestId,
    pub imported_game: ImportedGame,
    pub position_snapshot: PositionSnapshot,
    pub review_moment: ReviewMomentOccurrence,
    pub coach_turn_context: CoachTurnContext,
    pub evidence_packet: ReviewSessionEvidencePacket,
}

/// One chronologically admitted Review Moment.
///
/// Display facts remain available independently of the richer authoring core.
/// Delivery surfaces can therefore render the complete automatic set while
/// authoring preparation is still pending.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewSessionMoment {
    pub review_moment: ReviewMomentOccurrence,
    pub position_snapshot: PositionSnapshot,
    pub learning_material: ReviewMomentLearningMaterial,
    pub authoring: ReviewMomentAuthoringReadiness,
    /// Extractor verdict kind. Player-Selected Moments need this on the wire so
    /// surfaces can drop Neutral and render a nominated Positive Highlight or
    /// Improvement Opportunity after resume.
    #[serde(default)]
    pub classification_kind: Option<ReviewMomentClassificationKind>,
}

/// Extractor verdict kind carried on an admitted Review Moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum ReviewMomentClassificationKind {
    PositiveHighlight,
    ImprovementOpportunity,
    Neutral,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ReviewMomentAuthoringReadiness {
    Pending,
    Prepared {
        core: Box<ReviewSessionCoreContract>,
    },
}

impl ReviewSessionMoment {
    pub fn pending(
        core: &ReviewSessionCoreContract,
        learning_material: ReviewMomentLearningMaterial,
        classification_kind: Option<ReviewMomentClassificationKind>,
    ) -> Self {
        Self {
            review_moment: core.review_moment.clone(),
            position_snapshot: core.position_snapshot.clone(),
            learning_material,
            authoring: ReviewMomentAuthoringReadiness::Pending,
            classification_kind,
        }
    }

    pub fn prepared(
        core: ReviewSessionCoreContract,
        learning_material: ReviewMomentLearningMaterial,
        classification_kind: Option<ReviewMomentClassificationKind>,
    ) -> Self {
        Self {
            review_moment: core.review_moment.clone(),
            position_snapshot: core.position_snapshot.clone(),
            learning_material,
            authoring: ReviewMomentAuthoringReadiness::Prepared {
                core: Box::new(core),
            },
            classification_kind,
        }
    }

    pub fn prepared_core(&self) -> Option<&ReviewSessionCoreContract> {
        match &self.authoring {
            ReviewMomentAuthoringReadiness::Pending => None,
            ReviewMomentAuthoringReadiness::Prepared { core } => Some(core),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportedGame {
    pub game: CanonicalCompletedGame,
    pub review_side: ReviewSide,
    pub elo_profile: ResolvedEloProfile,
    pub provenance: ImportProvenance,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CanonicalCompletedGame {
    pub game_ref: GameRef,
    pub white: PlayerMetadata,
    pub black: PlayerMetadata,
    pub event: MetadataText,
    pub site: MetadataText,
    pub opening: OpeningMetadata,
    pub outcome: CompletedGameOutcome,
    #[schemars(length(min = 1))]
    pub moves: Vec<CanonicalGameMove>,
    pub final_position_ref: PositionRef,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlayerMetadata {
    pub name: MetadataText,
    pub rating: RatingMetadata,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum MetadataText {
    Present { value: String },
    Absent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum RatingMetadata {
    Present { rating: EloRating },
    Absent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum CompletedGameOutcome {
    Decisive {
        winner: Color,
        termination: DecisiveGameTermination,
    },
    Draw {
        termination: DrawGameTermination,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum DecisiveGameTermination {
    Checkmate,
    Resignation,
    Timeout,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum DrawGameTermination {
    DrawAgreement,
    Stalemate,
    InsufficientMaterial,
    Repetition,
    FiftyMoveRule,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CanonicalGameMove {
    #[schemars(range(min = 1))]
    pub ply: u16,
    #[schemars(range(min = 1))]
    pub move_number: u16,
    pub side: Color,
    pub san: String,
    pub uci: String,
    pub before_position_ref: PositionRef,
    pub after_position_ref: PositionRef,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum ReviewSide {
    White,
    Black,
    Both,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ReviewMomentSelection {
    PipelineCriticalMoment {
        critical_moment_id: CriticalMomentId,
    },
    PlayerSelectedMoment {
        #[schemars(range(min = 1))]
        ply: u16,
    },
}

/// How a Player named the Review Moment they want opened.
///
/// Distinct from [`ReviewMomentSelection`], which every resolved occurrence
/// carries as its provenance: this is only the ask, and `Next` names no moment
/// at all until the ply-ordered review is consulted. Keeping the ask separate
/// from what it resolves to is what lets one tool serve "this Critical Moment",
/// "the move on ply 27 that nothing flagged", "whatever comes after this one",
/// and "the next Improvement Opportunity" without the caller resolving or
/// filtering anything itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ReviewMomentReference {
    /// One Critical Moment the frozen review already named.
    Critical { review_moment_id: CriticalMomentId },
    /// Any legal ply of the Game, flagged or not.
    Ply {
        #[schemars(range(min = 1))]
        ply: u16,
    },
    /// The matching Critical Moment after this one in ply order; the first when absent.
    Next {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after_review_moment_id: Option<CriticalMomentId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        classification: Option<ReviewMomentReferenceClassification>,
    },
}

/// An optional classification constraint on a forward Review Moment reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum ReviewMomentReferenceClassification {
    ImprovementOpportunity,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewMomentOccurrence {
    pub moment_id: CriticalMomentId,
    #[schemars(range(min = 1))]
    pub ply: u16,
    pub preceding_move: Option<CanonicalGameMove>,
    pub selection: ReviewMomentSelection,
    pub game_ref: GameRef,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolvedEloProfile {
    pub rating: EloRating,
    pub source: EloSource,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum EloSource {
    ImportedMetadata { review_side: Color },
    PlayerProvided,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema, TS)]
#[serde(transparent)]
#[schemars(transparent)]
pub struct EloRating(#[schemars(range(min = 100, max = 3500))] u16);

impl TryFrom<u16> for EloRating {
    type Error = ContractValueError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        if (100..=3500).contains(&value) {
            Ok(Self(value))
        } else {
            Err(ContractValueError::new(
                "EloRating",
                "must be between 100 and 3500",
            ))
        }
    }
}

impl<'de> Deserialize<'de> for EloRating {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::try_from(u16::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

impl EloRating {
    pub fn value(self) -> u16 {
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
pub enum ImportProvenance {
    Lichess {
        canonical_game_id: CanonicalGameId,
        side_qualified_url: String,
        canonical_url: String,
        export_contract_version: String,
        captured_at: String,
        response_digest: ArtifactDigest,
        pgn_digest: ArtifactDigest,
    },
    ChessCom {
        canonical_game_id: CanonicalGameId,
        canonical_url: String,
        fetch_contract_version: String,
        captured_at: String,
        response_digest: ArtifactDigest,
        pgn_digest: ArtifactDigest,
    },
    PastedPgn {
        pgn_digest: ArtifactDigest,
    },
    LocalPgn {
        pgn_digest: ArtifactDigest,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PositionSnapshot {
    pub position_ref: PositionRef,
    pub variant: PositionVariant,
    pub fen: String,
    pub occupied: Vec<OccupiedSquare>,
    pub side_to_move: Color,
    pub castling_rights: CastlingRights,
    pub en_passant: EnPassantState,
    pub halfmove_clock: u16,
    #[schemars(range(min = 1))]
    pub fullmove_number: u32,
    pub repetition: RepetitionState,
    pub status: PositionStatus,
    pub history_digest: ArtifactDigest,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PositionSnapshotWire {
    position_ref: PositionRef,
    variant: PositionVariant,
    fen: String,
    occupied: Vec<OccupiedSquare>,
    side_to_move: Color,
    castling_rights: CastlingRights,
    en_passant: EnPassantState,
    halfmove_clock: u16,
    fullmove_number: u32,
    repetition: RepetitionState,
    status: PositionStatus,
    history_digest: ArtifactDigest,
}

impl<'de> Deserialize<'de> for PositionSnapshot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = PositionSnapshotWire::deserialize(deserializer)?;
        let snapshot = Self {
            position_ref: wire.position_ref,
            variant: wire.variant,
            fen: wire.fen,
            occupied: wire.occupied,
            side_to_move: wire.side_to_move,
            castling_rights: wire.castling_rights,
            en_passant: wire.en_passant,
            halfmove_clock: wire.halfmove_clock,
            fullmove_number: wire.fullmove_number,
            repetition: wire.repetition,
            status: wire.status,
            history_digest: wire.history_digest,
        };
        if snapshot.position_ref == snapshot.computed_position_ref()
            && snapshot.has_canonical_position_fields()
        {
            Ok(snapshot)
        } else {
            Err(de::Error::custom(
                "PositionSnapshot is not canonical or positionRef does not match its content",
            ))
        }
    }
}

impl PositionSnapshot {
    pub fn computed_position_ref(&self) -> PositionRef {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct PositionContent<'a> {
            variant: PositionVariant,
            fen: &'a str,
            occupied: &'a [OccupiedSquare],
            side_to_move: Color,
            castling_rights: &'a CastlingRights,
            en_passant: &'a EnPassantState,
            halfmove_clock: u16,
            fullmove_number: u32,
            repetition: &'a RepetitionState,
            status: &'a PositionStatus,
            history_digest: &'a ArtifactDigest,
        }

        let content = PositionContent {
            variant: self.variant,
            fen: &self.fen,
            occupied: &self.occupied,
            side_to_move: self.side_to_move,
            castling_rights: &self.castling_rights,
            en_passant: &self.en_passant,
            halfmove_clock: self.halfmove_clock,
            fullmove_number: self.fullmove_number,
            repetition: &self.repetition,
            status: &self.status,
            history_digest: &self.history_digest,
        };
        PositionRef::try_from(canonical_sha256(&content))
            .expect("SHA-256 output is a valid Position reference")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum PositionVariant {
    Standard,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OccupiedSquare {
    pub square: Square,
    pub piece: Piece,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, JsonSchema, TS)]
#[serde(transparent)]
#[schemars(transparent)]
pub struct Square(#[schemars(regex(pattern = r"^[a-h][1-8]$"))] String);

impl TryFrom<String> for Square {
    type Error = ContractValueError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let bytes = value.as_bytes();
        if bytes.len() == 2
            && (b'a'..=b'h').contains(&bytes[0])
            && (b'1'..=b'8').contains(&bytes[1])
        {
            Ok(Self(value))
        } else {
            Err(ContractValueError::new(
                "Square",
                "must be algebraic a1 through h8",
            ))
        }
    }
}

impl Square {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Square {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::try_from(String::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum Color {
    White,
    Black,
}

impl Color {
    /// The side to move after this one. Whose reply a Review Moment is talking
    /// about, when it talks about a reply at all.
    pub fn opponent(self) -> Self {
        match self {
            Self::White => Self::Black,
            Self::Black => Self::White,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum PieceRole {
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
    King,
}

impl PieceRole {
    /// Conventional material value under `material-values/v1`.
    ///
    /// A king is not material that can be exchanged, so it has no value rather
    /// than an arbitrary sentinel that could leak into a transaction.
    pub fn conventional_material_value(self) -> Option<u8> {
        match self {
            Self::Pawn => Some(1),
            Self::Knight | Self::Bishop => Some(3),
            Self::Rook => Some(5),
            Self::Queen => Some(9),
            Self::King => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Piece {
    pub color: Color,
    pub role: PieceRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CastlingRights {
    pub white_king_side: bool,
    pub white_queen_side: bool,
    pub black_king_side: bool,
    pub black_queen_side: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum EnPassantState {
    Available { square: Square },
    None,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum RepetitionState {
    FirstOccurrence,
    Repeated {
        #[schemars(range(min = 2, max = 2))]
        occurrences: u8,
    },
    DrawClaimAvailable {
        #[schemars(range(min = 3))]
        occurrences: u8,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum PositionStatus {
    Ongoing { draw_claims: DrawClaimState },
    Checkmate { winner: Color },
    Draw { reason: DrawReason },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum DrawClaimState {
    None,
    Available {
        first: DrawClaimReason,
        rest: Vec<DrawClaimReason>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum DrawClaimReason {
    ThreefoldRepetition,
    FiftyMoveRule,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum DrawReason {
    Stalemate,
    InsufficientMaterial,
    Agreement,
    ThreefoldRepetition,
    FivefoldRepetition,
    FiftyMoveRule,
    SeventyFiveMoveRule,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CoachTurnContext {
    pub coach_turn_id: CoachTurnId,
    pub reviewed_move: ReviewedMoveAnchor,
    pub selected_position_ref: PositionRef,
    pub target: CoachTurnTarget,
    pub required_evidence_refs: Vec<EvidenceId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewedMoveAnchor {
    pub critical_moment_id: CriticalMomentId,
    #[schemars(range(min = 1))]
    pub ply: u16,
    pub side: Color,
    pub position_ref: PositionRef,
    pub played_move_uci: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum CoachTurnTarget {
    ImportedGameMove {
        critical_moment_id: CriticalMomentId,
        #[schemars(range(min = 1))]
        ply: u16,
        uci: String,
    },
    AlternativeMove {
        branch_ref: BranchRef,
        uci: String,
    },
}

impl ImportedGame {
    pub fn digest(&self) -> ImportedGameDigest {
        ImportedGameDigest::try_from(canonical_sha256(self))
            .expect("SHA-256 output is a valid Imported Game digest")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractValueError {
    type_name: &'static str,
    requirement: &'static str,
}

impl ContractValueError {
    pub(super) fn new(type_name: &'static str, requirement: &'static str) -> Self {
        Self {
            type_name,
            requirement,
        }
    }
}

impl fmt::Display for ContractValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} {}", self.type_name, self.requirement)
    }
}

impl std::error::Error for ContractValueError {}

fn validate_semantic_id(type_name: &'static str, value: &str) -> Result<(), ContractValueError> {
    let mut characters = value.chars();
    let valid_first = characters
        .next()
        .is_some_and(|character| character.is_ascii_alphanumeric());
    let valid_rest = characters.all(|character| {
        character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | ':' | '-')
    });
    if valid_first && valid_rest && value.len() <= 128 {
        Ok(())
    } else {
        Err(ContractValueError::new(
            type_name,
            "must be a 1-128 character opaque ASCII token",
        ))
    }
}

fn validate_content_digest(type_name: &'static str, value: &str) -> Result<(), ContractValueError> {
    let digest = value.strip_prefix("sha256:");
    if digest.is_some_and(|digest| {
        digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    }) {
        Ok(())
    } else {
        Err(ContractValueError::new(
            type_name,
            "must be sha256 followed by 64 lowercase hexadecimal characters",
        ))
    }
}

pub(super) fn canonical_sha256(value: &impl Serialize) -> String {
    let bytes = serde_json_canonicalizer::to_vec(value)
        .expect("contract values should have an RFC 8785 canonical form");
    format!("sha256:{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::{EloRating, EvidenceId, PlayerId, RequestId};

    #[test]
    fn semantic_id_types_reject_invalid_boundary_values() {
        assert!(serde_json::from_str::<RequestId>(r#""request:1""#).is_ok());
        assert!(serde_json::from_str::<RequestId>(r#"""#).is_err());
        assert!(serde_json::from_str::<PlayerId>(r#"" ""#).is_err());
        assert!(serde_json::from_str::<EvidenceId>(r#""not-a-digest""#).is_err());
        assert!(serde_json::from_str::<EloRating>("99").is_err());
    }
}
