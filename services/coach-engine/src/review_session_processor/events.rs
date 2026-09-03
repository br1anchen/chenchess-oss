use std::{
    future::Future,
    sync::{
        atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};

use tokio::sync::mpsc;

use crate::request_trace::ReviewSessionTraceId;
use crate::review_session_contract::*;

pub(super) struct EventEmitter {
    request_id: RequestId,
    operation_id: OperationId,
    operation: OperationKind,
    sequence: AtomicU32,
    response_bytes: AtomicUsize,
    serialization_nanoseconds: AtomicU64,
    sender: mpsc::UnboundedSender<ReviewSessionEventEnvelope>,
    started_at: Instant,
    trace_id: Option<ReviewSessionTraceId>,
    validation_milliseconds: f64,
}

impl EventEmitter {
    pub(super) fn new(
        request_id: RequestId,
        operation_id: OperationId,
        operation: OperationKind,
        trace_id: Option<ReviewSessionTraceId>,
        validation_milliseconds: f64,
    ) -> (
        Arc<Self>,
        mpsc::UnboundedReceiver<ReviewSessionEventEnvelope>,
    ) {
        let (sender, receiver) = mpsc::unbounded_channel();
        (
            Arc::new(Self {
                request_id,
                operation_id,
                operation,
                sequence: AtomicU32::new(0),
                response_bytes: AtomicUsize::new(0),
                serialization_nanoseconds: AtomicU64::new(0),
                sender,
                started_at: Instant::now(),
                trace_id,
                validation_milliseconds,
            }),
            receiver,
        )
    }

    pub(super) fn accepted(&self, operation: OperationKind) {
        self.event(ReviewSessionEvent::Accepted {
            operation,
            limits: ReviewSessionLimits::V1,
        });
    }

    pub(super) fn completed(&self, result: OperationCompletion) {
        self.event(ReviewSessionEvent::Completed {
            result: Box::new(result),
        });
    }

    pub(super) fn unavailable(
        &self,
        operation: OperationKind,
        reason: ProviderUnavailableReason,
        retry: RetryDirective,
    ) {
        self.event(ReviewSessionEvent::Unavailable {
            operation,
            reason,
            retry,
        });
    }

    pub(super) fn review_moment_unavailable(
        &self,
        game_import_id: &GameImportId,
        review_moment_id: &CriticalMomentId,
        reason: ProviderUnavailableReason,
        retry: RetryDirective,
    ) {
        self.event(ReviewSessionEvent::ReviewMomentUnavailable {
            game_import_id: game_import_id.clone(),
            review_moment_id: review_moment_id.clone(),
            reason,
            retry,
        });
    }

    pub(super) fn cancelled(&self, operation: OperationKind) {
        self.event(ReviewSessionEvent::Cancelled { operation });
    }

    pub(super) fn conflict(&self, operation: OperationKind, reason: OperationConflictReason) {
        self.event(ReviewSessionEvent::Conflict { operation, reason });
    }

    pub(super) fn rejected(
        &self,
        operation: OperationKind,
        reason: CommandRejectionReason,
        recovery: RejectionRecovery,
    ) {
        self.event(ReviewSessionEvent::Rejected {
            operation,
            reason,
            recovery,
        });
    }

    pub(super) fn event(&self, event: ReviewSessionEvent) {
        let sequence = self.sequence.fetch_add(1, Ordering::Relaxed);
        let envelope = ReviewSessionEventEnvelope {
            request_id: self.request_id.clone(),
            operation_id: self.operation_id.clone(),
            sequence,
            event,
        };
        let serialization_started_at = Instant::now();
        // Measured on the delivery frame rather than the in-process envelope,
        // so the reported response size is the size a surface actually sees.
        let event_bytes =
            crate::review_session_contract::encode_delivery_frame(envelope.clone()).len();
        self.serialization_nanoseconds.fetch_add(
            u64::try_from(serialization_started_at.elapsed().as_nanos()).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
        let response_bytes = self
            .response_bytes
            .fetch_add(event_bytes, Ordering::Relaxed)
            + event_bytes;
        if is_terminal(&envelope.event) {
            self.emit_completion(&envelope.event, response_bytes);
        }
        let _ = self.sender.send(envelope);
    }

    pub(super) fn operation_id(&self) -> &OperationId {
        &self.operation_id
    }

    pub(super) fn request_id(&self) -> &RequestId {
        &self.request_id
    }

    fn emit_completion(&self, event: &ReviewSessionEvent, response_bytes: usize) {
        let (status, failure_kind) = terminal_status(event);
        let diagnostic = serde_json::json!({
            "boundary": "coach-engine",
            "event": "review_session_command_completion",
            "failureKind": failure_kind,
            "operation": self.operation,
            "operationId": self.operation_id,
            "requestId": self.request_id,
            "responseBytes": response_bytes,
            "schemaVersion": 1,
            "serializationMilliseconds":
                self.serialization_nanoseconds.load(Ordering::Relaxed) as f64 / 1_000_000.0,
            "status": status,
            "totalMilliseconds": round_milliseconds(self.started_at.elapsed()),
            "traceId": self.trace_id.as_ref().map(ReviewSessionTraceId::as_str),
            "validationMilliseconds": self.validation_milliseconds,
        });
        eprintln!(
            "{}",
            serde_json::to_string(&diagnostic)
                .expect("Review Session completion telemetry is serializable")
        );
    }
}

fn is_terminal(event: &ReviewSessionEvent) -> bool {
    !matches!(
        event,
        ReviewSessionEvent::Accepted { .. } | ReviewSessionEvent::Progress { .. }
    )
}

fn terminal_status(event: &ReviewSessionEvent) -> (&'static str, Option<&'static str>) {
    match event {
        ReviewSessionEvent::Completed { .. } => ("succeeded", None),
        ReviewSessionEvent::Cancelled { .. } => ("cancelled", Some("cancelled")),
        ReviewSessionEvent::Unavailable { .. }
        | ReviewSessionEvent::ReviewMomentUnavailable { .. } => {
            ("failed", Some("provider_unavailable"))
        }
        ReviewSessionEvent::Conflict { .. } => ("failed", Some("conflict")),
        ReviewSessionEvent::Rejected { .. } => ("failed", Some("rejected")),
        ReviewSessionEvent::Accepted { .. } | ReviewSessionEvent::Progress { .. } => {
            ("incomplete", Some("non_terminal"))
        }
    }
}

fn round_milliseconds(duration: std::time::Duration) -> f64 {
    (duration.as_secs_f64() * 100_000.0).round() / 100.0
}

pub(super) struct OperationProgressEmitter {
    emitter: Arc<EventEmitter>,
    current: Mutex<OperationProgress>,
}

impl OperationProgressEmitter {
    pub(super) fn new(emitter: Arc<EventEmitter>, initial: OperationProgress) -> Arc<Self> {
        let progress = Arc::new(Self {
            emitter,
            current: Mutex::new(initial),
        });
        progress.emit_current();
        progress
    }

    pub(super) fn set(&self, stage: OperationProgress) {
        *self.current.lock().expect("progress mutex is not poisoned") = stage;
        self.emit_current();
    }

    pub(super) async fn run<T>(&self, future: impl Future<Output = T>) -> T {
        tokio::pin!(future);
        let start = tokio::time::Instant::now() + std::time::Duration::from_secs(1);
        let mut heartbeat = tokio::time::interval_at(start, std::time::Duration::from_secs(1));
        loop {
            tokio::select! {
                result = &mut future => return result,
                _ = heartbeat.tick() => self.emit_current(),
            }
        }
    }

    fn emit_current(&self) {
        let stage = self
            .current
            .lock()
            .expect("progress mutex is not poisoned")
            .clone();
        self.emitter.event(ReviewSessionEvent::Progress { stage });
    }
}
