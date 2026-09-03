use chrono::{TimeDelta, TimeZone, Utc};

use super::*;
use crate::{
    review_durability::game_import_id,
    review_session_contract::{EloRating, GameInputSource, ReviewSide},
    review_session_game_identity::ReviewSessionGameIdentity,
};

#[tokio::test]
async fn a_minted_link_opens_the_address_it_was_minted_for() {
    let runtime = runtime();
    let owner = player("firebase-owner");

    let minted = runtime
        .mint(&owner, address(&owner, None), now())
        .await
        .expect("an owner may share their own review");
    let resolved = runtime
        .resolve(&minted.token, now())
        .await
        .expect("a fresh link resolves");

    assert_eq!(resolved, minted.grant);
    assert_eq!(resolved.owner, owner);
}

#[tokio::test]
async fn the_link_is_not_recoverable_from_anything_that_is_stored() {
    let runtime = runtime();
    let owner = player("firebase-owner");

    let minted = runtime
        .mint(&owner, address(&owner, None), now())
        .await
        .expect("an owner may share their own review");

    assert!(
        !minted.token.contains(&minted.grant.share_id),
        "the stored name of a grant must not be the secret in its link"
    );
    assert_eq!(
        runtime
            .resolve(
                &format!(
                    "{TOKEN_PREFIX}:{}:{}",
                    player_subtree_owner(&owner),
                    minted.grant.share_id
                ),
                now()
            )
            .await,
        Err(ReviewShareError::NotFound),
        "the public share id must not act as the secret"
    );
}

#[tokio::test]
async fn expiry_is_decided_when_the_link_is_resolved() {
    let runtime = runtime().with_lifetime(TimeDelta::hours(1));
    let owner = player("firebase-owner");

    let minted = runtime
        .mint(&owner, address(&owner, None), now())
        .await
        .expect("an owner may share their own review");

    assert!(runtime
        .resolve(&minted.token, now() + TimeDelta::minutes(59))
        .await
        .is_ok());
    assert_eq!(
        runtime
            .resolve(&minted.token, now() + TimeDelta::hours(1))
            .await,
        Err(ReviewShareError::Expired)
    );
}

#[tokio::test]
async fn revoking_a_share_stops_its_link_and_repeats_harmlessly() {
    let runtime = runtime();
    let owner = player("firebase-owner");
    let minted = runtime
        .mint(&owner, address(&owner, None), now())
        .await
        .expect("an owner may share their own review");

    runtime
        .revoke(&owner, &minted.grant.share_id)
        .await
        .expect("an owner may withdraw their own share");

    assert_eq!(
        runtime.resolve(&minted.token, now()).await,
        Err(ReviewShareError::NotFound)
    );
    assert!(runtime.revoke(&owner, &minted.grant.share_id).await.is_ok());
}

#[tokio::test]
async fn an_owner_can_name_their_outstanding_links_without_holding_them() {
    let runtime = runtime().with_lifetime(TimeDelta::hours(1));
    let owner = player("firebase-owner");
    let stranger = player("firebase-stranger");
    let first = runtime
        .mint(&owner, address(&owner, None), now())
        .await
        .expect("an owner may share their own review");
    let second = runtime
        .mint(&owner, address(&owner, None), now() + TimeDelta::minutes(1))
        .await
        .expect("an owner may share one address twice");
    runtime
        .mint(&stranger, address(&stranger, None), now())
        .await
        .expect("another Player may share their own review");

    // Withdrawal has to outlive the page that minted a link, so a Player who
    // shared twice and reloaded can still name and stop both.
    let outstanding = runtime
        .outstanding(&owner, now())
        .await
        .expect("an owner may list their own shares");
    assert_eq!(
        outstanding
            .iter()
            .map(|grant| grant.share_id.clone())
            .collect::<Vec<_>>(),
        vec![second.grant.share_id.clone(), first.grant.share_id.clone()],
        "the longest-lived grant is listed first, and only this owner's"
    );

    runtime.revoke(&owner, &first.grant.share_id).await.unwrap();
    assert_eq!(
        runtime.outstanding(&owner, now()).await.unwrap().len(),
        1,
        "a withdrawn link stops being offered"
    );
    assert!(
        runtime
            .outstanding(&owner, now() + TimeDelta::hours(2))
            .await
            .unwrap()
            .is_empty(),
        "a lapsed link is not offered for withdrawal it no longer needs"
    );
}

