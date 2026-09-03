use crate::{engine_analysis::PositionEvaluation, types::MoveSide};

/// Presentation-ready evaluation formatting for Critical Moments.
///
/// Scores and labels are expressed from White's perspective so review prose
/// can quote them verbatim without converting centipawns or flipping signs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaluationDisplay {
    pub score: String,
    pub label: String,
}

pub fn evaluation_display(evaluation: PositionEvaluation, mover: MoveSide) -> EvaluationDisplay {
    let evaluation = white_perspective(evaluation, mover);
    EvaluationDisplay {
        score: score_string(evaluation),
        label: label(evaluation),
    }
}

/// Annotation glyph for the played move: `??`, `?`, `?!`, `!` when the engine
/// move was played, or `None` for an unremarkable move.
pub fn annotation(
    played_is_best: bool,
    centipawn_loss: Option<u32>,
    best_evaluation: PositionEvaluation,
    played_evaluation: Option<PositionEvaluation>,
) -> Option<String> {
    if played_is_best {
        return Some("!".to_string());
    }
    let mate_swing = matches!(best_evaluation, PositionEvaluation::MateIn(_))
        || matches!(played_evaluation, Some(PositionEvaluation::MateIn(_)));
    let glyph = match centipawn_loss {
        _ if mate_swing => "??",
        Some(loss) if loss >= 300 => "??",
        Some(loss) if loss >= 120 => "?",
        Some(loss) if loss >= 50 => "?!",
        _ => return None,
    };
    Some(glyph.to_string())
}

pub fn loss_pawns(centipawn_loss: u32) -> String {
    format!("{:.1}", f64::from(centipawn_loss) / 100.0)
}

fn white_perspective(evaluation: PositionEvaluation, mover: MoveSide) -> PositionEvaluation {
    match mover {
        MoveSide::White => evaluation,
        MoveSide::Black => match evaluation {
            PositionEvaluation::Centipawns(value) => {
                PositionEvaluation::Centipawns(value.checked_neg().unwrap_or(i32::MAX))
            }
            PositionEvaluation::MateIn(value) => {
                PositionEvaluation::MateIn(value.checked_neg().unwrap_or(i32::MAX))
            }
        },
    }
}

fn score_string(evaluation: PositionEvaluation) -> String {
    match evaluation {
        PositionEvaluation::Centipawns(value) => {
            let tenths = (f64::from(value) / 10.0).round() as i64;
            if tenths == 0 {
                return "0.0".to_string();
            }
            let sign = if tenths > 0 { "+" } else { "-" };
            format!("{sign}{}.{}", tenths.abs() / 10, tenths.abs() % 10)
        }
        PositionEvaluation::MateIn(value) => format!("#{value}"),
    }
}

fn label(evaluation: PositionEvaluation) -> String {
    let value = match evaluation {
        PositionEvaluation::MateIn(value) if value > 0 => {
            return format!("White mates in {value}");
        }
        PositionEvaluation::MateIn(value) => {
            return format!("Black mates in {}", value.unsigned_abs());
        }
        PositionEvaluation::Centipawns(value) => value,
    };
    let pawns = f64::from(value).abs() / 100.0;
    if pawns < 0.3 {
        return "Roughly balanced".to_string();
    }
    let side = if value > 0 { "White" } else { "Black" };
    let degree = if pawns < 1.5 {
        "Slightly"
    } else if pawns < 3.0 {
        "Much"
    } else {
        "Massively"
    };
    format!("{degree} better for {side}")
}

#[cfg(test)]
mod tests {
    use super::{annotation, evaluation_display, loss_pawns};
    use crate::{engine_analysis::PositionEvaluation, types::MoveSide};

    #[test]
    fn scores_and_labels_use_the_white_perspective() {
        let display = evaluation_display(PositionEvaluation::Centipawns(-185), MoveSide::Black);
        assert_eq!(display.score, "+1.9");
        assert_eq!(display.label, "Much better for White");

        let display = evaluation_display(PositionEvaluation::Centipawns(-185), MoveSide::White);
        assert_eq!(display.score, "-1.9");
        assert_eq!(display.label, "Much better for Black");

        let display = evaluation_display(PositionEvaluation::Centipawns(-11), MoveSide::White);
        assert_eq!(display.score, "-0.1");
        assert_eq!(display.label, "Roughly balanced");

        let display = evaluation_display(PositionEvaluation::Centipawns(725), MoveSide::White);
        assert_eq!(display.score, "+7.3");
        assert_eq!(display.label, "Massively better for White");
    }

    #[test]
    fn mate_scores_name_the_mating_side() {
        let display = evaluation_display(PositionEvaluation::MateIn(3), MoveSide::White);
        assert_eq!(display.score, "#3");
        assert_eq!(display.label, "White mates in 3");

        let display = evaluation_display(PositionEvaluation::MateIn(2), MoveSide::Black);
        assert_eq!(display.score, "#-2");
        assert_eq!(display.label, "Black mates in 2");
    }

    #[test]
    fn annotations_grade_by_loss_and_mate_swings() {
        let cp = PositionEvaluation::Centipawns;
        assert_eq!(
            annotation(false, Some(736), cp(725), Some(cp(-11))).as_deref(),
            Some("??")
        );
        assert_eq!(
            annotation(false, Some(148), cp(-37), Some(cp(-185))).as_deref(),
            Some("?")
        );
        assert_eq!(
            annotation(false, Some(119), cp(480), Some(cp(361))).as_deref(),
            Some("?!")
        );
        assert_eq!(annotation(false, Some(10), cp(30), Some(cp(20))), None);
        assert_eq!(annotation(true, None, cp(30), None).as_deref(), Some("!"));
        assert_eq!(
            annotation(false, None, PositionEvaluation::MateIn(2), Some(cp(0))).as_deref(),
            Some("??")
        );
    }

    #[test]
    fn loss_is_reported_in_pawn_units() {
        assert_eq!(loss_pawns(148), "1.5");
        assert_eq!(loss_pawns(736), "7.4");
    }
}
