use crate::{
    engine_analysis::EngineProvenance,
    evaluation_recording::{
        PINNED_MAIA_CANDIDATE_LIMIT, PINNED_MAIA_CONFIG_DIGEST, PINNED_MAIA_IMAGE,
        PINNED_MAIA_MODEL, PINNED_MAIA_MODEL_DIGEST, PINNED_MAIA_PACKAGE,
    },
    human_move_model::HumanMoveCacheIdentity,
    review_session_contract::{ArtifactDigest, EloRating, EvidenceProvenance},
};

pub fn stockfish(provenance: EngineProvenance) -> Option<EvidenceProvenance> {
    Some(EvidenceProvenance::Stockfish {
        version: provenance.version,
        binary_digest: ArtifactDigest::try_from(format!("sha256:{}", provenance.binary_sha256))
            .ok()?,
        depth: provenance.depth,
        threads: provenance.threads,
        hash_mib: provenance.hash_mib,
    })
}

pub fn identified_maia(
    identity: &HumanMoveCacheIdentity,
    elo: EloRating,
) -> Option<EvidenceProvenance> {
    identity.is_pinned_maia().then(|| pinned_maia(elo))
}

pub fn pinned_maia(elo: EloRating) -> EvidenceProvenance {
    EvidenceProvenance::Maia {
        package: PINNED_MAIA_PACKAGE.to_string(),
        model: PINNED_MAIA_MODEL.to_string(),
        image: PINNED_MAIA_IMAGE.to_string(),
        model_digest: ArtifactDigest::try_from(PINNED_MAIA_MODEL_DIGEST.to_string())
            .expect("pinned Maia model digest is valid"),
        config_digest: ArtifactDigest::try_from(PINNED_MAIA_CONFIG_DIGEST.to_string())
            .expect("pinned Maia config digest is valid"),
        player_elo: elo,
        opponent_elo: elo,
        candidate_limit: PINNED_MAIA_CANDIDATE_LIMIT,
    }
}
