use crate::review_session_contract::{
    CurriculumLearningConcept, LearningResource, LearningResourceId, LearningResourceKind,
    LearningResourceRole,
};

use super::ResourceCatalogError;

mod registry;
mod resources;

use registry::entry_for;

pub(super) fn resources_for(
    concept: CurriculumLearningConcept,
) -> Result<Vec<LearningResource>, ResourceCatalogError> {
    let entry = entry_for(concept).ok_or(ResourceCatalogError::MissingResourceMapping)?;
    let mut materialized =
        Vec::with_capacity(entry.practice.len() + usize::from(entry.puzzle_theme.is_some()));
    for practice in entry.practice {
        materialized.push(LearningResource {
            resource_id: semantic_id(format!("lichess:practice:{}", practice.id))?,
            role: LearningResourceRole::Learn,
            kind: LearningResourceKind::PracticeModule,
            title: practice.title.to_string(),
            canonical_url: format!("https://lichess.org/practice/{}", practice.path),
        });
    }
    if let Some(theme) = entry.puzzle_theme {
        materialized.push(LearningResource {
            resource_id: semantic_id(format!("lichess:puzzles:{theme}"))?,
            role: LearningResourceRole::Drill,
            kind: LearningResourceKind::PuzzleStream,
            title: entry.title.to_string(),
            canonical_url: format!("https://lichess.org/training/{theme}"),
        });
    }
    if materialized.is_empty() {
        return Err(ResourceCatalogError::MissingResourceMapping);
    }
    Ok(materialized)
}

fn semantic_id(value: String) -> Result<LearningResourceId, ResourceCatalogError> {
    LearningResourceId::try_from(value).map_err(|_| ResourceCatalogError::MissingResourceMapping)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::registry::{
        CURRICULUM, NON_CONCEPT_PUZZLE_THEMES, PINNED_PRACTICE_IDS, PINNED_VISIBLE_PUZZLE_THEMES,
    };
    use super::*;

    #[test]
    fn every_catalogued_concept_has_one_exact_nonempty_resource_mapping() {
        let mut concepts = BTreeSet::new();
        for entry in CURRICULUM {
            assert!(
                concepts.insert(entry.concept),
                "duplicate {:?}",
                entry.concept
            );
            assert!(!entry.title.trim().is_empty());
            assert!(!resources_for(entry.concept).unwrap().is_empty());
        }
        assert_eq!(concepts.len(), CURRICULUM.len());

        let represented_themes = CURRICULUM
            .iter()
            .filter_map(|entry| entry.puzzle_theme)
            .collect::<BTreeSet<_>>();
        let metadata = NON_CONCEPT_PUZZLE_THEMES
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        assert_eq!(
            represented_themes
                .union(&metadata)
                .copied()
                .collect::<BTreeSet<_>>(),
            PINNED_VISIBLE_PUZZLE_THEMES
                .iter()
                .copied()
                .collect::<BTreeSet<_>>()
        );
        assert!(represented_themes.is_disjoint(&metadata));

        let practice_ids = CURRICULUM
            .iter()
            .flat_map(|entry| entry.practice.iter().map(|resource| resource.id))
            .collect::<BTreeSet<_>>();
        assert_eq!(
            practice_ids,
            PINNED_PRACTICE_IDS.iter().copied().collect::<BTreeSet<_>>()
        );
    }
}
