const SKILL: &str = include_str!("../../../skills/chenchess-coach/SKILL.md");
const WRITING_RULES: &str = include_str!("../../../skills/chenchess-coach/review-writing.md");
const REVIEW_SESSION: &str = include_str!("../../../skills/chenchess-coach/review-session.md");
const MANUAL_SCENARIOS: &str = include_str!("../../../skills/chenchess-coach/manual-scenarios.md");

#[test]
fn skill_has_a_bounded_review_session_trigger_contract() {
    assert!(SKILL.starts_with("---\nname: chenchess-coach\n"));
    assert!(SKILL.contains(
        "supported completed Chess.com Game, Lichess Game, pasted PGN, or local PGN file"
    ));
    assert!(SKILL.contains("Do not automatically invoke this skill"));
    assert!(SKILL.contains("The active agent is the Language Layer"));
    assert!(SKILL.contains("never invoke another hosted model"));
}

#[test]
fn skill_supports_all_sources_without_leaking_local_pgn() {
    for source in ["Chess.com", "Lichess", "local PGN", "pasted PGN"] {
        assert!(SKILL.contains(source));
    }
    assert!(SKILL.contains("pass only its path"));
    assert!(SKILL.contains("do not read the file into agent context"));
    assert!(REVIEW_SESSION.contains("localPgnFile"));
    assert!(REVIEW_SESSION.contains("pastedPgn"));
    assert!(REVIEW_SESSION.contains("lichessUrl"));
    assert!(REVIEW_SESSION.contains("fromQualifiedUrl"));
    assert!(REVIEW_SESSION.contains("playerProvided"));
}

#[test]
fn every_explained_position_uses_review_engine_board_side_and_evaluation() {
    for text in [SKILL, WRITING_RULES, REVIEW_SESSION] {
        assert!(text.contains("textBoard") || text.contains("text board"));
        assert!(text.contains("side to move") || text.contains("sideToMove"));
        assert!(text.contains("evaluation"));
        assert!(!text.contains("Rust"));
    }
    assert!(SKILL.contains("Never reconstruct a board or calculate an evaluation"));
    assert!(WRITING_RULES.contains("Never derive these from FEN yourself"));
}

#[test]
fn game_review_prompt_loads_causal_writing_rules() {
    assert!(SKILL.contains("[review-writing.md](review-writing.md)"));
    for instruction in [
        "concrete consequence",
        "opponent's strongest response",
        "clear causal chain",
        "Do not state what the Player intended as fact",
        "Evaluations support that explanation; they are not the explanation",
    ] {
        assert!(WRITING_RULES.contains(instruction), "missing {instruction}");
    }
    assert!(WRITING_RULES.contains("objective.lines.refutation"));
    assert!(WRITING_RULES.contains("translate UCI yourself"));
}

#[test]
fn game_review_prompt_grounds_prose_in_typed_causal_facts() {
    for instruction in [
        "criticalMoment.effects",
        "residualOutcome",
        "mechanism",
        "forcingIndex",
        "never from your own reading of centipawns",
        "already truncated at the payoff",
        "allows a queen trade",
    ] {
        assert!(WRITING_RULES.contains(instruction), "missing {instruction}");
    }
}

#[test]
fn authored_outputs_are_validated_before_presentation() {
    assert!(SKILL.contains("chenchess validate-review"));
    assert!(!SKILL.contains("chenchess validate-practice"));
    assert!(SKILL.contains("at most one structural repair"));
    assert!(SKILL.contains(
        "Present authored output only after the CLI returns a matching `completed` event"
    ));
    for authored in ["Player Plan Evaluations", "Alternative Move Assessments"] {
        assert!(SKILL.contains(authored));
    }
    assert!(REVIEW_SESSION.contains("Present only `coachTurnCompleted`"));
}

#[test]
fn chronological_review_moments_and_ephemeral_authoring_context_are_cli_authoritative() {
    for instruction in [
        "one `startReviewSession` with the opaque Game Import ID",
        "chronological `reviewMoments`",
        "Session start prepares objective Review Moment facts only",
        "Never expose or reconstruct candidates",
    ] {
        assert!(SKILL.contains(instruction), "missing {instruction}");
    }
    for instruction in [
        "Never render a separate Intent Hypothesis section",
        "Do not add a second hypothesis",
        "optional ephemeral intent wording",
    ] {
        assert!(WRITING_RULES.contains(instruction), "missing {instruction}");
    }
    assert!(
        REVIEW_SESSION.contains("Session start performs no Maia or Stockfish intent enrichment")
    );
    assert!(SKILL.contains("exactly one selected four-ply Projected Plan SAN line"));
    assert!(SKILL.contains("checks objective causal literals"));
}

