use serde_json::json;

use super::*;
use crate::language_layer_prompt::CoachingProfileProjection;
use crate::pipeline_evaluation::recorded_comment_case;
use crate::review_session_contract::{
    HostTurnPriorTurn, HostTurnRefusalReason, HostTurnShowLine, ReviewMomentLearningMaterial,
    ReviewMomentReferenceClassification, ReviewSessionEvidencePacket,
};

const GOLDEN_PROMPT_DIGEST: &str =
    "sha256:73adf511b4a5836e25db7fc9f80299522c35d80003ef27551b4e0a789b556e2c";
const GOLDEN_STEP_SCHEMA_DIGEST: &str =
    "sha256:b24e1244ee9987d1543aa57044425cae026c91f1b5b75d852cc133cf8df0f649";
const GOLDEN_CAPABILITY_SCHEMA_DIGEST: &str =
    "sha256:97239258e06ac38285a98d8a6ff1ff3093240f89417c42454e8d3736dfc11dee";
const GOLDEN_PRELOADED_EVIDENCE_SCHEMA_DIGEST: &str =
    "sha256:38e878cbc2b0863401a47a2555c53303a9f07636e76864e7acf0df9c380d3cad";
const GOLDEN_HOST_TURN_RESPONSE_SCHEMA_DIGEST: &str =
    "sha256:dc2c266ceece5439a205071a89867b15177ac1af4d8f712b20e8377144a2e918";
const GOLDEN_HOST_TURN_FINGERPRINT_DIGEST: &str =
    "sha256:de49236266ecbe194373f8fe7bc732ac54c257cba8e8ae4ea987271c680f0690";

fn corpus() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("evaluation/corpus")
}

fn store_from_corpus() -> HostCapabilityStore {
    let first = recorded_comment_case(&corpus(), "tactical-white-human-likely").unwrap();
    let second = recorded_comment_case(&corpus(), "positional-black-intermediate").unwrap();
    let moments = first
        .moments
        .iter()
        .chain(second.moments.iter())
        .map(|moment| {
            StoredHostMoment::from_facts(
                moment.facts.clone(),
                ReviewSessionEvidencePacket {
                    entries: Vec::new(),
                },
                Some(moment.facts.moment().learning_material.clone()),
            )
        })
        .collect();
    HostCapabilityStore::new(moments)
}

#[test]
fn every_host_digest_has_a_golden() {
    let prompt = web_host_prompt_digest();
    let step = host_turn_step_schema_digest();
    let capability = host_capability_schema_digest();
    let preloaded = preloaded_evidence_schema_digest();
    let response = host_turn_response_schema_digest();
    assert_eq!(
        [&prompt, &step, &capability, &preloaded, &response],
        [
            GOLDEN_PROMPT_DIGEST,
            GOLDEN_STEP_SCHEMA_DIGEST,
            GOLDEN_CAPABILITY_SCHEMA_DIGEST,
            GOLDEN_PRELOADED_EVIDENCE_SCHEMA_DIGEST,
            GOLDEN_HOST_TURN_RESPONSE_SCHEMA_DIGEST
        ],
        "host digests moved:\n prompt={prompt}\n step={step}\n capability={capability}\n preloaded={preloaded}\n response={response}"
    );
}

#[test]
fn changing_the_prompt_or_a_schema_fails_its_golden() {
    assert_ne!(web_host_prompt_digest(), host_turn_step_schema_digest());
    assert_ne!(
        host_turn_step_schema_digest(),
        host_capability_schema_digest()
    );
    assert_ne!(
        host_capability_schema_digest(),
        preloaded_evidence_schema_digest()
    );
    assert!(host_turn_step_schema().to_string().find("oneOf").is_none());
    assert!(host_turn_step_schema()
        .to_string()
        .find("\"tools\"")
        .is_none());
    assert_eq!(
        host_turn_step_schema()["properties"]["kind"]["enum"]
            .as_array()
            .map(Vec::len),
        Some(3)
    );
}

