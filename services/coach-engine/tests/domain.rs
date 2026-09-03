#[allow(dead_code)]
#[path = "review_session_processor/support.rs"]
mod processor_support;
#[allow(dead_code, unused_imports)]
#[path = "review_session_transports/support.rs"]
mod transport_support;

#[path = "canonical_game_fixture.rs"]
mod canonical_game_fixture;
#[path = "chronological_review_conformance.rs"]
mod chronological_review_conformance;
#[path = "critical_moment_comment.rs"]
mod critical_moment_comment;
#[path = "fact_shape_resolution.rs"]
mod fact_shape_resolution;
#[allow(dead_code)]
#[path = "marker_commentary.rs"]
mod marker_commentary;
#[path = "pipeline_evaluation.rs"]
mod pipeline_evaluation;
#[path = "review_moment_comment_publication.rs"]
mod review_moment_comment_publication;
#[path = "review_validation.rs"]
mod review_validation;
