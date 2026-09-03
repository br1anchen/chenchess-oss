use crate::{
    decision_explanation::{learning_concept_relationships, LearningConceptRelationships},
    review_session_contract::{
        CurriculumLearningConcept, LearningTrack, LearningTrackKey, LearningTrackSupport,
    },
};

pub(crate) fn order_learning_tracks(
    tracks: Vec<LearningTrack>,
) -> Result<Vec<LearningTrack>, &'static str> {
    let relationships = learning_concept_relationships()?;
    let effective_support = tracks
        .iter()
        .map(|track| {
            let descendant_support = track_concept(track)
                .map(|ancestor| {
                    tracks
                        .iter()
                        .filter_map(|candidate| {
                            let descendant = track_concept(candidate)?;
                            relationships
                                .refines(descendant, ancestor)
                                .then_some(candidate.support.len())
                        })
                        .sum::<usize>()
                })
                .unwrap_or_default();
            track.support.len() + descendant_support
        })
        .collect::<Vec<_>>();
    let mut ranked = tracks
        .into_iter()
        .zip(effective_support)
        .collect::<Vec<_>>();
    ranked.sort_by(|(left, left_support), (right, right_support)| {
        right_support
            .cmp(left_support)
            .then_with(|| track_has_improvement(right).cmp(&track_has_improvement(left)))
            .then_with(|| left.key.cmp(&right.key))
    });
    let tracks = ranked.into_iter().map(|(track, _)| track).collect();

    let clustered = cluster_refinements(tracks, &relationships);
    Ok(stable_prerequisite_order(clustered, &relationships))
}

fn cluster_refinements(
    tracks: Vec<LearningTrack>,
    relationships: &LearningConceptRelationships,
) -> Vec<LearningTrack> {
    let concepts = tracks.iter().map(track_concept).collect::<Vec<_>>();
    let mut parents = vec![None; tracks.len()];
    for (child_index, child) in concepts
        .iter()
        .copied()
        .enumerate()
        .filter_map(|(index, concept)| concept.map(|concept| (index, concept)))
    {
        let ancestors = concepts
            .iter()
            .copied()
            .enumerate()
            .filter_map(|(index, candidate)| {
                let candidate = candidate?;
                relationships
                    .refines(child, candidate)
                    .then_some((index, candidate))
            })
            .collect::<Vec<_>>();
        parents[child_index] = ancestors
            .iter()
            .filter(|(candidate_index, candidate)| {
                !ancestors.iter().any(|(other_index, other)| {
                    other_index != candidate_index && relationships.refines(*other, *candidate)
                })
            })
            .map(|(index, _)| *index)
            .min();
    }

    let mut children = vec![Vec::new(); tracks.len()];
    for (child, parent) in parents.iter().copied().enumerate() {
        if let Some(parent) = parent {
            children[parent].push(child);
        }
    }
    let mut order = Vec::with_capacity(tracks.len());
    for root in (0..tracks.len()).filter(|index| parents[*index].is_none()) {
        append_refinement_cluster(root, &children, &mut order);
    }
    reorder_tracks(tracks, order)
}

fn append_refinement_cluster(index: usize, children: &[Vec<usize>], order: &mut Vec<usize>) {
    order.push(index);
    for child in &children[index] {
        append_refinement_cluster(*child, children, order);
    }
}

fn stable_prerequisite_order(
    tracks: Vec<LearningTrack>,
    relationships: &LearningConceptRelationships,
) -> Vec<LearningTrack> {
    let concepts = tracks.iter().map(track_concept).collect::<Vec<_>>();
    let mut emitted = vec![false; tracks.len()];
    let mut order = Vec::with_capacity(tracks.len());
    // Pull each foundation immediately before the first ranked recommendation
    // that needs it, retaining that recommendation's position among unrelated
    // tracks whenever the hard constraint permits it.
    for index in 0..tracks.len() {
        append_with_prerequisites(index, &concepts, relationships, &mut emitted, &mut order);
    }
    reorder_tracks(tracks, order)
}

fn append_with_prerequisites(
    index: usize,
    concepts: &[Option<CurriculumLearningConcept>],
    relationships: &LearningConceptRelationships,
    emitted: &mut [bool],
    order: &mut Vec<usize>,
) {
    if emitted[index] {
        return;
    }
    if let Some(dependent) = concepts[index] {
        for (prerequisite_index, prerequisite) in concepts
            .iter()
            .copied()
            .enumerate()
            .filter_map(|(index, concept)| concept.map(|concept| (index, concept)))
        {
            if relationships.has_prerequisite(dependent, prerequisite) {
                append_with_prerequisites(
                    prerequisite_index,
                    concepts,
                    relationships,
                    emitted,
                    order,
                );
            }
        }
    }
    emitted[index] = true;
    order.push(index);
}

fn reorder_tracks(tracks: Vec<LearningTrack>, order: Vec<usize>) -> Vec<LearningTrack> {
    let mut positions = vec![0; tracks.len()];
    for (position, index) in order.into_iter().enumerate() {
        positions[index] = position;
    }
    let mut tracks = tracks.into_iter().enumerate().collect::<Vec<_>>();
    tracks.sort_by_key(|(index, _)| positions[*index]);
    tracks.into_iter().map(|(_, track)| track).collect()
}

fn track_concept(track: &LearningTrack) -> Option<CurriculumLearningConcept> {
    match &track.key {
        LearningTrackKey::Curriculum { concept } => Some(*concept),
        LearningTrackKey::Opening { .. } => None,
    }
}

fn track_has_improvement(track: &LearningTrack) -> bool {
    track
        .support
        .iter()
        .any(|support| matches!(support, LearningTrackSupport::Improvement { .. }))
}