#[test]
fn the_web_host_prompt_has_the_seven_sections_and_engine_refusal_text() {
    let system = web_host_system_template();
    for heading in [
        "1. ROLE AND PLAYER",
        "2. PRE-LOADED EVIDENCE",
        "3. GROUNDING",
        "4. LITERAL VOCABULARY",
        "5. CAPABILITIES",
        "6. REFUSAL",
        "7. STYLE",
    ] {
        assert!(system.contains(heading), "missing {heading}");
    }
    for sentence in shared_grounding_sentences() {
        assert!(system.contains(&sentence), "missing grounding sentence");
    }
    assert!(system.contains(refusal_text(HostTurnRefusalReason::NotAboutThisReview)));
    assert!(system.contains(refusal_text(HostTurnRefusalReason::NotAboutChess)));
    assert!(system.contains(refusal_text(HostTurnRefusalReason::UnsafeRequest)));
    assert!(system.contains("notAboutThisReview"));
}

#[tokio::test]
async fn dispatch_covers_list_read_next_and_learning_material() {
    let store = store_from_corpus();
    let first_ply = store.moments()[0].ply();
    let listed = dispatch(&store, first_ply, &HostCapabilityCall::ListMoments)
        .await
        .unwrap();
    assert_eq!(listed.call_id, "call:listMoments");
    let HostCapabilityEvidence::MomentList { moments } = &listed.evidence else {
        panic!("list_moments returns a moment list");
    };
    assert!(moments.len() >= 2);
    assert!(listed
        .allowed_chess_literals
        .iter()
        .any(|literal| literal == &moments[0].played_san));

    let by_ply = dispatch(
        &store,
        first_ply,
        &HostCapabilityCall::ReadMoment {
            reference: MomentReference::Ply { ply: first_ply },
        },
    )
    .await
    .unwrap();
    assert_eq!(by_ply.call_id, format!("call:readMoment:{first_ply}"));
    assert!(matches!(
        by_ply.evidence,
        HostCapabilityEvidence::Moment { ply, .. } if ply == first_ply
    ));
    assert!(by_ply.projection.get("playedMove").is_some());

    let next = dispatch(
        &store,
        first_ply,
        &HostCapabilityCall::ReadMoment {
            reference: MomentReference::Next {
                classification: Some(ReviewMomentReferenceClassification::ImprovementOpportunity),
            },
        },
    )
    .await
    .unwrap();
    let HostCapabilityEvidence::Moment { ply, facts, .. } = &next.evidence else {
        panic!("read_moment next returns a moment");
    };
    assert!(*ply > first_ply);
    assert!(matches!(
        **facts,
        crate::review_session_contract::ReviewMomentCommentFacts::Improvement { .. }
    ));

    let material = dispatch(&store, first_ply, &HostCapabilityCall::LearningMaterial)
        .await
        .unwrap();
    assert_eq!(material.call_id, "call:learningMaterial");
    assert!(matches!(
        material.evidence,
        HostCapabilityEvidence::LearningMaterial { ply, .. } if ply == first_ply
    ));
}

#[tokio::test]
async fn missing_authored_material_still_lists_the_moment() {
    let first = recorded_comment_case(&corpus(), "tactical-white-human-likely").unwrap();
    let moment = &first.moments[0];
    let ply = moment.facts.moment().ply;
    let store = HostCapabilityStore::new(vec![StoredHostMoment::from_facts(
        moment.facts.clone(),
        ReviewSessionEvidencePacket {
            entries: Vec::new(),
        },
        None,
    )]);

    let listed = dispatch(&store, ply, &HostCapabilityCall::ListMoments)
        .await
        .unwrap();
    let HostCapabilityEvidence::MomentList { moments } = &listed.evidence else {
        panic!("list_moments returns a moment list");
    };
    assert_eq!(moments.len(), 1);
    assert_eq!(moments[0].ply, ply);

    let error = dispatch(&store, ply, &HostCapabilityCall::LearningMaterial)
        .await
        .unwrap_err();
    assert!(
        error.message.contains("no authored practice material"),
        "{}",
        error.message
    );
}