#[test]
fn player_plan_discussion_is_conversational_and_evaluation_is_one_shot() {
    for instruction in [
        "Keep Player plan discussion in the normal conversational reply flow",
        "Do not introduce an intent card, confirmation, correction, skip, clarification, assessment controls",
        "Call Player Plan Evaluation only when an engine-backed comparison would materially improve the answer",
        "never ask a tool-driven clarification or create intent state",
    ] {
        assert!(REVIEW_SESSION.contains(instruction), "missing {instruction}");
    }
    assert!(REVIEW_SESSION.contains("`evaluatePlayerPlan`"));
    assert!(REVIEW_SESSION.contains("`playerPlanEvaluated.text`"));
}

#[test]
fn interactive_flow_preserves_complete_context_and_terminal_semantics() {
    assert!(SKILL.contains("Carry the complete returned `context` by value"));
    assert!(SKILL.contains("never replace it with agent memory"));
    for operation in [
        "inspectPosition",
        "evaluatePlayerPlan",
        "exploreAlternativeMove",
        "startCoachTurn",
        "publishCoachTurn",
    ] {
        assert!(REVIEW_SESSION.contains(operation));
    }
    for terminal in ["`rejected`", "`unavailable`", "`cancelled`", "`conflict`"] {
        assert!(REVIEW_SESSION.contains(terminal));
    }
    assert!(REVIEW_SESSION.contains("Cancellation is idempotent"));
    assert!(REVIEW_SESSION.contains("For steering"));
}

#[test]
fn concluded_discussion_offers_a_fresh_critical_moment_picker() {
    for instruction in [
        "reaches a natural conclusion",
        "ask whether the Player wants to select another Critical Moment",
        "fresh chronological Critical Moment picker",
        "never rewrite an earlier moment card",
    ] {
        assert!(SKILL.contains(instruction), "missing {instruction}");
    }
}

#[test]
fn learning_plan_is_consumed_directly_and_empty_material_is_omitted() {
    assert!(SKILL.contains("gameImported.review.learningPlan"));
    assert!(SKILL.contains("active `reviewMoments[]` entry's `learningMaterial`"));
    assert!(SKILL.contains("including for Player-selected moments"));
    assert!(SKILL.contains("Review Engine-selected facts"));
    assert!(SKILL.contains("Render every resource's exact title and canonical URL"));
    assert!(SKILL.contains("Never expose rank, support count, or selection trace"));
    assert!(SKILL.contains("Empty track collections render no learning section"));
    assert!(SKILL.contains("never propose, reorder, replace, browse for, or author them"));
    assert!(SKILL.contains("must not trigger generic lesson or training-plan prose"));
    assert!(WRITING_RULES.contains("Do not add generic lesson or training-plan prose"));
    assert!(!SKILL.contains("chenchess validate-practice"));
    assert!(!SKILL.contains("gameImported.review.practiceSelection"));
}

#[test]
fn fixed_scenarios_cover_required_coach_skill_journeys() {
    for scenario in [
        "Canonical Lichess URL",
        "Pasted PGN",
        "Local-file privacy",
        "Pipeline-selected moment",
        "Player-selected moment",
        "Player plan discussion",
        "Player Plan Evaluation",
        "Alternative Move and targeted coaching",
        "Cancellation and steering",
        "Fork Learning Path",
        "Hanging Piece Learning Path",
        "Opening Learning Path",
        "Passed Pawn Promotion Learning Path",
        "Empty Learning Plan",
        "Player-selected Local Learning Tracks",
        "Typed failures",
        "Equivalent fixture journeys",
    ] {
        assert!(MANUAL_SCENARIOS.contains(scenario), "missing {scenario}");
    }
}

#[test]
fn skill_keeps_review_session_ephemeral_and_cleans_up() {
    assert!(SKILL.contains("one long-lived `chenchess review-session --jsonl` process"));
    assert!(SKILL.contains("`--command-fifo"));
    assert!(SKILL.contains("never send Review Session commands through terminal stdin"));
    assert!(REVIEW_SESSION.contains("canonical PTYs can discard a JSONL line"));
    assert!(MANUAL_SCENARIOS.contains("exceeds 1024 bytes"));
    assert!(SKILL.contains("only in memory and that directory"));
    assert!(SKILL.contains("stop the JSONL process"));
    assert!(SKILL.contains("remove the drafts, event logs, and temporary directory"));
    assert!(SKILL.contains("Do not copy them into a repository or persist"));
    assert!(MANUAL_SCENARIOS.contains("without any Central Host upload"));
    assert!(MANUAL_SCENARIOS.contains("temporary event/draft cleanup"));
}

#[test]
fn skill_surfaces_measured_pipeline_time_when_a_review_is_slow() {
    assert!(REVIEW_SESSION.contains("gameImported.timing"));
    assert!(REVIEW_SESSION.contains("timing.totalPipelineMilliseconds"));
    assert!(REVIEW_SESSION.contains("Engine Analysis and Human Move Model call summaries"));
    assert!(REVIEW_SESSION.contains("measure Stockfish and Maia respectively"));
}
