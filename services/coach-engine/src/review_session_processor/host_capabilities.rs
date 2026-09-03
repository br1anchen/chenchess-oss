use crate::review_session_host::{
    dispatch, HostCapabilityCall, HostCapabilityDispatch, HostCapabilityError, HostCapabilityStore,
    StoredHostMoment,
};

use super::session::{ProcessorReviewMomentEntry, ProcessorSession};

impl ProcessorSession {
    pub(crate) async fn host_capability_store(&self) -> HostCapabilityStore {
        let mut moments = Vec::new();
        for entry in self.review_moment_entries().await {
            if let Some(stored) = stored_host_moment(&entry).await {
                moments.push(stored);
            }
        }
        moments.sort_by_key(StoredHostMoment::ply);
        HostCapabilityStore::new(moments)
    }

    pub(crate) async fn dispatch_host_capability(
        &self,
        open_ply: u16,
        call: &HostCapabilityCall,
    ) -> Result<HostCapabilityDispatch, HostCapabilityError> {
        let store = self.host_capability_store().await;
        dispatch(&store, open_ply, call).await
    }
}

async fn stored_host_moment(entry: &ProcessorReviewMomentEntry) -> Option<StoredHostMoment> {
    let prepared = entry.prepared_moment().await?;
    let facts = prepared.comment_facts()?.clone();
    let material = prepared.host_learning_material();
    let packet = prepared.core_snapshot().await.evidence_packet;
    Some(
        StoredHostMoment::from_facts(facts, packet, material)
            .with_shared_exploration(prepared.exploration.clone()),
    )
}
