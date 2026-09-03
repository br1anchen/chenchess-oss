use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use chrono::Utc;
use tokio::sync::mpsc;

use crate::{
    quality_capture::{
        NoQualityCaptureStore, QualityCapturePreferenceStore, QualityCaptureRuntime,
        QualityCaptureStoreError, RetentionPreference, ReviewFeedbackReason,
    },
    request_trace::ReviewSessionTraceId,
    review_session_contract::*,
    review_session_processor::{ProcessorCommandAdmission, ProcessorPrincipal},
    review_share::{
        InMemoryReviewShareStore, MintedReviewShare, ReviewShareAddress, ReviewShareError,
        ReviewShareGrant, ReviewShareRuntime, ReviewShareStore,
    },
};

use super::ReviewSessionCommandExecutor;

/// Which of a shared review's two resources a recipient asked for.
///
/// A caller holding a share token names a resource and never a command: the
/// binding builds the command from the grant, so an unauthenticated request
/// cannot reach the command vocabulary at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SharedReviewResource {
    GameReview,
    ReviewMoment,
}

#[derive(Clone)]
pub struct ReviewSessionWebBinding {
    executor: Arc<dyn ReviewSessionCommandExecutor>,
    quality_capture: Arc<dyn QualityCapturePreferenceStore>,
    quality_runtime: Option<Arc<QualityCaptureRuntime>>,
    review_shares: Arc<ReviewShareRuntime>,
    shared_reads: Arc<AtomicU64>,
}

