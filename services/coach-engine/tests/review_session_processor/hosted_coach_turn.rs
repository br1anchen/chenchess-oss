use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use axum::{routing::post, Json, Router};
use chen_chess_coach_engine::{
    critical_moment_comment::HostedCommentRuntime,
    evaluation_fingerprint::EvaluationEnvironment,
    language_layer_ledger::{
        LanguageLayerAdmissionConfig, LanguageLayerLedger, MemoryLanguageLayerLedger,
        ProviderConcurrency,
    },
    language_layer_provider::LanguageLayerProvider,
    pin_record::{compiled_pin_record, fingerprint_from_pin},
    review_session_contract::*,
    review_session_processor::{ProcessorPrincipal, ReviewSessionProcessor},
};
use serde_json::json;

use super::*;

#[tokio::test]
async fn web_start_coach_turn_is_rejected_before_admission() {
    let (base, hits) = spawn_turn_server().await;
    let (processor, ledger) = bound_turn_processor(&base);
    let principal = ProcessorPrincipal::Player(
        PlayerId::try_from("player:hosted-turn:web".to_string()).unwrap(),
    );
    let (game_import_id, core) = import_and_start(&processor, principal.clone()).await;
    let command = coach_command(&processor, &principal, game_import_id, &core, "hosted-web").await;
    let mut envelope = command;
    envelope.surface = DeliverySurface::Web;
    let events = submit(&processor, principal, envelope).await;
    assert!(
        events.iter().any(|event| matches!(
            &event.event,
            ReviewSessionEvent::Rejected {
                reason: CommandRejectionReason::InvalidCommand,
                ..
            }
        )),
        "Web StartCoachTurn must be rejected after the web half of Coach Turn retired: {events:?}"
    );
    assert_eq!(hits.load(Ordering::SeqCst), 0);
    assert!(ledger.records().await.unwrap().is_empty());
}

#[tokio::test]
async fn other_surfaces_and_unbound_runtimes_still_prepare_for_a_host_model() {
    let (base, hits) = spawn_turn_server().await;
    let (bound, ledger) = bound_turn_processor(&base);
    let player = ProcessorPrincipal::Player(
        PlayerId::try_from("player:hosted-turn:surfaces".to_string()).unwrap(),
    );
    let (game_import_id, core) = import_and_start(&bound, player.clone()).await;
    let mut coach_app =
        coach_command(&bound, &player, game_import_id, &core, "hosted-coach-app").await;
    coach_app.surface = DeliverySurface::CoachApp;
    let prepared = submit(&bound, player.clone(), coach_app).await;
    assert!(prepared.iter().find_map(coach_turn_preparation).is_some());
    assert_eq!(hits.load(Ordering::SeqCst), 0);
    assert!(ledger.records().await.unwrap().is_empty());

    let (unbound, _, _) = processor(false);
    let unbound_player = ProcessorPrincipal::Player(
        PlayerId::try_from("player:hosted-turn:unbound".to_string()).unwrap(),
    );
    let (unbound_id, unbound_core) = import_and_start(&unbound, unbound_player.clone()).await;
    let mut web = coach_command(
        &unbound,
        &unbound_player,
        unbound_id,
        &unbound_core,
        "hosted-unbound-web",
    )
    .await;
    web.surface = DeliverySurface::Web;
    let rejected = submit(&unbound, unbound_player, web).await;
    assert!(rejected.iter().any(|event| matches!(
        &event.event,
        ReviewSessionEvent::Rejected {
            reason: CommandRejectionReason::InvalidCommand,
            ..
        }
    )));
    assert_eq!(hits.load(Ordering::SeqCst), 0);
}

fn bound_turn_processor(
    provider_base: &str,
) -> (
    Arc<ReviewSessionProcessor<CapturedLichess>>,
    Arc<MemoryLanguageLayerLedger>,
) {
    let recording = support::provider_recording();
    let pin = compiled_pin_record();
    let ledger = Arc::new(MemoryLanguageLayerLedger::new());
    let hosted = Arc::new(HostedCommentRuntime::new(
        Arc::new(LanguageLayerProvider::from_client_at(
            reqwest::Client::new(),
            "test",
            provider_base,
        )),
        pin.clone(),
        fingerprint_from_pin(&pin, EvaluationEnvironment::Staging),
        ledger.clone(),
        Arc::new(ProviderConcurrency::new(4)),
        LanguageLayerAdmissionConfig::conservative_defaults(),
    ));
    let built = ReviewSessionProcessor::new(
        CapturedLichess::new(),
        recording.clone(),
        Arc::new(support::RecordingEngine::new(&recording)),
        Arc::new(support::RecordingHuman::new(&recording, false)),
        Arc::new(support::GroundedAuthor),
    )
    .unwrap()
    .with_language_layer_ledger(ledger.clone())
    .with_hosted_comment(hosted);
    (Arc::new(built), ledger)
}

async fn spawn_turn_server() -> (String, Arc<AtomicUsize>) {
    let hits = Arc::new(AtomicUsize::new(0));
    let counted = hits.clone();
    let pin = compiled_pin_record();
    let model = pin.model.clone();
    let app = Router::new()
        .route(
            "/generation",
            axum::routing::get(move || {
                let model = model.clone();
                async move {
                    Json(json!({
                        "data": {
                            "model": model,
                            "provider_name": "Google Vertex",
                            "data_region": "global",
                            "provider_responses": [{
                                "endpoint_id": "ep-hosted-turn",
                                "routed_service_tier": null
                            }]
                        }
                    }))
                }
            }),
        )
        .route(
            "/chat/completions",
            post(move || {
                let counted = counted.clone();
                async move {
                    counted.fetch_add(1, Ordering::SeqCst);
                    Json(json!({
                        "id": "gen-hosted-turn",
                        "choices": [{
                            "message": {
                                "content": "{\"kind\":\"assessment\",\"objectiveQuality\":\"By the engine's reckoning {alternativeMove} lands at {alternativeEval}, against {bestMove} at {bestEval}.\",\"findability\":\"Whether {alternativeMove} turns up at the board is the real question here.\",\"resilience\":\"After {alternativeMove} the reply that decides it is {strongestReply}.\",\"refusalReason\":\"none\"}"
                            },
                            "finish_reason": "stop"
                        }],
                        "usage": { "prompt_tokens": 12, "completion_tokens": 8, "cost": 0.001 }
                    }))
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });
    (format!("http://{addr}"), hits)
}
