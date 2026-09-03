use std::sync::Arc;

use tokio::sync::mpsc;

use crate::{
    lichess::LichessExportClient,
    review_session_contract::ReviewSessionEventEnvelope,
    review_session_processor::{
        ProcessorCommandAdmission, ProcessorPrincipal, ReviewSessionProcessor,
    },
};

mod jsonl;
mod web;

pub use jsonl::{
    run_review_session_jsonl, run_review_session_jsonl_ingress, start_review_session_jsonl_ingress,
    ReviewSessionJsonlIngress,
};
pub use web::{ReviewSessionWebBinding, SharedReviewResource};

pub trait ReviewSessionCommandExecutor: Send + Sync {
    fn submit(
        self: Arc<Self>,
        principal: ProcessorPrincipal,
        admission: ProcessorCommandAdmission,
    ) -> mpsc::UnboundedReceiver<ReviewSessionEventEnvelope>;

    fn submit_with_trace(
        self: Arc<Self>,
        principal: ProcessorPrincipal,
        admission: ProcessorCommandAdmission,
        trace_id: Option<String>,
    ) -> mpsc::UnboundedReceiver<ReviewSessionEventEnvelope> {
        let _ = trace_id;
        self.submit(principal, admission)
    }

    fn submit_unmetered(
        self: Arc<Self>,
        principal: ProcessorPrincipal,
        admission: ProcessorCommandAdmission,
    ) -> mpsc::UnboundedReceiver<ReviewSessionEventEnvelope> {
        self.submit(principal, admission)
    }
}

impl<C> ReviewSessionCommandExecutor for ReviewSessionProcessor<C>
where
    C: LichessExportClient + 'static,
{
    fn submit(
        self: Arc<Self>,
        principal: ProcessorPrincipal,
        admission: ProcessorCommandAdmission,
    ) -> mpsc::UnboundedReceiver<ReviewSessionEventEnvelope> {
        ReviewSessionProcessor::submit_admitted(&self, principal, admission)
    }

    fn submit_with_trace(
        self: Arc<Self>,
        principal: ProcessorPrincipal,
        admission: ProcessorCommandAdmission,
        trace_id: Option<String>,
    ) -> mpsc::UnboundedReceiver<ReviewSessionEventEnvelope> {
        ReviewSessionProcessor::submit_admitted_with_trace(
            &self,
            principal,
            admission,
            trace_id.and_then(|value| crate::request_trace::ReviewSessionTraceId::parse(&value)),
        )
    }

    fn submit_unmetered(
        self: Arc<Self>,
        principal: ProcessorPrincipal,
        admission: ProcessorCommandAdmission,
    ) -> mpsc::UnboundedReceiver<ReviewSessionEventEnvelope> {
        ReviewSessionProcessor::submit_admitted_unmetered(&self, principal, admission)
    }
}
