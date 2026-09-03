//! Which recorded Review Moment stands for each Fact Shape.
//!
//! A [`FactShape`] names an authoring problem; an **Exemplar** is the one
//! recorded Review Moment a measurement uses for it. Addressing exemplars by
//! shape rather than by `(case, ply)` is the whole point: a rule change moves
//! plies, and the resolution is recomputed against the corpus instead of being
//! hand-repointed.
//!
//! [`resolve`] asks which moment stands for each shape the corpus holds. It is
//! incumbent-stable, so a recorded resolution churns only where the rules
//! moved. [`verify_resolution`] holds a recorded resolution to the corpus it
//! was computed over.
//!
//! Nothing here pins a shape count. The count follows the corpus.

use std::{collections::BTreeMap, fs, path::Path};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    critical_moment_comment::{FactShape, FactShapeId},
    review_session_contract::ReviewMomentCommentFacts,
};

use super::{case_path, corpus_case_ids, read_typed_json, recorded_comment_case, EvaluationError};

/// The recorded answer to "which moment stands for each Fact Shape".
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExemplarResolution {
    /// Moves only when the resolution itself moves, so a no-op re-resolution
    /// leaves the file byte-identical and reviewable as "nothing changed".
    pub resolved_at: String,
    pub corpus_digest: String,
    pub exemplars: BTreeMap<FactShapeId, Exemplar>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Exemplar {
    pub case_id: String,
    pub ply: u16,
    /// Digests the `ReviewMomentCommentFacts` this exemplar resolved to. A
    /// re-recorded corpus that moves this invalidates replay instead of
    /// silently changing the subject of a measurement.
    pub facts_digest: String,
}

/// Re-resolves every Fact Shape the corpus holds to the moment that stands for
/// it.
///
/// Incumbent-stable: a prior Exemplar that still exhibits its shape is kept, so
/// re-resolving after an unrelated rule change rewrites only the entries the
/// change actually moved. Otherwise the lowest `(case id, ply)` wins, which is
/// an arbitrary rule chosen for being stable rather than for being good — the
/// measurement is over shapes, and any moment of a shape presents the same
/// authoring problem.
pub fn resolve(
    corpus: &Path,
    prior: Option<&ExemplarResolution>,
) -> Result<ExemplarResolution, FactShapeResolutionError> {
    let moments = corpus_moments(corpus)?;
    let mut exemplars: BTreeMap<FactShapeId, Exemplar> = BTreeMap::new();
    for moment in &moments {
        let id = moment.shape.id();
        let incumbent = prior
            .and_then(|prior| prior.exemplars.get(&id))
            .is_some_and(|exemplar| {
                exemplar.case_id == moment.case_id && exemplar.ply == moment.ply
            });
        if incumbent {
            exemplars.insert(id, moment.as_exemplar());
        } else {
            exemplars.entry(id).or_insert_with(|| moment.as_exemplar());
        }
    }

    let corpus_digest = corpus_digest(corpus)?;
    let resolved_at = prior
        .filter(|prior| prior.corpus_digest == corpus_digest && prior.exemplars == exemplars)
        .map_or_else(now_utc, |prior| prior.resolved_at.clone());

    Ok(ExemplarResolution {
        resolved_at,
        corpus_digest,
        exemplars,
    })
}

/// Checks that every recorded Exemplar still names a moment the corpus records,
/// still exhibits the shape it was recorded for, and still digests to what was
/// recorded.
///
/// This is what a replay refuses on: a moved digest means the prompt a run
/// would issue is no longer the prompt the prior run issued, so comparing them
/// would compare two different subjects.
pub fn verify_resolution(
    corpus: &Path,
    resolution: &ExemplarResolution,
) -> Result<(), FactShapeResolutionError> {
    let moments = corpus_moments(corpus)?;
    let mut stale = Vec::new();
    for (shape, exemplar) in &resolution.exemplars {
        let recorded = moments
            .iter()
            .find(|moment| moment.case_id == exemplar.case_id && moment.ply == exemplar.ply);
        let reason = match recorded {
            None => Some(StaleReason::MomentMissing),
            Some(moment) if moment.shape.id() != *shape => Some(StaleReason::ShapeMoved {
                now: moment.shape.id(),
            }),
            Some(moment) if moment.facts_digest != exemplar.facts_digest => {
                Some(StaleReason::FactsDigestMoved {
                    now: moment.facts_digest.clone(),
                })
            }
            Some(_) => None,
        };
        if let Some(reason) = reason {
            stale.push(StaleExemplar {
                shape: shape.clone(),
                case_id: exemplar.case_id.clone(),
                ply: exemplar.ply,
                reason,
            });
        }
    }
    if stale.is_empty() {
        Ok(())
    } else {
        Err(FactShapeResolutionError::StaleResolution(stale))
    }
}

pub fn read_resolution(path: &Path) -> Result<ExemplarResolution, FactShapeResolutionError> {
    Ok(read_typed_json(path)?)
}

pub fn write_resolution(
    path: &Path,
    resolution: &ExemplarResolution,
) -> Result<(), FactShapeResolutionError> {
    let mut bytes = serde_json::to_vec_pretty(resolution).map_err(EvaluationError::Serialize)?;
    bytes.push(b'\n');
    fs::write(path, bytes).map_err(|source| EvaluationError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(())
}

/// One recorded moment, with everything the resolution reads.
struct CorpusMoment {
    case_id: String,
    ply: u16,
    shape: FactShape,
    facts_digest: String,
}

impl CorpusMoment {
    fn as_exemplar(&self) -> Exemplar {
        Exemplar {
            case_id: self.case_id.clone(),
            ply: self.ply,
            facts_digest: self.facts_digest.clone(),
        }
    }
}

/// Every authorable moment the corpus records, in `(case id, ply)` order —
/// which is also the tie-break order [`resolve`] depends on.
fn corpus_moments(corpus: &Path) -> Result<Vec<CorpusMoment>, FactShapeResolutionError> {
    let mut moments = Vec::new();
    for case_id in corpus_case_ids(corpus)? {
        let case = recorded_comment_case(corpus, &case_id)?;
        for moment in case.moments {
            moments.push(CorpusMoment {
                case_id: case_id.clone(),
                ply: moment.ply,
                shape: FactShape::of(&moment.facts),
                facts_digest: facts_digest(&moment.facts),
            });
        }
    }
    moments.sort_by(|left, right| (&left.case_id, left.ply).cmp(&(&right.case_id, right.ply)));
    Ok(moments)
}

fn facts_digest(facts: &ReviewMomentCommentFacts) -> String {
    let bytes = serde_json_canonicalizer::to_vec(facts)
        .expect("Review Moment comment facts should have an RFC 8785 canonical form");
    format!("sha256:{:x}", Sha256::digest(bytes))
}

/// Digests the case files a resolution was computed over, so a resolution
/// carries what it answers about.
fn corpus_digest(corpus: &Path) -> Result<String, FactShapeResolutionError> {
    let mut hasher = Sha256::new();
    for case_id in corpus_case_ids(corpus)? {
        let path = case_path(corpus, &case_id);
        let bytes = fs::read(&path).map_err(|source| EvaluationError::Io {
            path: path.clone(),
            source,
        })?;
        hasher.update(case_id.as_bytes());
        hasher.update(Sha256::digest(bytes));
    }
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

fn now_utc() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaleExemplar {
    pub shape: FactShapeId,
    pub case_id: String,
    pub ply: u16,
    pub reason: StaleReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StaleReason {
    MomentMissing,
    ShapeMoved { now: FactShapeId },
    FactsDigestMoved { now: String },
}

#[derive(Debug, thiserror::Error)]
pub enum FactShapeResolutionError {
    #[error("{}", stale_resolution_message(.0))]
    StaleResolution(Vec<StaleExemplar>),
    #[error(transparent)]
    Evaluation(#[from] EvaluationError),
}

fn stale_resolution_message(stale: &[StaleExemplar]) -> String {
    let entries = stale
        .iter()
        .map(|entry| {
            let reason = match &entry.reason {
                StaleReason::MomentMissing => "the corpus no longer records it".to_string(),
                StaleReason::ShapeMoved { now } => format!("it now exhibits {now}"),
                StaleReason::FactsDigestMoved { now } => format!("its facts now digest to {now}"),
            };
            format!(
                "  {} -> {}:{}: {reason}",
                entry.shape, entry.case_id, entry.ply
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "{} recorded Exemplar(s) no longer resolve; re-resolve the corpus:\n{entries}",
        stale.len()
    )
}