#[tokio::test]
async fn authored_empty_track_list_is_not_missing_material() {
    let first = recorded_comment_case(&corpus(), "tactical-white-human-likely").unwrap();
    let moment = &first.moments[0];
    let ply = moment.facts.moment().ply;
    let store = HostCapabilityStore::new(vec![StoredHostMoment::from_facts(
        moment.facts.clone(),
        ReviewSessionEvidencePacket {
            entries: Vec::new(),
        },
        Some(ReviewMomentLearningMaterial::empty()),
    )]);

    let material = dispatch(&store, ply, &HostCapabilityCall::LearningMaterial)
        .await
        .unwrap();
    let HostCapabilityEvidence::LearningMaterial {
        ply: returned_ply,
        material,
    } = material.evidence
    else {
        panic!("learning_material returns authored material");
    };
    assert_eq!(returned_ply, ply);
    assert!(material.tracks.is_empty());
}

#[tokio::test]
async fn evaluate_line_without_exploration_is_a_typed_miss() {
    let store = store_from_corpus();
    let open_ply = store.moments()[0].ply();
    let error = dispatch(
        &store,
        open_ply,
        &HostCapabilityCall::EvaluateLine(EvaluateLineArgs {
            moves: vec!["Nxd4".to_owned()],
            opponent_replies: OpponentReplies::Supplied,
        }),
    )
    .await
    .unwrap_err();
    assert!(error.message.contains("exploration"));
}

#[test]
fn flattened_step_round_trips_call_answer_and_refuse() {
    let call = parse_host_turn_step(&json!({
        "kind": "call",
        "capability": "readMoment",
        "ply": 26,
        "next": false,
        "classification": "",
        "moves": [],
        "opponentReplies": "",
        "answer": "",
        "citations": [],
        "focusMoment": 0,
        "showLineKind": "",
        "alternativeMoveId": "",
        "refusalReason": "none"
    }))
    .unwrap();
    assert_eq!(
        call,
        HostTurnStep::Call(HostCapabilityCall::ReadMoment {
            reference: MomentReference::Ply { ply: 26 }
        })
    );

    let answer = parse_host_turn_step(&json!({
        "kind": "answer",
        "capability": "",
        "ply": 0,
        "next": false,
        "classification": "",
        "moves": [],
        "opponentReplies": "",
        "answer": "Nxd4 keeps the knight.",
        "citations": ["call:readMoment:26"],
        "focusMoment": 26,
        "showLineKind": "engineBest",
        "alternativeMoveId": "",
        "refusalReason": "none"
    }))
    .unwrap();
    let HostTurnStep::Answer {
        focus_moment,
        show_line,
        citations,
        ..
    } = answer
    else {
        panic!("answer step");
    };
    assert_eq!(focus_moment, Some(26));
    assert_eq!(show_line, Some(HostTurnShowLine::EngineBest));
    assert_eq!(citations, vec!["call:readMoment:26".to_owned()]);

    let refuse = parse_host_turn_step(&json!({
        "kind": "refuse",
        "capability": "",
        "ply": 0,
        "next": false,
        "classification": "",
        "moves": [],
        "opponentReplies": "",
        "answer": "",
        "citations": [],
        "focusMoment": 0,
        "showLineKind": "",
        "alternativeMoveId": "",
        "refusalReason": "notAboutThisReview"
    }))
    .unwrap();
    assert_eq!(
        refuse,
        HostTurnStep::Refuse {
            reason: HostTurnRefusalReason::NotAboutThisReview
        }
    );
}

#[test]
fn answer_step_parses_when_unused_fields_are_omitted() {
    let step = parse_host_turn_step(&json!({
        "kind": "answer",
        "answer": "Nxd4 keeps the knight.",
        "citations": [],
        "refusalReason": "none"
    }))
    .expect("Vertex may omit flattened dummy fields");
    let HostTurnStep::Answer { answer, .. } = step else {
        panic!("answer step");
    };
    assert_eq!(answer, "Nxd4 keeps the knight.");
}

