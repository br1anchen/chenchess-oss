use shakmaty::{attacks, Board, Color, Position, Role, Square};

use crate::review_session_contract::{
    AtomicChessFact, AtomicChessFactData, CurriculumLearningConcept as Concept,
    DecisionTerminalState, MaterialInventory, SemanticOutcomeData,
};

use super::{proof, DetectedConcept};
use crate::decision_explanation::candidate::ReplayedCandidate;

pub(super) fn detect(
    candidate: &ReplayedCandidate,
    facts: &[&AtomicChessFact],
) -> Vec<DetectedConcept> {
    let mut detected = Vec::new();
    for ((index, step), position) in candidate
        .contract
        .line_steps
        .iter()
        .enumerate()
        .zip(candidate.positions.iter().skip(1))
    {
        if !position.is_checkmate() {
            continue;
        }
        let Some(geometry) = MateGeometry::from_position(position, step.mover) else {
            continue;
        };
        let supporting_fact_refs = facts
            .iter()
            .filter(|fact| {
                matches!(
                    &fact.data,
                    AtomicChessFactData::TerminalPosition {
                        snapshot_ref,
                        state: DecisionTerminalState::Checkmate,
                    } | AtomicChessFactData::Checkers { snapshot_ref, .. }
                        | AtomicChessFactData::KingZonePressure { snapshot_ref, .. }
                        | AtomicChessFactData::LegalDestinations { snapshot_ref, .. }
                        | AtomicChessFactData::PieceOccupancy { snapshot_ref, .. }
                        | AtomicChessFactData::MaterialInventory { snapshot_ref, .. }
                        if snapshot_ref == &step.after_snapshot_ref
                )
            })
            .map(|fact| fact.fact_ref.clone())
            .collect::<Vec<_>>();
        let outcome_refs = candidate
            .contract
            .outcomes
            .iter()
            .filter(|outcome| {
                matches!(
                    outcome.data,
                    SemanticOutcomeData::TerminalStateReached {
                        result: DecisionTerminalState::Checkmate,
                        ..
                    }
                )
            })
            .map(|outcome| outcome.outcome_ref.clone())
            .collect::<Vec<_>>();
        for concept in geometry.recognized_concepts() {
            if let Some(proof) = proof(
                candidate,
                concept,
                index,
                index,
                supporting_fact_refs.clone(),
                outcome_refs.clone(),
            ) {
                detected.push(proof);
            }
        }
    }
    detected
}

struct MateGeometry {
    board: Board,
    king: Square,
    mating_color: Color,
    mated_color: Color,
    checker_squares: Vec<Square>,
    attacking_material: MaterialInventory,
    own_neighbors: Vec<Square>,
}

type MateRule = (Concept, fn(&MateGeometry) -> bool);

