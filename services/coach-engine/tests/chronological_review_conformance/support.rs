use std::sync::Arc;

use chen_chess_coach_engine::{
    review_session_contract::*,
    review_session_processor::{ProcessorPrincipal, ReviewSessionProcessor},
};

use crate::{processor_support, transport_support};

const CANONICAL_URL: &str = "https://lichess.org/Synthet1Demo/black";

fn idempotency_key(label: &str) -> IdempotencyKey {
    IdempotencyKey::try_from(format!("idempotency-key:conformance:{label}")).unwrap()
}

pub(crate) struct CanonicalJourney {
    pub(crate) processor: Arc<ReviewSessionProcessor<processor_support::CapturedLichess>>,
    pub(crate) game_import_id: GameImportId,
    pub(crate) review: GameReview,
    pub(crate) review_moments: Vec<ReviewSessionCoreContract>,
    pub(crate) player_selected_material: ReviewMomentLearningMaterial,
    pub(crate) player_selected_authoring_material: Option<ReviewMomentLearningMaterial>,
    pub(crate) neutral_material: ReviewMomentLearningMaterial,
}

pub(crate) async fn run_journey(
    surface: DeliverySurface,
    principal: ProcessorPrincipal,
) -> CanonicalJourney {
    let processor = processor_support::processor(false).0;
    let imported = submit(
        &processor,
        principal.clone(),
        surface,
        "import",
        ReviewSessionCommand::ImportGame {
            source: GameInputSource::LichessUrl {
                url: CANONICAL_URL.to_string(),
            },
            review_side: RequestedReviewSide::FromQualifiedUrl,
            elo_profile: RequestedEloProfile::FromImportedMetadata,
        },
    )
    .await;
    let (game_import_id, review) = match completion(&imported) {
        OperationCompletion::GameImported {
            game_import_id,
            review,
            ..
        } => (game_import_id.clone(), review.as_ref().clone()),
        result => panic!("expected a Game import, got {result:?}"),
    };
    let started = submit(
        &processor,
        principal.clone(),
        surface,
        "start",
        ReviewSessionCommand::StartReviewSession { game_import_id },
    )
    .await;
    let (game_import_id, admitted_moments) = match completion(&started) {
        OperationCompletion::ReviewSessionStarted {
            game_import_id,
            review_moments,
            ..
        } => (game_import_id.clone(), review_moments.clone()),
        result => panic!("expected a Review Session start, got {result:?}"),
    };
    let mut review_moments = Vec::with_capacity(admitted_moments.len());
    for (index, admitted) in admitted_moments.into_iter().enumerate() {
        if let Some(core) = admitted.prepared_core() {
            review_moments.push(core.clone());
            continue;
        }
        let opened = submit(
            &processor,
            principal.clone(),
            surface,
            &format!("open-{index}"),
            ReviewSessionCommand::OpenReviewMoment {
                game_import_id: game_import_id.clone(),
                selection: admitted.review_moment.selection,
                idempotency_key: idempotency_key(&format!("open-{index}")),
            },
        )
        .await;
        match completion(&opened) {
            OperationCompletion::ReviewMomentOpened { review_moment, .. } => {
                review_moments.push(review_moment.as_ref().clone());
            }
            result => panic!("expected Review Moment preparation, got {result:?}"),
        }
    }
    let frozen_plan = serde_json::to_vec(&review.learning_plan).unwrap();
    let (player_selected_material, player_selected_authoring_material) = open_player_selected(
        &processor,
        &principal,
        surface,
        &game_import_id,
        "local-x-ray",
        45,
    )
    .await;
    let (neutral_material, _) = open_player_selected(
        &processor,
        &principal,
        surface,
        &game_import_id,
        "local-neutral",
        1,
    )
    .await;
    assert_eq!(
        serde_json::to_vec(
            &resumed_review(&processor, &principal, surface, &game_import_id)
                .await
                .learning_plan
        )
        .unwrap(),
        frozen_plan,
        "Player-selected exploration must not rewrite the frozen plan"
    );
    CanonicalJourney {
        processor,
        game_import_id,
        review,
        review_moments,
        player_selected_material,
        player_selected_authoring_material,
        neutral_material,
    }
}

async fn open_player_selected(
    processor: &Arc<ReviewSessionProcessor<processor_support::CapturedLichess>>,
    principal: &ProcessorPrincipal,
    surface: DeliverySurface,
    game_import_id: &GameImportId,
    label: &str,
    ply: u16,
) -> (
    ReviewMomentLearningMaterial,
    Option<ReviewMomentLearningMaterial>,
) {
    let opened = submit(
        processor,
        principal.clone(),
        surface,
        label,
        ReviewSessionCommand::OpenReviewMoment {
            game_import_id: game_import_id.clone(),
            selection: ReviewMomentSelection::PlayerSelectedMoment { ply },
            idempotency_key: idempotency_key(label),
        },
    )
    .await;
    match completion(&opened) {
        OperationCompletion::ReviewMomentOpened {
            review_moment,
            critical_moment,
            authoring_context,
            ..
        } => {
            assert!(matches!(
                &review_moment.review_moment.selection,
                ReviewMomentSelection::PlayerSelectedMoment { ply: selected } if *selected == ply
            ));
            let material = critical_moment.learning_material.clone();
            let authoring_material = authoring_context
                .as_ref()
                .map(|context| context.facts.moment().learning_material.clone());
            if let Some(authoring_material) = &authoring_material {
                assert_eq!(authoring_material, &material);
            }
            (material, authoring_material)
        }
        result => panic!("expected a Player-selected Review Moment, got {result:?}"),
    }
}

/// The frozen review is session-scoped rather than per-moment, so conformance
/// reads it back from the Review Session instead of from a moment open.
async fn resumed_review(
    processor: &Arc<ReviewSessionProcessor<processor_support::CapturedLichess>>,
    principal: &ProcessorPrincipal,
    surface: DeliverySurface,
    game_import_id: &GameImportId,
) -> GameReview {
    let resumed = submit(
        processor,
        principal.clone(),
        surface,
        "resume-review",
        ReviewSessionCommand::StartReviewSession {
            game_import_id: game_import_id.clone(),
        },
    )
    .await;
    match completion(&resumed) {
        OperationCompletion::ReviewSessionStarted { review, .. } => review.as_ref().clone(),
        result => panic!("expected a resumed Review Session, got {result:?}"),
    }
}

pub(crate) async fn submit(
    processor: &Arc<ReviewSessionProcessor<processor_support::CapturedLichess>>,
    principal: ProcessorPrincipal,
    surface: DeliverySurface,
    label: &str,
    command: ReviewSessionCommand,
) -> Vec<ReviewSessionEventEnvelope> {
    let envelope = transport_support::envelope(surface, label, command);
    transport_support::collect_receiver(
        processor.submit(principal, &serde_json::to_vec(&envelope).unwrap()),
    )
    .await
}

pub(crate) fn completion(events: &[ReviewSessionEventEnvelope]) -> &OperationCompletion {
    events
        .iter()
        .find_map(|event| match &event.event {
            ReviewSessionEvent::Completed { result } => Some(result.as_ref()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("expected a completed operation: {events:?}"))
}

pub(crate) fn player_id(suffix: &str) -> PlayerId {
    PlayerId::try_from(format!("conformance-{suffix}")).unwrap()
}