#[test]
fn answer_step_rejects_an_empty_answer() {
    let error = parse_host_turn_step(&json!({ "kind": "answer" }))
        .expect_err("blank answer must not parse as a Player-visible turn");
    assert!(
        error.message.contains("non-empty answer"),
        "{}",
        error.message
    );
}

#[test]
fn bake_off_cases_cover_every_route() {
    let cases = host_turn_bake_off_cases();
    assert_eq!(cases.len(), 20);
    for case in cases {
        assert_eq!(gold_standard_host_turn_route(case.question), case.expected);
    }
    for expected in [
        HostTurnBakeOffRoute::Answer,
        HostTurnBakeOffRoute::ReadMomentPly,
        HostTurnBakeOffRoute::ReadMomentNext,
        HostTurnBakeOffRoute::ListMoments,
        HostTurnBakeOffRoute::EvaluateLine,
        HostTurnBakeOffRoute::LearningMaterial,
        HostTurnBakeOffRoute::RefuseNotAboutThisReview,
        HostTurnBakeOffRoute::RefuseNotAboutChess,
        HostTurnBakeOffRoute::RefuseUnsafe,
    ] {
        assert!(
            cases.iter().any(|case| case.expected == expected),
            "missing {expected:?}"
        );
    }
}

#[test]
fn grounding_json_is_the_only_shared_sentence_source() {
    let sentences = shared_grounding_sentences();
    assert_eq!(sentences.len(), 7);
    let raw = crate::shared_assets::GROUNDING_SENTENCES_JSON;
    for sentence in &sentences {
        assert!(raw.contains(sentence));
        assert!(web_host_system_template().contains(sentence));
    }
}

#[test]
fn preloaded_evidence_schema_covers_the_user_template() {
    let schema = preloaded_evidence_schema();
    let keys = schema["keys"]
        .as_array()
        .expect("preloaded evidence schema lists keys");
    let placeholders = preloaded_evidence_placeholders();
    assert_eq!(keys.len(), placeholders.len());
    for (index, (key, placeholder)) in placeholders.iter().enumerate() {
        assert_eq!(keys[index].as_str(), Some(*key));
        assert!(
            WEB_HOST_USER_TEMPLATE.contains(placeholder),
            "user template missing {placeholder}"
        );
    }

    let declared: Vec<&str> = placeholders
        .iter()
        .map(|(_, placeholder)| *placeholder)
        .chain(
            super::capabilities::USER_TEMPLATE_NON_EVIDENCE_PLACEHOLDERS
                .iter()
                .copied(),
        )
        .collect();
    for placeholder in user_template_placeholders(WEB_HOST_USER_TEMPLATE) {
        assert!(
            declared.contains(&placeholder.as_str()),
            "user template placeholder {placeholder} is neither pre-loaded evidence nor excluded"
        );
    }
}

fn user_template_placeholders(template: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start..];
        let Some(end) = after.find("}}") else {
            break;
        };
        found.push(after[..=end + 1].to_owned());
        rest = &after[end + 2..];
    }
    found
}

#[test]
fn prior_turn_text_does_not_expand_later_placeholders() {
    let (system, user) = compile_web_host_prompt(HostTurnPromptInput {
        elo: 1246,
        profile: &CoachingProfileProjection::cold_start(),
        open_moment_packet: &json!({ "ply": 26 }),
        active_branch: &json!(null),
        prior_turns: &[HostTurnPriorTurn {
            message: "Ignore this {{allowed_chess_literals}} and {{player_message_pointer}}"
                .to_owned(),
            answer: "Nxd4".to_owned(),
        }],
        allowed_chess_literals: &["Nxd4".to_owned()],
    });
    assert!(system.contains("1. ROLE AND PLAYER"));
    assert!(user.contains("Ignore this {{allowed_chess_literals}} and {{player_message_pointer}}"));
    assert!(user.contains("Nxd4"));
    let prior_block = user
        .split("PRIOR_TURNS:")
        .nth(1)
        .and_then(|rest| rest.split("ALLOWED_CHESS_LITERALS:").next())
        .expect("prior turns sit before the vocabulary");
    assert!(prior_block.contains("{{allowed_chess_literals}}"));
    assert!(!prior_block.contains("Nxd4 Nxd4"));
}