impl MateGeometry {
    // Every matching geometry is retained. Table position is not policy.
    const SPECIFIC_RULES: &'static [MateRule] = &[
        (
            Concept::KnightAndBishopMate,
            Self::is_knight_and_bishop_mate,
        ),
        (Concept::SmotheredMate, Self::is_smothered),
        (Concept::BlindSwineMate, Self::is_blind_swine),
        (Concept::BodenMate, Self::is_boden),
        (Concept::DoubleBishopMate, Self::is_double_bishop),
        (Concept::PillsburysMate, Self::is_pillsbury),
        (Concept::VukovicMate, Self::is_vukovic),
        (Concept::ArabianMate, Self::is_arabian),
        (Concept::AnastasiaMate, Self::is_anastasia),
        (Concept::KillBoxMate, Self::is_kill_box),
        (Concept::BalestraMate, Self::is_balestra),
        (Concept::MorphysMate, Self::is_morphy),
        (Concept::OperaMate, Self::is_opera),
        (Concept::HookMate, Self::is_hook),
        (Concept::EpauletteMate, Self::is_epaulette),
        (Concept::SwallowstailMate, Self::is_swallowtail),
        (Concept::DovetailMate, Self::is_dovetail),
        (Concept::TriangleMate, Self::is_triangle),
        (Concept::CornerMate, Self::is_corner_mate),
        (Concept::BackRankMate, Self::is_back_rank),
    ];

    fn from_position(
        position: &shakmaty::Chess,
        mating_color: crate::review_session_contract::Color,
    ) -> Option<Self> {
        let mating_color = to_chess_color(mating_color);
        let mated_color = !mating_color;
        let board = position.board();
        let king = board.king_of(mated_color)?;
        let neighbors = attacks::king_attacks(king);
        let own_neighbors = (neighbors & board.by_color(mated_color))
            .into_iter()
            .collect::<Vec<_>>();
        let checker_squares = board
            .attacks_to(king, mating_color, board.occupied())
            .into_iter()
            .collect::<Vec<_>>();
        let material = board.material_side(mating_color);
        Some(Self {
            board: board.clone(),
            king,
            mating_color,
            mated_color,
            checker_squares,
            attacking_material: MaterialInventory {
                pawns: material.pawn,
                knights: material.knight,
                bishops: material.bishop,
                rooks: material.rook,
                queens: material.queen,
            },
            own_neighbors,
        })
    }

    fn recognized_concepts(&self) -> Vec<Concept> {
        let mut concepts = Self::SPECIFIC_RULES
            .iter()
            .filter_map(|(concept, recognizes)| recognizes(self).then_some(*concept))
            .collect::<Vec<_>>();
        let has_specific = !concepts.is_empty();
        let has_piece_method = concepts.contains(&Concept::KnightAndBishopMate);
        let has_named_geometry = concepts
            .iter()
            .any(|concept| *concept != Concept::KnightAndBishopMate);
        if has_piece_method || (!has_specific && self.is_piece_checkmate()) {
            concepts.push(Concept::PieceCheckmates);
        }
        if has_named_geometry
            || (!has_specific
                && !self.is_piece_checkmate()
                && self.pressuring_squares().count() >= 2)
        {
            concepts.push(Concept::CheckmatePatterns);
        }
        concepts.push(Concept::Checkmate);
        concepts.sort();
        concepts.dedup();
        concepts
    }

    fn is_knight_and_bishop_mate(&self) -> bool {
        self.attacking_material.queens == 0
            && self.attacking_material.rooks == 0
            && self.attacking_material.knights == 1
            && self.attacking_material.bishops == 1
    }

    fn is_smothered(&self) -> bool {
        self.checker_has(Role::Knight)
            && attacks::king_attacks(self.king)
                .into_iter()
                .all(|square| self.is_piece(square, self.mated_color, None))
    }

    fn is_blind_swine(&self) -> bool {
        let swine_rank = self
            .mating_color
            .fold_wb(shakmaty::Rank::Seventh, shakmaty::Rank::Second);
        self.is_back_rank_king()
            && self
                .squares(self.mating_color, Role::Rook)
                .filter(|square| square.rank() == swine_rank)
                .count()
                >= 2
    }

    fn is_double_bishop(&self) -> bool {
        self.checker_has(Role::Bishop)
            && self.supports_neighbor(Role::Bishop)
            && self.own_neighbors.len() <= 1
    }

    fn is_boden(&self) -> bool {
        self.checker_has(Role::Bishop)
            && self.supports_neighbor(Role::Bishop)
            && self.own_neighbors.len() >= 2
    }

    fn is_pillsbury(&self) -> bool {
        self.checker(Role::Rook)
            .is_some_and(|rook| rook.distance(self.king) > 1)
            && self.pressures_neighbor(Role::Bishop)
    }

    fn is_vukovic(&self) -> bool {
        self.checker(Role::Rook).is_some_and(|rook| {
            rook.distance(self.king) == 1
                && self.is_edge_king()
                && self.pressures_neighbor(Role::Knight)
                && self.square_is_attacked_by(rook, &[Role::King, Role::Pawn])
                && !self.square_is_attacked_by(rook, &[Role::Knight])
        })
    }

    fn is_arabian(&self) -> bool {
        self.checker(Role::Rook).is_some_and(|rook| {
            self.is_corner()
                && rook.distance(self.king) == 1
                && self.square_is_attacked_by(rook, &[Role::Knight])
        })
    }

    fn is_anastasia(&self) -> bool {
        self.is_edge_king()
            && !self.is_corner()
            && self
                .checker(Role::Rook)
                .is_some_and(|rook| rook.distance(self.king) > 1)
            && self.pressures_neighbor(Role::Knight)
            && self.has_enemy_pawn_neighbor()
    }

    fn is_kill_box(&self) -> bool {
        self.checker(Role::Rook).is_some_and(|rook| {
            rook.distance(self.king) == 1
                && self
                    .squares(self.mating_color, Role::Queen)
                    .any(|queen| queen.distance(rook) == 2 && self.attacks(queen, rook))
        })
    }

    fn is_balestra(&self) -> bool {
        self.checker_has(Role::Bishop) && self.pressures_neighbor(Role::Queen)
    }

    fn is_morphy(&self) -> bool {
        self.checker_has(Role::Bishop)
            && self.pressures_neighbor(Role::Rook)
            && self.has_enemy_pawn_neighbor()
    }

    fn is_opera(&self) -> bool {
        self.checker(Role::Rook).is_some_and(|rook| {
            rook.distance(self.king) == 1 && self.square_is_attacked_by(rook, &[Role::Bishop])
        })
    }

    fn is_hook(&self) -> bool {
        self.checker(Role::Rook).is_some_and(|rook| {
            rook.distance(self.king) == 1
                && self.squares(self.mating_color, Role::Knight).any(|knight| {
                    self.attacks(knight, rook) && self.square_is_attacked_by(knight, &[Role::Pawn])
                })
                && self.has_enemy_pawn_neighbor()
        })
    }

    fn is_back_rank(&self) -> bool {
        self.is_back_rank_king()
            && !self.own_neighbors.is_empty()
            && (self.checker_has(Role::Rook) || self.checker_has(Role::Queen))
    }

    fn is_epaulette(&self) -> bool {
        let Some(queen) = self.checker(Role::Queen) else {
            return false;
        };
        let check = direction(self.king, queen);
        let blockers = self
            .own_neighbors
            .iter()
            .map(|square| direction(self.king, *square))
            .collect::<Vec<_>>();
        blockers.contains(&rotate_left(check)) && blockers.contains(&rotate_right(check))
    }

    fn is_swallowtail(&self) -> bool {
        let Some(queen) = self.checker(Role::Queen) else {
            return false;
        };
        let (file, rank) = direction(self.king, queen);
        if file != 0 && rank != 0 {
            return false;
        }
        let away = (-file, -rank);
        let blockers = self
            .own_neighbors
            .iter()
            .map(|square| direction(self.king, *square))
            .collect::<Vec<_>>();
        let left = rotate_left(away);
        let right = rotate_right(away);
        blockers.contains(&((away.0 + left.0).signum(), (away.1 + left.1).signum()))
            && blockers.contains(&((away.0 + right.0).signum(), (away.1 + right.1).signum()))
    }

    fn is_dovetail(&self) -> bool {
        let Some(queen) = self.checker(Role::Queen) else {
            return false;
        };
        let check = direction(self.king, queen);
        if check.0 == 0 || check.1 == 0 {
            return false;
        }
        let away = negate(check);
        let blockers = self
            .own_neighbors
            .iter()
            .map(|square| direction(self.king, *square))
            .collect::<Vec<_>>();
        blockers.contains(&(away.0, 0)) && blockers.contains(&(0, away.1))
    }

    fn is_triangle(&self) -> bool {
        self.checker(Role::Queen).is_some_and(|queen| {
            self.squares(self.mating_color, Role::Rook).any(|rook| {
                queen.file() == rook.file()
                    && queen.distance(rook) == 2
                    && self.attacks(rook, queen)
            })
        })
    }

    fn is_corner_mate(&self) -> bool {
        self.is_corner()
            && (self.checker_has(Role::Knight) || self.checker_has(Role::Bishop))
            && self.pressures_neighbor(Role::Rook)
            && self.has_enemy_pawn_neighbor()
    }

    fn is_piece_checkmate(&self) -> bool {
        self.attacking_material.pawns
            + self.attacking_material.knights
            + self.attacking_material.bishops
            + self.attacking_material.rooks
            + self.attacking_material.queens
            == 1
    }

    fn is_corner(&self) -> bool {
        matches!(self.king, Square::A1 | Square::A8 | Square::H1 | Square::H8)
    }

    fn is_edge_king(&self) -> bool {
        matches!(self.king.file(), shakmaty::File::A | shakmaty::File::H)
            || matches!(
                self.king.rank(),
                shakmaty::Rank::First | shakmaty::Rank::Eighth
            )
    }

    fn is_back_rank_king(&self) -> bool {
        self.mated_color.fold_wb(
            self.king.rank() == shakmaty::Rank::First,
            self.king.rank() == shakmaty::Rank::Eighth,
        )
    }

    fn checker(&self, role: Role) -> Option<Square> {
        self.checker_squares
            .iter()
            .copied()
            .find(|square| self.board.role_at(*square) == Some(role))
    }

    fn checker_has(&self, role: Role) -> bool {
        self.checker(role).is_some()
    }

    fn squares(&self, color: Color, role: Role) -> impl Iterator<Item = Square> + '_ {
        (self.board.by_color(color) & self.board.by_role(role)).into_iter()
    }

    fn attacks(&self, attacker: Square, target: Square) -> bool {
        self.board.attacks_from(attacker).contains(target)
    }

    fn square_is_attacked_by(&self, target: Square, roles: &[Role]) -> bool {
        self.squares_with_roles(self.mating_color, roles)
            .any(|attacker| self.attacks(attacker, target))
    }

    fn squares_with_roles<'a>(
        &'a self,
        color: Color,
        roles: &'a [Role],
    ) -> impl Iterator<Item = Square> + 'a {
        self.board
            .by_color(color)
            .into_iter()
            .filter(move |square| {
                self.board
                    .role_at(*square)
                    .is_some_and(|role| roles.contains(&role))
            })
    }

    fn pressure_squares_for(&self, role: Role) -> impl Iterator<Item = Square> + '_ {
        let king_zone = attacks::king_attacks(self.king);
        self.squares(self.mating_color, role)
            .filter(move |square| !(self.board.attacks_from(*square) & king_zone).is_empty())
    }

    fn pressures_neighbor(&self, role: Role) -> bool {
        self.pressure_squares_for(role).next().is_some()
    }

    fn supports_neighbor(&self, role: Role) -> bool {
        self.pressure_squares_for(role)
            .any(|square| !self.checker_squares.contains(&square))
    }

    fn pressuring_squares(&self) -> impl Iterator<Item = Square> + '_ {
        let king_zone = attacks::king_attacks(self.king);
        self.board
            .by_color(self.mating_color)
            .into_iter()
            .filter(move |square| !(self.board.attacks_from(*square) & king_zone).is_empty())
    }

    fn has_enemy_pawn_neighbor(&self) -> bool {
        attacks::king_attacks(self.king)
            .into_iter()
            .any(|square| self.is_piece(square, self.mated_color, Some(Role::Pawn)))
    }

    fn is_piece(&self, square: Square, color: Color, role: Option<Role>) -> bool {
        self.board.piece_at(square).is_some_and(|piece| {
            piece.color == color && role.is_none_or(|expected| piece.role == expected)
        })
    }
}

fn direction(from: Square, to: Square) -> (i8, i8) {
    (
        (i16::from(u8::from(to.file())) - i16::from(u8::from(from.file()))).signum() as i8,
        (i16::from(u8::from(to.rank())) - i16::from(u8::from(from.rank()))).signum() as i8,
    )
}

fn negate(direction: (i8, i8)) -> (i8, i8) {
    (-direction.0, -direction.1)
}

fn rotate_left(direction: (i8, i8)) -> (i8, i8) {
    (-direction.1, direction.0)
}

fn rotate_right(direction: (i8, i8)) -> (i8, i8) {
    (direction.1, -direction.0)
}

fn to_chess_color(color: crate::review_session_contract::Color) -> Color {
    match color {
        crate::review_session_contract::Color::White => Color::White,
        crate::review_session_contract::Color::Black => Color::Black,
    }
}
