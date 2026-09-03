use super::*;

#[test]
fn coach_assessment_explanations_are_bounded_before_domain_dispatch() {
    let mut encoded =
        serde_json::to_value(coach_assessment_envelope()).expect("the command is serializable");
    let maximum = "x".repeat(usize::from(
        ReviewSessionLimits::V1.max_player_message_bytes,
    ));
    for field in ["objectiveQuality", "findability", "resilience"] {
        encoded["command"]["assessment"][field]["explanation"] =
            serde_json::Value::String(maximum.clone());
    }
    assert!(serde_json::from_value::<ReviewSessionCommandEnvelope>(encoded.clone()).is_ok());

    encoded["command"]["assessment"]["resilience"]["explanation"] =
        serde_json::Value::String(format!("{maximum}x"));
    assert!(serde_json::from_value::<ReviewSessionCommandEnvelope>(encoded).is_err());
}

#[test]
fn durable_text_limit_counts_json_escape_expansion() {
    assert!(has_nonempty_json_text_within_limit(
        &"\0".repeat(682),
        ReviewSessionLimits::V1.max_player_message_bytes,
    ));
    assert!(!has_nonempty_json_text_within_limit(
        &"\0".repeat(683),
        ReviewSessionLimits::V1.max_player_message_bytes,
    ));
}

#[test]
fn start_host_turn_is_web_only_and_bounds_prior_turns() {
    let mut encoded = serde_json::to_value(host_turn_envelope(vec![prior_turn(), prior_turn()]))
        .expect("the command is serializable");
    assert!(serde_json::from_value::<ReviewSessionCommandEnvelope>(encoded.clone()).is_ok());

    encoded["surface"] = serde_json::Value::String("coachApp".to_string());
    let coach_app_host_turn =
        serde_json::from_value::<ReviewSessionCommandEnvelope>(encoded).unwrap_err();
    assert!(coach_app_host_turn
        .to_string()
        .contains("surface or text policy"));

    let at_cap = host_turn_envelope(
        (0..usize::from(ReviewSessionLimits::V1.max_host_turn_prior_turns))
            .map(|_| prior_turn())
            .collect(),
    );
    assert!(serde_json::from_value::<ReviewSessionCommandEnvelope>(
        serde_json::to_value(at_cap).expect("the command is serializable")
    )
    .is_ok());

    let over_cap = host_turn_envelope(
        (0..=usize::from(ReviewSessionLimits::V1.max_host_turn_prior_turns))
            .map(|_| prior_turn())
            .collect(),
    );
    assert!(serde_json::from_value::<ReviewSessionCommandEnvelope>(
        serde_json::to_value(over_cap).expect("the command is serializable")
    )
    .is_err());
}

#[test]
fn start_coach_turn_is_not_web_admissible() {
    let coach_turn_id =
        CoachTurnId::try_from("coach-turn:boundary".to_string()).expect("valid fixture ID");
    let position = PositionRef::try_from(format!("sha256:{}", "0".repeat(64)))
        .expect("valid fixture position");
    let envelope = ReviewSessionCommandEnvelope {
        request_id: RequestId::try_from("request:coach-turn-boundary".to_string())
            .expect("valid fixture ID"),
        operation_id: OperationId::try_from("operation:coach-turn-boundary".to_string())
            .expect("valid fixture ID"),
        surface: DeliverySurface::CoachApp,
        command: ReviewSessionCommand::StartCoachTurn {
            game_import_id: GameImportId::try_from("game-import:coach-turn-boundary".to_string())
                .expect("valid fixture ID"),
            review_moment_id: CriticalMomentId::try_from(
                "review-moment:coach-turn-boundary".to_string(),
            )
            .expect("valid fixture ID"),
            coach_turn_id: coach_turn_id.clone(),
            context: Box::new(CoachTurnContext {
                coach_turn_id,
                reviewed_move: ReviewedMoveAnchor {
                    critical_moment_id: CriticalMomentId::try_from(
                        "review-moment:coach-turn-boundary".to_string(),
                    )
                    .expect("valid fixture ID"),
                    ply: 1,
                    side: Color::White,
                    position_ref: position.clone(),
                    played_move_uci: "e2e4".to_string(),
                },
                selected_position_ref: position,
                target: CoachTurnTarget::ImportedGameMove {
                    critical_moment_id: CriticalMomentId::try_from(
                        "review-moment:coach-turn-boundary".to_string(),
                    )
                    .expect("valid fixture ID"),
                    ply: 1,
                    uci: "e2e4".to_string(),
                },
                required_evidence_refs: Vec::new(),
            }),
            message: "Please assess this branch.".to_string(),
            idempotency_key: IdempotencyKey::try_from(
                "idempotency-key:coach-turn-boundary".to_string(),
            )
            .expect("valid fixture ID"),
            prior_turn: PriorCoachTurn::None,
        },
    };
    let mut encoded = serde_json::to_value(envelope).expect("the command is serializable");
    assert!(serde_json::from_value::<ReviewSessionCommandEnvelope>(encoded.clone()).is_ok());

    encoded["surface"] = serde_json::Value::String("web".to_string());
    assert!(serde_json::from_value::<ReviewSessionCommandEnvelope>(encoded).is_err());
}