#[tokio::test]
async fn one_link_cannot_be_read_without_limit() {
    let runtime = runtime().with_allowance(SharedReadAllowance {
        reads: 2,
        window: TimeDelta::minutes(1),
    });
    let owner = player("firebase-owner");
    let first = runtime
        .mint(&owner, address(&owner, None), now())
        .await
        .expect("an owner may share their own review");
    let second = runtime
        .mint(&owner, address(&owner, None), now())
        .await
        .expect("an owner may share one address twice");

    assert!(runtime.resolve(&first.token, now()).await.is_ok());
    assert!(runtime.resolve(&first.token, now()).await.is_ok());
    assert_eq!(
        runtime.resolve(&first.token, now()).await,
        Err(ReviewShareError::TooManyReads)
    );
    // Metered per grant: one link being hammered must not close another.
    assert!(runtime.resolve(&second.token, now()).await.is_ok());
    // The window is a window, not a lifetime cap.
    assert!(runtime
        .resolve(&first.token, now() + TimeDelta::minutes(2))
        .await
        .is_ok());
}

#[tokio::test]
async fn one_player_cannot_revoke_another_players_share() {
    let runtime = runtime();
    let owner = player("firebase-owner");
    let stranger = player("firebase-stranger");
    let minted = runtime
        .mint(&owner, address(&owner, None), now())
        .await
        .expect("an owner may share their own review");

    runtime
        .revoke(&stranger, &minted.grant.share_id)
        .await
        .expect("a revoke addresses the caller's own subtree");

    assert!(
        runtime.resolve(&minted.token, now()).await.is_ok(),
        "revoking inside one subtree must not reach another"
    );
}

#[tokio::test]
async fn a_review_owned_by_someone_else_cannot_be_shared() {
    let runtime = runtime();
    let owner = player("firebase-owner");
    let stranger = player("firebase-stranger");

    assert_eq!(
        runtime
            .mint(&stranger, address(&owner, None), now())
            .await
            .err(),
        Some(ReviewShareError::NotOwned)
    );
}

#[tokio::test]
async fn a_malformed_token_is_refused_before_the_store_is_asked() {
    let runtime = runtime();
    let owner = player("firebase-owner");
    let minted = runtime
        .mint(&owner, address(&owner, None), now())
        .await
        .expect("an owner may share their own review");
    let (_, secret) = parse_token(&minted.token).expect("a minted token parses");

    for token in [
        "".to_string(),
        secret.to_string(),
        format!("review-share:{secret}"),
        format!("review-share:{}:{secret}", "z".repeat(64)),
        format!(
            "{TOKEN_PREFIX}:{}:{secret}:extra",
            player_subtree_owner(&owner)
        ),
    ] {
        assert_eq!(
            runtime.resolve(&token, now()).await,
            Err(ReviewShareError::InvalidToken),
            "{token} must not be treated as a share link"
        );
    }
}

#[tokio::test]
async fn a_shared_continuation_keeps_the_address_it_names() {
    let runtime = runtime();
    let owner = player("firebase-owner");

    let minted = runtime
        .mint(
            &owner,
            address(&owner, Some(MoveSequencePresentationKind::EngineBest)),
            now(),
        )
        .await
        .expect("an owner may share their own review");
    let resolved = runtime
        .resolve(&minted.token, now())
        .await
        .expect("a fresh link resolves");

    assert_eq!(
        resolved.address.sequence_kind,
        Some(MoveSequencePresentationKind::EngineBest)
    );
}

fn runtime() -> ReviewShareRuntime {
    ReviewShareRuntime::new(Arc::new(InMemoryReviewShareStore::default()))
}

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 9, 12, 0, 0).unwrap()
}

fn player(id: &str) -> PlayerId {
    PlayerId::try_from(id.to_string()).expect("a test Player ID is valid")
}

fn address(
    owner: &PlayerId,
    sequence_kind: Option<MoveSequencePresentationKind>,
) -> ReviewShareAddress {
    ReviewShareAddress {
        game_import_id: game_import_id(
            &ProcessorPrincipal::Player(owner.clone()),
            &ReviewSessionGameIdentity::from_request(
                &GameInputSource::LichessUrl {
                    url: "https://lichess.org/Synthet1".to_string(),
                },
                ReviewSide::Black,
                EloRating::try_from(1450).unwrap(),
            )
            .unwrap(),
        ),
        review_moment_id: CriticalMomentId::try_from("critical-moment:fixture:1".to_string())
            .expect("a test Critical Moment ID is valid"),
        sequence_kind,
    }
}