impl ReviewSessionWebBinding {
    pub fn new(executor: Arc<dyn ReviewSessionCommandExecutor>) -> Self {
        Self {
            executor,
            quality_capture: Arc::new(NoQualityCaptureStore),
            quality_runtime: None,
            review_shares: Arc::new(ReviewShareRuntime::new(Arc::new(
                InMemoryReviewShareStore::default(),
            ))),
            shared_reads: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn with_quality_capture_store(
        mut self,
        quality_capture: Arc<dyn QualityCapturePreferenceStore>,
    ) -> Self {
        self.quality_capture = quality_capture;
        self
    }

    pub fn with_quality_capture_runtime(mut self, runtime: Arc<QualityCaptureRuntime>) -> Self {
        self.quality_capture = runtime.preference_store();
        self.quality_runtime = Some(runtime);
        self
    }

    pub(crate) async fn record_feedback(
        &self,
        authenticated_subject: &str,
        reason_codes: Vec<ReviewFeedbackReason>,
    ) -> Result<(), QualityCaptureStoreError> {
        let Some(runtime) = &self.quality_runtime else {
            return Ok(());
        };
        let player_id = web_player_id(authenticated_subject);
        runtime.record_feedback(&player_id, reason_codes).await
    }

    pub fn with_review_share_store(mut self, store: Arc<dyn ReviewShareStore>) -> Self {
        self.review_shares = Arc::new(ReviewShareRuntime::new(store));
        self
    }

    /// Mints one Review Share Grant for the signed-in owner.
    /// Mints one Review Share Grant, over an address that exists.
    ///
    /// Ownership alone is not enough to mint on: a Game Import ID says whose
    /// review it is, and nothing in it says which Review Moments the review
    /// has. Minting over a moment the review does not contain would hand the
    /// Player a link that looks live and fails on the recipient's screen, which
    /// is the failure mode #258 exists to remove. The Critical Moments of the
    /// immutable snapshot are exactly what the shared page can render, so they
    /// are what a grant is checked against.
    pub async fn mint_review_share(
        &self,
        authenticated_subject: &str,
        address: ReviewShareAddress,
    ) -> Result<MintedReviewShare, ReviewShareError> {
        let owner = web_player_id(authenticated_subject);
        if !self.review_contains_moment(&owner, &address).await? {
            return Err(ReviewShareError::UnknownAddress);
        }
        self.review_shares.mint(&owner, address, Utc::now()).await
    }

    async fn review_contains_moment(
        &self,
        owner: &PlayerId,
        address: &ReviewShareAddress,
    ) -> Result<bool, ReviewShareError> {
        let mut events = self.submit_read(
            ProcessorPrincipal::Player(owner.clone()),
            ReviewSessionCommand::ReadGameReviewSnapshot {
                game_import_id: address.game_import_id.clone(),
                // Grant validation reads the review to check it, so a
                // revalidation answer would tell it nothing.
                known_content_digest: None,
            },
        );
        while let Some(envelope) = events.recv().await {
            if let ReviewSessionEvent::Completed { result } = envelope.event {
                let OperationCompletion::GameReviewSnapshotRead { review, .. } = *result else {
                    return Err(ReviewShareError::Unavailable);
                };
                return Ok(review
                    .critical_moments
                    .iter()
                    .any(|moment| moment.critical_moment_id == address.review_moment_id));
            }
        }
        Err(ReviewShareError::Unavailable)
    }

    /// The grants this Player still has outstanding.
    pub async fn outstanding_review_shares(
        &self,
        authenticated_subject: &str,
    ) -> Result<Vec<ReviewShareGrant>, ReviewShareError> {
        self.review_shares
            .outstanding(&web_player_id(authenticated_subject), Utc::now())
            .await
    }

    pub async fn revoke_review_share(
        &self,
        authenticated_subject: &str,
        share_id: &str,
    ) -> Result<(), ReviewShareError> {
        self.review_shares
            .revoke(&web_player_id(authenticated_subject), share_id)
            .await
    }

    /// Answers a share token with the grant behind it, or refuses it.
    ///
    /// Expiry and revocation are decided here, on the server, for every read a
    /// recipient makes and not once when the page loads.
    pub async fn resolve_review_share(
        &self,
        token: &str,
    ) -> Result<ReviewShareGrant, ReviewShareError> {
        self.review_shares.resolve(token, Utc::now()).await
    }

    /// Runs one read of a shared review as the Player who shared it.
    ///
    /// The recipient supplies no identity and no command — the grant supplies
    /// both — so a share can only ever read the review it names.
    pub fn read_shared_review(
        &self,
        grant: &ReviewShareGrant,
        resource: SharedReviewResource,
    ) -> mpsc::UnboundedReceiver<ReviewSessionEventEnvelope> {
        let command = match resource {
            SharedReviewResource::GameReview => ReviewSessionCommand::ReadGameReviewSnapshot {
                game_import_id: grant.address.game_import_id.clone(),
                // A share visitor has no Player subtree, so nothing caches
                // this read and there is never a digest to offer.
                known_content_digest: None,
            },
            SharedReviewResource::ReviewMoment => ReviewSessionCommand::ReadReviewMomentDetail {
                game_import_id: grant.address.game_import_id.clone(),
                review_moment_id: grant.address.review_moment_id.clone(),
                // A share visitor has no Player subtree and caches nothing.
                known_content_digest: None,
            },
        };
        self.submit_read(grant.principal(), command)
    }

    /// Runs one read command on behalf of a Player who is not the caller.
    ///
    /// Both share paths reach the Coach Engine this way — the mint-time address
    /// check and the recipient's own reads — so the command is built here and
    /// never named by whoever asked.
    fn submit_read(
        &self,
        principal: ProcessorPrincipal,
        command: ReviewSessionCommand,
    ) -> mpsc::UnboundedReceiver<ReviewSessionEventEnvelope> {
        let sequence = self.shared_reads.fetch_add(1, Ordering::Relaxed);
        let envelope = ReviewSessionCommandEnvelope {
            request_id: RequestId::try_from(format!("request:review-share:{sequence}"))
                .expect("a counted shared read identifier is valid"),
            operation_id: OperationId::try_from(format!("operation:review-share:{sequence}"))
                .expect("a counted shared read identifier is valid"),
            surface: DeliverySurface::Web,
            command,
        };
        let serialized = serde_json::to_vec(&envelope)
            .expect("a shared review read has an infallible representation");
        self.executor
            .clone()
            .submit_unmetered(principal, ProcessorCommandAdmission::parse(&serialized))
    }

    pub async fn retention_preference(
        &self,
        authenticated_subject: &str,
    ) -> Result<RetentionPreference, QualityCaptureStoreError> {
        self.quality_capture
            .preference(&web_player_id(authenticated_subject))
            .await
    }

    pub async fn set_retention_preference(
        &self,
        authenticated_subject: &str,
        enabled: bool,
    ) -> Result<RetentionPreference, QualityCaptureStoreError> {
        self.quality_capture
            .set_preference(&web_player_id(authenticated_subject), enabled)
            .await
    }

    pub fn submit(
        &self,
        authenticated_subject: &str,
        serialized_command: &[u8],
    ) -> mpsc::UnboundedReceiver<ReviewSessionEventEnvelope> {
        self.submit_with_trace(authenticated_subject, serialized_command, None)
    }

    pub fn submit_with_trace(
        &self,
        authenticated_subject: &str,
        serialized_command: &[u8],
        trace_id: Option<&str>,
    ) -> mpsc::UnboundedReceiver<ReviewSessionEventEnvelope> {
        let player_id = web_player_id(authenticated_subject);
        let admission = ProcessorCommandAdmission::parse(serialized_command);
        self.executor.clone().submit_with_trace(
            ProcessorPrincipal::Player(player_id),
            admission,
            trace_id
                .and_then(ReviewSessionTraceId::parse)
                .map(|trace_id| trace_id.as_str().to_owned()),
        )
    }
}

fn web_player_id(authenticated_subject: &str) -> PlayerId {
    PlayerId::try_from(authenticated_subject.to_string())
        .expect("Firebase verification guarantees a valid Player ID")
}
