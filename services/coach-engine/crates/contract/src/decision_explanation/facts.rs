use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{AtomicFactRef, DecisionPositionSnapshotRef, LineStepRef, SemanticOutcomeRef};
use crate::{Color, PieceRole, PositionRef, Square};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecisionPositionSnapshot {
    pub snapshot_ref: DecisionPositionSnapshotRef,
    pub canonical_position_ref: PositionRef,
    pub fen: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecisionLineStep {
    pub step_ref: LineStepRef,
    pub before_snapshot_ref: DecisionPositionSnapshotRef,
    pub after_snapshot_ref: DecisionPositionSnapshotRef,
    pub uci: String,
    pub mover: Color,
    pub role: PieceRole,
    pub from_square: Square,
    pub to_square: Square,
    pub captured: Option<PieceAtSquare>,
    pub promotion: Option<PieceRole>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PieceAtSquare {
    pub color: Color,
    pub role: PieceRole,
    pub square: Square,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MaterialInventory {
    pub pawns: u8,
    pub knights: u8,
    pub bishops: u8,
    pub rooks: u8,
    pub queens: u8,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "camelCase")]
pub enum CastlingWing {
    KingSide,
    QueenSide,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AtomicChessFact {
    pub fact_ref: AtomicFactRef,
    pub data: AtomicChessFactData,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum AtomicChessFactData {
    PieceOccupancy {
        snapshot_ref: DecisionPositionSnapshotRef,
        piece: PieceAtSquare,
    },
    AttackSet {
        snapshot_ref: DecisionPositionSnapshotRef,
        attacker: PieceAtSquare,
        attacked_squares: Vec<Square>,
    },
    SoleRayBlocker {
        snapshot_ref: DecisionPositionSnapshotRef,
        attacker: PieceAtSquare,
        blocker: PieceAtSquare,
        target: PieceAtSquare,
    },
    Checkers {
        snapshot_ref: DecisionPositionSnapshotRef,
        king: PieceAtSquare,
        checking_pieces: Vec<PieceAtSquare>,
    },
    KingZonePressure {
        snapshot_ref: DecisionPositionSnapshotRef,
        king: PieceAtSquare,
        zone_squares: Vec<Square>,
        attacking_pieces: Vec<PieceAtSquare>,
    },
    LegalRecaptures {
        snapshot_ref: DecisionPositionSnapshotRef,
        side: Color,
        target_square: Square,
        moves: Vec<String>,
    },
    PieceMoved {
        step_ref: LineStepRef,
        piece: PieceAtSquare,
        from_square: Square,
        to_square: Square,
        uci: String,
    },
    PieceCaptured {
        step_ref: LineStepRef,
        captured: PieceAtSquare,
    },
    PiecePromoted {
        step_ref: LineStepRef,
        pawn_origin_square: Square,
        promotion_square: Square,
        promotion_role: PieceRole,
    },
    Castled {
        step_ref: LineStepRef,
        side: Color,
        wing: CastlingWing,
        king_from_square: Square,
        king_to_square: Square,
        rook_from_square: Square,
        rook_to_square: Square,
    },
    EnPassantCaptured {
        step_ref: LineStepRef,
        capturing_pawn: PieceAtSquare,
        from_square: Square,
        to_square: Square,
        captured_pawn_square: Square,
    },
    PawnFrontSpanOccupancy {
        snapshot_ref: DecisionPositionSnapshotRef,
        pawn: PieceAtSquare,
        front_span: Vec<Square>,
        opposing_pawns: Vec<Square>,
    },
    LegalDestinations {
        snapshot_ref: DecisionPositionSnapshotRef,
        piece: PieceAtSquare,
        destinations: Vec<Square>,
    },
    TerminalPosition {
        snapshot_ref: DecisionPositionSnapshotRef,
        state: DecisionTerminalState,
    },
    MaterialInventory {
        snapshot_ref: DecisionPositionSnapshotRef,
        side: Color,
        inventory: MaterialInventory,
    },
    MaterialChanged {
        step_ref: LineStepRef,
        before_inventory_refs: Vec<AtomicFactRef>,
        after_inventory_refs: Vec<AtomicFactRef>,
        captured: Option<PieceAtSquare>,
        promoted: Option<PieceRole>,
        conventional_value_delta: i16,
        value_policy_version: MaterialValuePolicyVersion,
    },
    AttackSetChanged {
        step_ref: LineStepRef,
        before_attack_ref: AtomicFactRef,
        after_attack_ref: AtomicFactRef,
        added_squares: Vec<Square>,
        removed_squares: Vec<Square>,
    },
    CheckersChanged {
        step_ref: LineStepRef,
        before_checkers_ref: AtomicFactRef,
        after_checkers_ref: AtomicFactRef,
        added_checkers: Vec<PieceAtSquare>,
        removed_checkers: Vec<PieceAtSquare>,
    },
    KingZonePressureChanged {
        step_ref: LineStepRef,
        before_pressure_ref: AtomicFactRef,
        after_pressure_ref: AtomicFactRef,
        added_attackers: Vec<PieceAtSquare>,
        removed_attackers: Vec<PieceAtSquare>,
    },
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "camelCase")]
pub enum DecisionTerminalState {
    Ongoing,
    Checkmate,
    Stalemate,
    Draw,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema, TS,
)]
pub enum MaterialValuePolicyVersion {
    #[serde(rename = "material-values/v1")]
    V1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SemanticOutcome {
    pub outcome_ref: SemanticOutcomeRef,
    pub data: SemanticOutcomeData,
    pub supporting_fact_refs: Vec<AtomicFactRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum PositionGoal {
    /// Win material by gaining at least one of these exact pieces. Multiple
    /// targets preserve the concrete choice created by a tactic such as a
    /// fork; realizing either branch satisfies the goal.
    GainMaterial { targets: Vec<PieceAtSquare> },
}

impl PositionGoal {
    /// Returns whether one observed outcome establishes this desired state
    /// change. Goal matching is deliberately total: unrelated outcome kinds
    /// are ordinary non-matches.
    pub fn is_satisfied_by(&self, outcome: &SemanticOutcome) -> bool {
        match self {
            Self::GainMaterial { targets } => match &outcome.data {
                SemanticOutcomeData::MaterialBalanceChanged {
                    conventional_value_delta,
                    gained,
                    ..
                } => {
                    *conventional_value_delta > 0
                        && gained.iter().any(|piece| targets.contains(piece))
                }
                _ => false,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum SemanticOutcomeData {
    MaterialBalanceChanged {
        conventional_value_delta: i16,
        value_policy_version: MaterialValuePolicyVersion,
        gained: Vec<PieceAtSquare>,
        lost: Vec<PieceAtSquare>,
    },
    PawnProgressed {
        pawn: PieceAtSquare,
        from_square: Square,
        to_square: Square,
        promotion_role: Option<PieceRole>,
        before_front_span_ref: AtomicFactRef,
        after_front_span_ref: Option<AtomicFactRef>,
    },
    MaterialConfigurationChanged {
        before_inventory_refs: Vec<AtomicFactRef>,
        after_inventory_refs: Vec<AtomicFactRef>,
    },
    TerminalStateReached {
        before_state_ref: AtomicFactRef,
        after_state_ref: AtomicFactRef,
        result: DecisionTerminalState,
    },
    AttackAccessChanged {
        before_attack_ref: AtomicFactRef,
        after_attack_ref: AtomicFactRef,
        added_squares: Vec<Square>,
        removed_squares: Vec<Square>,
    },
    CheckStateChanged {
        before_checkers_ref: AtomicFactRef,
        after_checkers_ref: AtomicFactRef,
        added_checkers: Vec<PieceAtSquare>,
        removed_checkers: Vec<PieceAtSquare>,
    },
    KingZonePressureChanged {
        before_pressure_ref: AtomicFactRef,
        after_pressure_ref: AtomicFactRef,
        added_attackers: Vec<PieceAtSquare>,
        removed_attackers: Vec<PieceAtSquare>,
    },
}
