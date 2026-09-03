use std::sync::Arc;

use chen_chess_coach_engine::{
    review_session_contract::*,
    review_session_processor::{ProcessorPrincipal, ReviewSessionProcessor},
};
use tokio::sync::mpsc;

pub use super::processor_support::{grounded_coach_assessment, processor, CapturedLichess};

pub const WEB_SUBJECT: &str = "journey-player";

pub struct TransportHarness {
    processor: Arc<ReviewSessionProcessor<CapturedLichess>>,
}

impl TransportHarness {
    pub fn local(processor: Arc<ReviewSessionProcessor<CapturedLichess>>) -> Self {
        Self { processor }
    }

    pub async fn submit(
        &mut self,
        label: &str,
        command: ReviewSessionCommand,
    ) -> Vec<ReviewSessionEventEnvelope> {
        let envelope = envelope(DeliverySurface::CoachSkill, label, command);
        collect_receiver(self.processor.submit(
            ProcessorPrincipal::LocalCoach,
            &serde_json::to_vec(&envelope).unwrap(),
        ))
        .await
    }
}

pub fn envelope(
    surface: DeliverySurface,
    label: &str,
    command: ReviewSessionCommand,
) -> ReviewSessionCommandEnvelope {
    ReviewSessionCommandEnvelope {
        request_id: RequestId::try_from(format!("request:journey:{label}")).unwrap(),
        operation_id: OperationId::try_from(format!("operation:journey:{label}")).unwrap(),
        surface,
        command,
    }
}

pub async fn collect_receiver(
    mut receiver: mpsc::UnboundedReceiver<ReviewSessionEventEnvelope>,
) -> Vec<ReviewSessionEventEnvelope> {
    let mut events = Vec::new();
    while let Some(event) = receiver.recv().await {
        events.push(event);
    }
    events
}
