use crate::review_session_contract::{
    CriticalMomentGroundingRejection, LearningResourceRole, ReviewMomentCommentFacts,
};

/// Learning prose is optional. When an author chooses to include a resource,
/// its role, title, and URL must be one exact materialized fact; no live or
/// invented link is admitted.
pub(super) fn validate(
    text: &str,
    facts: &ReviewMomentCommentFacts,
) -> Result<(), CriticalMomentGroundingRejection> {
    let resources = facts
        .moment()
        .learning_material
        .tracks
        .iter()
        .flat_map(|track| track.resources.iter())
        .collect::<Vec<_>>();
    if urls_in(text).iter().any(|url| {
        !resources
            .iter()
            .any(|resource| resource.canonical_url == *url)
    }) {
        return Err(CriticalMomentGroundingRejection::ChangedFact);
    }

    let mut exact_learn = 0usize;
    let mut exact_drill = 0usize;
    for resource in resources {
        let (role, count) = match resource.role {
            LearningResourceRole::Learn => ("Learn", &mut exact_learn),
            LearningResourceRole::Drill => ("Drill", &mut exact_drill),
        };
        let exact_literal = format!("{role}: {} ({})", resource.title, resource.canonical_url);
        if text.contains(&resource.canonical_url) && !text.contains(&exact_literal) {
            return Err(CriticalMomentGroundingRejection::ChangedFact);
        }
        *count += text.matches(&exact_literal).count();
    }
    if text.matches("Learn:").count() != exact_learn
        || text.matches("Drill:").count() != exact_drill
    {
        return Err(CriticalMomentGroundingRejection::ChangedFact);
    }
    Ok(())
}

fn urls_in(text: &str) -> Vec<&str> {
    text.split_whitespace()
        .filter_map(|token| {
            let start = token.find("https://").or_else(|| token.find("http://"))?;
            Some(token[start..].trim_end_matches(|character: char| {
                matches!(character, ')' | ']' | '}' | '>' | ',' | ';' | '.')
            }))
        })
        .collect()
}
