use super::*;

#[tokio::test]
async fn successful_handoff_terminalizes_the_pre_provider_claim() {
    let store = InMemoryDigestEmailStore::default();
    let owner = DailyCoachingOwnerKey::for_player(
        &PlayerId::try_from("terminal-delivery-player".to_string()).unwrap(),
    );
    let claimed_at = DateTime::parse_from_rfc3339("2026-08-11T10:00:00Z")
        .unwrap()
        .with_timezone(&Utc);

    let DeliveryClaim::Claimed(lease) = store
        .claim(
            &owner,
            "daily-2026-08-10",
            claimed_at,
            TimeDelta::minutes(5),
            TimeDelta::hours(23),
        )
        .await
        .unwrap()
    else {
        panic!("the new delivery should be claimed");
    };
    store
        .finish(
            &owner,
            "daily-2026-08-10",
            lease,
            DeliveryCompletion::Sent {
                provider_message_id: "provider-message-1".to_string(),
                recipient: NormalizedEmail::parse("player@example.com").unwrap(),
            },
        )
        .await
        .unwrap();

    let inner = store.inner.lock().unwrap();
    let record = inner
        .deliveries
        .get(&(owner, "daily-2026-08-10".to_string()))
        .unwrap();
    assert_eq!(record.claimed_at, claimed_at);
    assert_eq!(record.attempt_count, 1);
    assert_eq!(record.status, DeliveryStatus::Sent);
    assert_eq!(
        record.recipient.as_ref().map(NormalizedEmail::as_str),
        Some("player@example.com")
    );
    assert_eq!(
        record.provider_message_id.as_deref(),
        Some("provider-message-1")
    );
    assert_eq!(record.suppression_reason, None);
}

#[tokio::test]
async fn pending_claim_recovery_is_lease_fenced_and_bounded_by_provider_idempotency() {
    let store = InMemoryDigestEmailStore::default();
    let owner = DailyCoachingOwnerKey::for_player(
        &PlayerId::try_from("recoverable-delivery-player".to_string()).unwrap(),
    );
    let first_claim = DateTime::parse_from_rfc3339("2026-08-11T10:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let lease_ttl = TimeDelta::minutes(5);
    let retry_horizon = TimeDelta::hours(23);

    let DeliveryClaim::Claimed(first_lease) = store
        .claim(
            &owner,
            "daily-2026-08-10",
            first_claim,
            lease_ttl,
            retry_horizon,
        )
        .await
        .unwrap()
    else {
        panic!("the new delivery should be claimed");
    };
    assert_eq!(
        store
            .claim(
                &owner,
                "daily-2026-08-10",
                first_claim + lease_ttl - TimeDelta::milliseconds(1),
                lease_ttl,
                retry_horizon,
            )
            .await
            .unwrap(),
        DeliveryClaim::AlreadyClaimed
    );
    let DeliveryClaim::Claimed(second_lease) = store
        .claim(
            &owner,
            "daily-2026-08-10",
            first_claim + lease_ttl,
            lease_ttl,
            retry_horizon,
        )
        .await
        .unwrap()
    else {
        panic!("the expired lease should be reclaimed");
    };
    assert_ne!(first_lease, second_lease);

    store
        .finish(
            &owner,
            "daily-2026-08-10",
            first_lease,
            DeliveryCompletion::Sent {
                provider_message_id: "stale-provider-message".to_string(),
                recipient: NormalizedEmail::parse("player@example.com").unwrap(),
            },
        )
        .await
        .unwrap();

    assert_eq!(
        store
            .claim(
                &owner,
                "daily-2026-08-10",
                first_claim + retry_horizon,
                lease_ttl,
                retry_horizon,
            )
            .await
            .unwrap(),
        DeliveryClaim::AlreadyClaimed
    );

    let inner = store.inner.lock().unwrap();
    let record = inner
        .deliveries
        .get(&(owner, "daily-2026-08-10".to_string()))
        .unwrap();
    assert_eq!(record.first_claimed_at, first_claim);
    assert_eq!(record.claimed_at, first_claim + lease_ttl);
    assert_eq!(record.attempt_count, 2);
    assert_eq!(record.status, DeliveryStatus::Suppressed);
    assert_eq!(
        record.suppression_reason,
        Some(DeliverySuppressionReason::RetryWindowExpired)
    );
}