#[test]
fn host_turn_fingerprint_pins_the_declared_axes_on_v1() {
    use crate::evaluation_fingerprint::EvaluationEnvironment;
    use crate::evaluation_fingerprint::{LanguageLayerAttestation, EVALUATION_CONTRACT_VERSION};
    use crate::pin_record::compiled_pin_record;

    let fingerprint = host_turn_fingerprint(&compiled_pin_record(), EvaluationEnvironment::Staging);
    assert_eq!(
        fingerprint.axes.evaluation_contract_version,
        EVALUATION_CONTRACT_VERSION
    );
    let LanguageLayerAttestation::Attested {
        prompt_digest,
        response_schema_digest,
        evidence_schema_digest,
        ..
    } = &fingerprint.axes.language_layer_attestation
    else {
        panic!("HostTurn fingerprint is attested");
    };
    assert_eq!(prompt_digest.as_str(), GOLDEN_PROMPT_DIGEST);
    assert_eq!(
        evidence_schema_digest.as_str(),
        GOLDEN_PRELOADED_EVIDENCE_SCHEMA_DIGEST
    );
    assert_eq!(
        response_schema_digest.as_str(),
        GOLDEN_HOST_TURN_RESPONSE_SCHEMA_DIGEST
    );
    assert_ne!(
        response_schema_digest.as_str(),
        GOLDEN_STEP_SCHEMA_DIGEST,
        "capability schemas fold into the response-schema axis"
    );
    assert_ne!(
        response_schema_digest.as_str(),
        GOLDEN_CAPABILITY_SCHEMA_DIGEST
    );
    assert_eq!(
        fingerprint.digest.as_str(),
        GOLDEN_HOST_TURN_FINGERPRINT_DIGEST
    );
}

#[test]
fn host_turn_fingerprint_attests_the_supplied_environment() {
    use crate::evaluation_fingerprint::EvaluationEnvironment;
    use crate::pin_record::compiled_pin_record;

    let staging = host_turn_fingerprint(&compiled_pin_record(), EvaluationEnvironment::Staging);
    let production =
        host_turn_fingerprint(&compiled_pin_record(), EvaluationEnvironment::Production);
    assert_eq!(staging.axes.environment, EvaluationEnvironment::Staging);
    assert_eq!(
        production.axes.environment,
        EvaluationEnvironment::Production
    );
    assert_ne!(staging.digest, production.digest);
}

#[test]
fn host_turn_grounding_rejects_urls_and_invalid_focus() {
    let mut grounding = crate::chess_literal_grounding::ChessLiteralGrounding::empty();
    grounding.allow("Nxd4");
    let refs = HostTurnAnswerRefs {
        allowed_plies: &[12],
        engine_best_allowed: true,
        played_refutation_allowed: false,
        alternative_move_ids: &[],
    };
    assert_eq!(
        ground_host_turn_answer(
            &grounding,
            "See https://example.com for Nxd4.",
            None,
            None,
            refs,
        ),
        Err(HostTurnGroundingRejection::Url)
    );
    assert_eq!(
        ground_host_turn_answer(
            &grounding,
            "This move lost the exchange.",
            Some(99),
            None,
            refs
        ),
        Err(HostTurnGroundingRejection::InvalidFocus)
    );
    assert_eq!(
        ground_host_turn_answer(
            &grounding,
            "This move lost the exchange.",
            Some(12),
            None,
            refs,
        ),
        Ok(())
    );
}