#[test]
fn start_host_turn_message_must_be_nonempty() {
    let mut encoded =
        serde_json::to_value(host_turn_envelope(Vec::new())).expect("the command is serializable");
    encoded["command"]["message"] = serde_json::Value::String(String::new());
    assert!(serde_json::from_value::<ReviewSessionCommandEnvelope>(encoded.clone()).is_err());

    encoded["command"]["message"] = serde_json::Value::String("   ".to_string());
    assert!(serde_json::from_value::<ReviewSessionCommandEnvelope>(encoded).is_err());
}

#[test]
fn start_host_turn_message_is_bounded_before_domain_dispatch() {
    let mut encoded =
        serde_json::to_value(host_turn_envelope(Vec::new())).expect("the command is serializable");
    let maximum = "x".repeat(usize::from(
        ReviewSessionLimits::V1.max_player_message_bytes,
    ));
    encoded["command"]["message"] = serde_json::Value::String(maximum.clone());
    assert!(serde_json::from_value::<ReviewSessionCommandEnvelope>(encoded.clone()).is_ok());

    encoded["command"]["message"] = serde_json::Value::String(format!("{maximum}x"));
    assert!(serde_json::from_value::<ReviewSessionCommandEnvelope>(encoded).is_err());
}

fn host_turn_envelope(prior_turns: Vec<HostTurnPriorTurn>) -> ReviewSessionCommandEnvelope {
    ReviewSessionCommandEnvelope {
        request_id: RequestId::try_from("request:host-turn-boundary".to_string())
            .expect("valid fixture ID"),
        operation_id: OperationId::try_from("operation:host-turn-boundary".to_string())
            .expect("valid fixture ID"),
        surface: DeliverySurface::Web,
        command: ReviewSessionCommand::StartHostTurn {
            game_import_id: GameImportId::try_from("game-import:host-turn-boundary".to_string())
                .expect("valid fixture ID"),
            message: "Why was this move a mistake?".to_string(),
            prior_turns,
            idempotency_key: IdempotencyKey::try_from(
                "idempotency-key:host-turn-boundary".to_string(),
            )
            .expect("valid fixture ID"),
        },
    }
}

fn prior_turn() -> HostTurnPriorTurn {
    HostTurnPriorTurn {
        message: "What should I have played?".to_string(),
        answer: "Nf6 keeps the piece.".to_string(),
    }
}

#[test]
fn learning_path_vote_must_be_present_even_when_it_is_null() {
    let command = serde_json::json!({
        "kind": "updateLearningPathVote",
        "gameImportId": "game-import:boundary",
        "learningPathRef": "learning-path:boundary"
    });
    assert!(serde_json::from_value::<ReviewSessionCommand>(command.clone()).is_err());

    let mut clear = command;
    clear["vote"] = serde_json::Value::Null;
    assert!(serde_json::from_value::<ReviewSessionCommand>(clear).is_ok());
}

fn coach_assessment_envelope() -> ReviewSessionCommandEnvelope {
    let coach_turn_id =
        CoachTurnId::try_from("coach-turn:boundary".to_string()).expect("valid fixture ID");
    let dimension = AssessmentDimension {
        explanation: "grounded".to_string(),
        evidence_refs: Vec::new(),
    };
    ReviewSessionCommandEnvelope {
        request_id: RequestId::try_from("request:boundary".to_string()).expect("valid fixture ID"),
        operation_id: OperationId::try_from("operation:boundary".to_string())
            .expect("valid fixture ID"),
        surface: DeliverySurface::CoachApp,
        command: ReviewSessionCommand::PublishCoachTurn {
            game_import_id: GameImportId::try_from("game-import:boundary".to_string())
                .expect("valid fixture ID"),
            review_moment_id: CriticalMomentId::try_from("review-moment:boundary".to_string())
                .expect("valid fixture ID"),
            coach_turn_id: coach_turn_id.clone(),
            assessment: Box::new(AlternativeMoveAssessment {
                coach_turn_id,
                alternative_move_id: AlternativeMoveId::try_from(
                    "alternative-move:boundary".to_string(),
                )
                .expect("valid fixture ID"),
                objective_quality: dimension.clone(),
                findability: dimension.clone(),
                resilience: dimension,
            }),
            idempotency_key: IdempotencyKey::try_from("idempotency-key:boundary".to_string())
                .expect("valid fixture ID"),
        },
    }
}

#[test]
fn delete_game_import_is_web_only() {
    let envelope = ReviewSessionCommandEnvelope {
        request_id: RequestId::try_from("request:delete-boundary".to_string())
            .expect("valid fixture ID"),
        operation_id: OperationId::try_from("operation:delete-boundary".to_string())
            .expect("valid fixture ID"),
        surface: DeliverySurface::Web,
        command: ReviewSessionCommand::DeleteGameImport {
            game_import_id: GameImportId::try_from("game-import:delete-boundary".to_string())
                .expect("valid fixture ID"),
        },
    };
    let mut encoded = serde_json::to_value(envelope).expect("the command is serializable");
    assert!(serde_json::from_value::<ReviewSessionCommandEnvelope>(encoded.clone()).is_ok());

    for surface in ["coachApp", "coachSkill"] {
        encoded["surface"] = serde_json::Value::String(surface.to_string());
        assert!(serde_json::from_value::<ReviewSessionCommandEnvelope>(encoded.clone()).is_err());
    }
}
