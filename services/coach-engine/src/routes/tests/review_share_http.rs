use std::sync::Arc;

use axum::{
    body::{to_bytes, Body},
    http::{header, Method, Request, StatusCode},
};
use tokio::sync::mpsc;
use tower::ServiceExt;

use crate::{
    auth::AuthConfig,
    quality_capture::NoQualityCaptureStore,
    review_durability::game_import_id,
    review_session_contract::{
        EloRating, GameImportId, GameInputSource, GameReview, OperationCompletion, OperationId,
        PlayerId, RequestId, ReviewSessionEvent, ReviewSessionEventEnvelope, ReviewSide,
    },
    review_session_game_identity::ReviewSessionGameIdentity,
    review_session_processor::{ProcessorCommandAdmission, ProcessorPrincipal},
    review_session_transport::{ReviewSessionCommandExecutor, ReviewSessionWebBinding},
    types::{AppState, SharedState},
};

use crate::routes::firebase_token_test_support::{
    firebase_token as valid_token, jwt_jwks, FIREBASE_PROJECT_ID,
};

const OWNER: &str = "firebase-share-owner";
const STRANGER: &str = "firebase-share-stranger";

#[tokio::test]
async fn only_the_owner_can_mint_a_share_and_only_for_their_own_review() {
    let state = state();

    let anonymous = mint(&state, None, OWNER).await;
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);

    let stranger = mint(&state, Some(STRANGER), OWNER).await;
    assert_eq!(
        stranger.status(),
        StatusCode::FORBIDDEN,
        "a Player must not mint a link to a review that is not theirs"
    );

    let owner = mint(&state, Some(OWNER), OWNER).await;
    assert_eq!(owner.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn a_minted_link_resolves_without_signing_in_and_stops_when_it_is_withdrawn() {
    let state = state();
    let minted = minted_share(&state).await;
    let share_token = minted["shareToken"].as_str().expect("a minted token");

    let resolved = json(resolve(&state, share_token).await, StatusCode::OK).await;
    assert_eq!(
        resolved["gameImportId"].as_str(),
        Some(owned_game_import_id(OWNER).as_str()),
    );

    let revoked = crate::app(state.clone())
        .oneshot(
            authenticated(
                Method::POST,
                &format!(
                    "/api/v1/review-shares/{}/revoke",
                    minted["shareId"].as_str().expect("a minted share id")
                ),
                Some(OWNER),
            )
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .expect("the revoke request completes");
    assert_eq!(revoked.status(), StatusCode::NO_CONTENT);

    let after = resolve(&state, share_token).await;
    assert_eq!(after.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_withdrawn_link_stops_reading_the_review_it_used_to_open() {
    let state = state();
    let minted = minted_share(&state).await;
    let share_token = minted["shareToken"]
        .as_str()
        .expect("a minted token")
        .to_string();

    let read = read_shared(&state, &share_token).await;
    assert_eq!(
        read.status(),
        StatusCode::OK,
        "a live grant answers a read with the Coach Engine's terminal event"
    );

    crate::app(state.clone())
        .oneshot(
            authenticated(
                Method::POST,
                &format!(
                    "/api/v1/review-shares/{}/revoke",
                    minted["shareId"].as_str().expect("a minted share id")
                ),
                Some(OWNER),
            )
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .expect("the revoke request completes");

    assert_eq!(
        read_shared(&state, &share_token).await.status(),
        StatusCode::NOT_FOUND,
        "the grant is resolved again for every read, not once for the page"
    );
}

#[tokio::test]
async fn an_owner_lists_their_own_outstanding_links_and_never_their_tokens() {
    let state = state();
    let minted = minted_share(&state).await;

    let anonymous = crate::app(state.clone())
        .oneshot(
            authenticated(Method::GET, "/api/v1/review-shares", None)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("the list request completes");
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);

    let listed = json(
        crate::app(state.clone())
            .oneshot(
                authenticated(Method::GET, "/api/v1/review-shares", Some(OWNER))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("the list request completes"),
        StatusCode::OK,
    )
    .await;

    let shares = listed["shares"].as_array().expect("a share list");
    assert_eq!(shares.len(), 1);
    assert_eq!(shares[0]["shareId"], minted["shareId"]);
    // The only copy of the secret left with the link. Listing hands back the
    // name a Player withdraws by and nothing that could open the review.
    assert!(!listed
        .to_string()
        .contains(minted["shareToken"].as_str().expect("a minted token")));
    assert_eq!(
        json(
            crate::app(state.clone())
                .oneshot(
                    authenticated(Method::GET, "/api/v1/review-shares", Some(STRANGER))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .expect("the list request completes"),
            StatusCode::OK,
        )
        .await["shares"]
            .as_array()
            .map(Vec::len),
        Some(0),
        "one Player's outstanding links are not another's to see"
    );
}

#[tokio::test]
async fn a_moment_the_review_does_not_contain_is_refused_at_mint() {
    let state = state();

    let response = crate::app(state.clone())
        .oneshot(
            authenticated(Method::POST, "/api/v1/review-shares", Some(OWNER))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "gameImportId": owned_game_import_id(OWNER),
                        "reviewMomentId": "critical-moment:not-in-this-review",
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .expect("the mint request completes");

    // A link that looks live and dies on the recipient's screen is the failure
    // this whole redesign exists to remove, so it is refused at the mint.
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        json(response, StatusCode::NOT_FOUND).await["reason"],
        "unknownAddress"
    );
}

#[tokio::test]
async fn a_token_that_was_never_minted_is_refused_by_shape() {
    let state = state();

    assert_eq!(
        resolve(&state, "not-a-share-token").await.status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        resolve(
            &state,
            &format!("review-share:{}:{}", "a".repeat(64), "b".repeat(64))
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
}

async fn minted_share(state: &SharedState) -> serde_json::Value {
    json(mint(state, Some(OWNER), OWNER).await, StatusCode::CREATED).await
}

async fn mint(
    state: &SharedState,
    subject: Option<&str>,
    review_owner: &str,
) -> axum::response::Response {
    let body = serde_json::json!({
        "gameImportId": owned_game_import_id(review_owner),
        "reviewMomentId": shared_moment_id(),
    });
    crate::app(state.clone())
        .oneshot(
            authenticated(Method::POST, "/api/v1/review-shares", subject)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .expect("the mint request completes")
}

async fn resolve(state: &SharedState, share_token: &str) -> axum::response::Response {
    public_post(
        state,
        "/api/v1/review-shares/resolve",
        serde_json::json!({ "shareToken": share_token }),
    )
    .await
}

async fn read_shared(state: &SharedState, share_token: &str) -> axum::response::Response {
    public_post(
        state,
        "/api/v1/review-shares/read",
        serde_json::json!({ "shareToken": share_token, "resource": "gameReview" }),
    )
    .await
}

async fn public_post(
    state: &SharedState,
    uri: &str,
    body: serde_json::Value,
) -> axum::response::Response {
    crate::app(state.clone())
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(uri)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .expect("the request completes")
}

fn authenticated(method: Method, uri: &str, subject: Option<&str>) -> axum::http::request::Builder {
    let builder = Request::builder().method(method).uri(uri);
    match subject {
        Some(subject) => builder.header(
            header::AUTHORIZATION,
            format!("Bearer {}", valid_token(subject)),
        ),
        None => builder,
    }
}

async fn json(response: axum::response::Response, expected: StatusCode) -> serde_json::Value {
    assert_eq!(response.status(), expected);
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
        .expect("a JSON response body")
}

fn owned_game_import_id(player: &str) -> String {
    game_import_id(
        &ProcessorPrincipal::Player(PlayerId::try_from(player.to_string()).unwrap()),
        &ReviewSessionGameIdentity::from_request(
            &GameInputSource::LichessUrl {
                url: "https://lichess.org/Synthet1".to_string(),
            },
            ReviewSide::Black,
            EloRating::try_from(1450).unwrap(),
        )
        .unwrap(),
    )
    .as_str()
    .to_string()
}

fn state() -> SharedState {
    Arc::new(AppState {
        account_deletion: crate::account_deletion::AccountDeletionRuntime::disabled(),
        auth: AuthConfig::new_firebase(FIREBASE_PROJECT_ID, jwt_jwks())
            .expect("test key should be valid"),
        beta_access: crate::beta_access::BetaAccessRuntime::disabled(),
        daily_coaching: crate::daily_coaching::DailyCoachingRuntime::disabled(),
        imported_games: crate::imported_games::ImportedGamesRuntime::in_memory(),
        opening_analysis: crate::opening_analysis::OpeningAnalysisRuntime::disabled(),
        review_session: ReviewSessionWebBinding::new(Arc::new(SnapshotExecutor))
            .with_quality_capture_store(Arc::new(NoQualityCaptureStore)),
    })
}

/// The generated contract fixture's Game Review, which every share here is
/// minted over. Minting checks that the address names a Critical Moment the
/// review actually has, so the tests share a moment that exists.
fn fixture_review() -> GameReview {
    let events: Vec<ReviewSessionEventEnvelope> = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../packages/coach-engine-sdk/fixtures/events.json"
    )))
    .expect("the generated event fixtures are valid");
    events
        .into_iter()
        .find_map(|envelope| match envelope.event {
            ReviewSessionEvent::Completed { result } => match *result {
                OperationCompletion::GameImported { review, .. } => Some(*review),
                _ => None,
            },
            _ => None,
        })
        .expect("the generated fixtures contain an imported Game Review")
}

fn shared_moment_id() -> String {
    fixture_review().critical_moments[0]
        .critical_moment_id
        .as_str()
        .to_string()
}

/// A Coach Engine that answers the one read the share surface makes.
///
/// The share endpoints are the subject here, so the executor answers a Game
/// Review snapshot from the generated fixtures rather than importing a Game:
/// what these tests assert is who may mint, list, resolve, and read — never how
/// a review is produced.
struct SnapshotExecutor;

impl ReviewSessionCommandExecutor for SnapshotExecutor {
    fn submit(
        self: Arc<Self>,
        _principal: ProcessorPrincipal,
        _admission: ProcessorCommandAdmission,
    ) -> mpsc::UnboundedReceiver<ReviewSessionEventEnvelope> {
        let (sender, receiver) = mpsc::unbounded_channel();
        let _ = sender.send(ReviewSessionEventEnvelope {
            request_id: RequestId::try_from("request:review-share:test".to_string())
                .expect("a static test identifier is valid"),
            operation_id: OperationId::try_from("operation:review-share:test".to_string())
                .expect("a static test identifier is valid"),
            sequence: 0,
            event: ReviewSessionEvent::Completed {
                result: Box::new(OperationCompletion::GameReviewSnapshotRead {
                    game_import_id: GameImportId::try_from(owned_game_import_id(OWNER))
                        .expect("the fixture Game Import ID is valid"),
                    review: Box::new(fixture_review()),
                    imported_game: Box::new(fixture_imported_game()),
                    review_moments: Vec::new(),
                    content_digest: crate::review_session_contract::ReviewContentDigest::try_from(
                        format!("sha256:{}", "0".repeat(64)),
                    )
                    .expect("the fixture content digest is valid"),
                }),
            },
        });
        receiver
    }
}

fn fixture_imported_game() -> crate::review_session_contract::ImportedGame {
    serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../packages/coach-engine-sdk/fixtures/imported-game.json"
    )))
    .expect("the generated imported Game fixture is valid")
}
