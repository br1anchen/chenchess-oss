use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Digest;

use crate::{
    engine_analysis::{EngineAnalysisError, EngineAnalysisInput, EngineAnalyzer, EngineProvenance},
    operating_limits::{EVALUATION_CENTIPAWN_TOLERANCE, PROBABILITY_TOLERANCE},
    pgn::{parse_pgn, PgnImportError},
    review_facts::{
        decision_explanation, game_review, ProviderEvidence, ReviewFactsError, ReviewFactsService,
    },
    review_session_contract::{ContractValueError, GameRef, ReviewMomentCommentFacts},
    rule_extractor::{self, RuleExtraction, RuleExtractorError},
    types::{EloProfile, Game, ReviewSide},
};

use multi_pv_recording::{MultiPvRecordingError, RecordedDecisionProof, RecordedMultiPv};

mod coach_turn;
mod fact_shape_resolution;
mod learning;
mod multi_pv_recording;

pub use coach_turn::{recorded_coach_turn_case, RecordedCoachTurnCase};
pub use fact_shape_resolution::{
    read_resolution, resolve, verify_resolution, write_resolution, Exemplar, ExemplarResolution,
    FactShapeResolutionError, StaleExemplar, StaleReason,
};
pub use learning::{
    evaluate_frozen_game_review, evaluate_frozen_learning_review, migration_disposition_for_idea,
    ConceptMigrationDisposition, LearningMatchDisposition, LearningMatchEvaluation,
    LearningMatchEvaluationError, LearningMatchPurpose,
};

const ACCEPTED_DIFF_FILE: &str = "last-accepted.diff";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationReport {
    pub evaluated_cases: usize,
    pub differences: Vec<CaseDifference>,
}

impl EvaluationReport {
    pub fn require_clean(&self) -> Result<(), EvaluationDrift> {
        if self.differences.is_empty() {
            Ok(())
        } else {
            Err(EvaluationDrift {
                differing_cases: self.differences.len(),
            })
        }
    }
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("{differing_cases} case(s) differ from their baselines")]
pub struct EvaluationDrift {
    pub differing_cases: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseDifference {
    pub case_id: String,
    pub field_diff: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptanceReport {
    pub changed_cases: Vec<String>,
    pub diff_path: PathBuf,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingReport {
    pub changed_cases: Vec<String>,
    pub recorded_comparisons: usize,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CorpusCase {
    id: String,
    description: String,
    input: CaseInput,
    provenance: LiveProviderProvenance,
    /// The authoritative single-PV search's own provenance, which the Decision
    /// Explanation stamps onto the candidate it publishes. Absent until the case has
    /// been through MultiPV recording, and absent means no candidate comparison is
    /// replayed at all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    engine_provenance: Option<EngineProvenance>,
    evidence: Vec<ProviderEvidence>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    multi_pv_evidence: Vec<RecordedMultiPv>,
}

#[derive(Debug, Deserialize, Serialize)]
struct CaseInput {
    pgn: String,
    elo: u16,
    operation: EvaluationOperation,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum EvaluationOperation {
    Game { review_side: ReviewSide },
    SelectedMoment { selected_ply: usize },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveProviderProvenance {
    pub stockfish: String,
    pub maia_image: String,
    pub maia_package: String,
    pub maia_model: String,
}

#[derive(Clone, Copy)]
struct LiveTolerances {
    centipawns: u32,
    probability: f64,
}

#[derive(Clone, Copy)]
enum DiffMode {
    Exact,
    Live(LiveTolerances),
}

impl Default for LiveTolerances {
    fn default() -> Self {
        Self {
            centipawns: EVALUATION_CENTIPAWN_TOLERANCE,
            probability: PROBABILITY_TOLERANCE,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FactBaseline {
    case_id: String,
    provenance: LiveProviderProvenance,
    facts: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    selector_trace: Option<rule_extractor::SelectorTrace>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    decision_explanations: Vec<RecordedDecisionProof>,
}

struct EvaluatedFacts {
    facts: Value,
    selector_trace: Option<rule_extractor::SelectorTrace>,
    decision_explanations: Vec<RecordedDecisionProof>,
}

pub fn run_fast_evaluation(corpus_dir: &Path) -> Result<EvaluationReport, EvaluationError> {
    let actual = evaluate_cases(corpus_dir)?;
    reject_orphan_baselines(corpus_dir, actual.keys())?;
    let mut differences = Vec::new();
    for (case_id, actual_value) in &actual {
        let baseline_path = baseline_path(corpus_dir, case_id);
        let expected = read_json(&baseline_path)?;
        let field_diff = diff_json(&expected, actual_value);
        if !field_diff.is_empty() {
            differences.push(CaseDifference {
                case_id: case_id.clone(),
                field_diff,
            });
        }
    }
    Ok(EvaluationReport {
        evaluated_cases: actual.len(),
        differences,
    })
}

pub fn accept_fast_evaluation(corpus_dir: &Path) -> Result<AcceptanceReport, EvaluationError> {
    let actual = evaluate_cases(corpus_dir)?;
    reject_orphan_baselines(corpus_dir, actual.keys())?;
    let mut changed_cases = Vec::new();
    let mut diff_report = String::new();
    for (case_id, actual_value) in &actual {
        let path = baseline_path(corpus_dir, case_id);
        let expected = if path.exists() {
            read_json(&path)?
        } else {
            Value::Null
        };
        let field_diff = diff_json(&expected, actual_value);
        if field_diff.is_empty() {
            continue;
        }
        changed_cases.push(case_id.clone());
        diff_report.push_str(&format!("## {case_id}\n{field_diff}\n"));
        write_canonical_json(&path, actual_value)?;
    }
    let diff_path = corpus_dir.join(ACCEPTED_DIFF_FILE);
    fs::write(&diff_path, diff_report).map_err(|source| EvaluationError::Io {
        path: diff_path.clone(),
        source,
    })?;
    Ok(AcceptanceReport {
        changed_cases,
        diff_path,
    })
}

pub async fn run_live_evaluation(
    corpus_dir: &Path,
    service: &ReviewFactsService,
    provenance: LiveProviderProvenance,
) -> Result<EvaluationReport, EvaluationError> {
    run_live_evaluation_with_progress(corpus_dir, service, provenance, |_, _, _| {}).await
}

pub async fn run_live_evaluation_with_progress(
    corpus_dir: &Path,
    service: &ReviewFactsService,
    provenance: LiveProviderProvenance,
    mut progress: impl FnMut(usize, usize, &str),
) -> Result<EvaluationReport, EvaluationError> {
    let tolerances = LiveTolerances::default();
    let actual = evaluate_live_cases(corpus_dir, service, provenance, &mut progress).await?;
    reject_orphan_baselines(corpus_dir, actual.keys())?;
    let mut differences = Vec::new();
    for (case_id, live) in &actual {
        let expected_case = read_json(&case_path(corpus_dir, case_id))?;
        let expected_facts = read_json(&baseline_path(corpus_dir, case_id))?;
        let case_diff = diff_json_with_tolerances(&expected_case, &live.case, tolerances);
        let facts_diff = diff_json_with_tolerances(&expected_facts, &live.facts, tolerances);
        let field_diff = labeled_diff(&case_diff, &facts_diff);
        if !field_diff.is_empty() {
            differences.push(CaseDifference {
                case_id: case_id.clone(),
                field_diff,
            });
        }
    }
    Ok(EvaluationReport {
        evaluated_cases: actual.len(),
        differences,
    })
}

pub async fn accept_live_evaluation(
    corpus_dir: &Path,
    service: &ReviewFactsService,
    provenance: LiveProviderProvenance,
) -> Result<AcceptanceReport, EvaluationError> {
    let actual = evaluate_live_cases(corpus_dir, service, provenance, &mut |_, _, _| {}).await?;
    reject_orphan_baselines(corpus_dir, actual.keys())?;
    let mut changed_cases = Vec::new();
    let mut diff_report = String::new();
    for (case_id, live) in &actual {
        let case_path = case_path(corpus_dir, case_id);
        let baseline_path = baseline_path(corpus_dir, case_id);
        let case_diff = diff_json(&read_json(&case_path)?, &live.case);
        let expected_facts = if baseline_path.exists() {
            read_json(&baseline_path)?
        } else {
            Value::Null
        };
        let facts_diff = diff_json(&expected_facts, &live.facts);
        let field_diff = labeled_diff(&case_diff, &facts_diff);
        if field_diff.is_empty() {
            continue;
        }
        changed_cases.push(case_id.clone());
        diff_report.push_str(&format!("## {case_id}\n{field_diff}\n"));
        write_canonical_json_pair(&case_path, &live.case, &baseline_path, &live.facts)?;
    }
    let diff_path = corpus_dir.join(ACCEPTED_DIFF_FILE);
    fs::write(&diff_path, diff_report).map_err(|source| EvaluationError::Io {
        path: diff_path.clone(),
        source,
    })?;
    Ok(AcceptanceReport {
        changed_cases,
        diff_path,
    })
}

/// Records the MultiPV comparison search each Critical Moment needs, in place, using only
/// the installed Stockfish.
///
/// The recorded single-PV evidence stays untouched: it was captured with the Human Move
/// Model alongside it, and re-capturing that is neither free nor reproducible against a
/// different Maia image. Every comparison is therefore anchored by first reproducing the
/// recorded authoritative analysis for the same Position — a case whose Engine Analysis
/// this Stockfish cannot reproduce is refused rather than half-recorded.
pub async fn record_multi_pv_evidence(
    corpus_dir: &Path,
    engine: &dyn EngineAnalyzer,
) -> Result<RecordingReport, EvaluationError> {
    let case_paths = paths_with_suffix(corpus_dir, ".case.json")?;
    if case_paths.is_empty() {
        return Err(EvaluationError::EmptyCorpus(corpus_dir.to_path_buf()));
    }
    let mut changed_cases = Vec::new();
    let mut recorded_comparisons = 0;
    for path in case_paths {
        let mut case: CorpusCase = read_typed_json(&path)?;
        validate_case_identity(&path, &case)?;
        let EvaluationOperation::Game { review_side } = case.input.operation else {
            continue;
        };
        let game = parse_pgn(&case.input.pgn)?;
        let elo = EloProfile::try_from(case.input.elo).map_err(EvaluationError::InvalidElo)?;
        let facts = case_facts(&case, &game, elo, review_side)?;
        if facts.critical_moments.is_empty() {
            continue;
        }
        let engine_provenance = reproduce_engine_provenance(&case, &game, &facts, engine).await?;
        let (engine_provenance, recorded) = record_case_comparisons(
            &case,
            &game,
            elo,
            review_side,
            Some(engine_provenance),
            engine,
        )
        .await?;
        recorded_comparisons += recorded.len();
        let before = serde_json::to_value(&case)?;
        case.engine_provenance = engine_provenance;
        case.multi_pv_evidence = recorded;
        let after = serde_json::to_value(&case)?;
        if diff_json(&before, &after).is_empty() {
            continue;
        }
        changed_cases.push(case.id.clone());
        write_canonical_json(&path, &after)?;
    }
    Ok(RecordingReport {
        changed_cases,
        recorded_comparisons,
    })
}

/// Runs the MultiPV comparison for every Critical Moment of one case that needs one.
///
/// The authoritative provenance travels with the recording because a comparison is only
/// publishable beside a single-PV candidate that states where its own score came from.
async fn record_case_comparisons(
    case: &CorpusCase,
    game: &Game,
    elo: EloProfile,
    review_side: ReviewSide,
    engine_provenance: Option<EngineProvenance>,
    engine: &dyn EngineAnalyzer,
) -> Result<(Option<EngineProvenance>, Vec<RecordedMultiPv>), EvaluationError> {
    // No `supports_multi_pv` escape: a Game case whose Critical Moments reach candidate
    // comparison has to record it, so an adapter that cannot run MultiPV must fail by
    // name rather than quietly produce a corpus with the comparison gate switched off.
    let facts = case_facts(case, game, elo, review_side)?;
    let game_ref = game_ref(&case.input.pgn)?;
    let comparable = multi_pv_recording::comparable_moments(
        &game_ref,
        game,
        &facts,
        &case.evidence,
        engine_provenance.as_ref(),
    )
    .map_err(|source| EvaluationError::MultiPvRecording {
        case_id: case.id.clone(),
        source,
    })?;
    let mut recorded = Vec::with_capacity(comparable.len());
    for moment in &comparable {
        recorded.push(
            multi_pv_recording::record(
                engine,
                moment,
                decision_explanation::AUTOMATIC_DECISION_MULTI_PV,
            )
            .await?,
        );
    }
    Ok((engine_provenance, recorded))
}

fn case_facts(
    case: &CorpusCase,
    game: &Game,
    elo: EloProfile,
    review_side: ReviewSide,
) -> Result<RuleExtraction, EvaluationError> {
    let rule_evidence = case
        .evidence
        .iter()
        .map(ProviderEvidence::as_rule_evidence)
        .collect::<Vec<_>>();
    Ok(rule_extractor::extract(
        game,
        elo,
        review_side,
        &rule_evidence,
    )?)
}

/// Reproduces the recorded authoritative analysis at every Critical Moment Position and
/// returns the provenance of the engine that reproduced it.
async fn reproduce_engine_provenance(
    case: &CorpusCase,
    game: &Game,
    facts: &RuleExtraction,
    engine: &dyn EngineAnalyzer,
) -> Result<EngineProvenance, EvaluationError> {
    let mut measured = None;
    for moment in &facts.critical_moments {
        let recorded = case
            .evidence
            .iter()
            .find(|item| item.ply == moment.ply)
            .ok_or(EvaluationError::EngineDrift {
                case_id: case.id.clone(),
                ply: moment.ply,
            })?;
        let position = game
            .moves
            .iter()
            .find(|game_move| game_move.ply == moment.ply)
            .ok_or(EvaluationError::EngineDrift {
                case_id: case.id.clone(),
                ply: moment.ply,
            })?;
        let output = engine
            .analyze_with_provenance(EngineAnalysisInput {
                position: &position.position,
            })
            .await?;
        if output.analysis != recorded.engine_before {
            return Err(EvaluationError::EngineDrift {
                case_id: case.id.clone(),
                ply: moment.ply,
            });
        }
        measured = output.provenance;
    }
    measured.ok_or_else(|| EvaluationError::MissingEngineProvenance(case.id.clone()))
}

struct LiveCaseResult {
    case: Value,
    facts: Value,
}

async fn evaluate_live_cases(
    corpus_dir: &Path,
    service: &ReviewFactsService,
    provenance: LiveProviderProvenance,
    progress: &mut impl FnMut(usize, usize, &str),
) -> Result<BTreeMap<String, LiveCaseResult>, EvaluationError> {
    let case_paths = paths_with_suffix(corpus_dir, ".case.json")?;
    if case_paths.is_empty() {
        return Err(EvaluationError::EmptyCorpus(corpus_dir.to_path_buf()));
    }
    let mut cases = BTreeMap::new();
    let case_count = case_paths.len();
    let mut next_progress = 1;
    for (index, path) in case_paths.into_iter().enumerate() {
        let mut case: CorpusCase = read_typed_json(&path)?;
        validate_case_identity(&path, &case)?;
        if index + 1 >= next_progress {
            progress(index + 1, case_count, &case.id);
            next_progress =
                next_progress.saturating_add(crate::operating_limits::PROGRESS_EVERY_CASES);
        }
        let game = parse_pgn(&case.input.pgn)?;
        if game.moves.len() > crate::operating_limits::MAX_GAME_PLIES {
            return Err(EvaluationError::GameTooLarge {
                plies: game.moves.len(),
                maximum: crate::operating_limits::MAX_GAME_PLIES,
            });
        }
        let elo = EloProfile::try_from(case.input.elo).map_err(EvaluationError::InvalidElo)?;
        match case.input.operation {
            EvaluationOperation::Game { review_side } => {
                let (evidence, engine_provenance) =
                    service.collect_game_evidence(&game, elo).await?;
                case.evidence = evidence;
                // The comparison searches ride along with the capture. Leaving the
                // previous recording beside freshly captured evidence would pin a gap to
                // a search of a Position that may no longer be a Critical Moment.
                let (engine_provenance, recorded) = record_case_comparisons(
                    &case,
                    &game,
                    elo,
                    review_side,
                    engine_provenance,
                    service.engine().as_ref(),
                )
                .await?;
                case.engine_provenance = engine_provenance;
                case.multi_pv_evidence = recorded;
            }
            EvaluationOperation::SelectedMoment { selected_ply } => {
                case.evidence = vec![
                    service
                        .collect_selected_evidence(&game, elo, selected_ply)
                        .await?,
                ];
            }
        }
        case.provenance = provenance.clone();
        let facts = evaluate_case(&case)?;
        let case_value = serde_json::to_value(&case)?;
        cases.insert(
            case.id.clone(),
            LiveCaseResult {
                case: case_value,
                facts,
            },
        );
    }
    Ok(cases)
}

fn labeled_diff(case_diff: &str, facts_diff: &str) -> String {
    match (case_diff.is_empty(), facts_diff.is_empty()) {
        (true, true) => String::new(),
        (false, true) => format!("### evidence\n{case_diff}"),
        (true, false) => format!("### facts\n{facts_diff}"),
        (false, false) => format!("### evidence\n{case_diff}\n### facts\n{facts_diff}"),
    }
}

fn evaluate_cases(corpus_dir: &Path) -> Result<BTreeMap<String, Value>, EvaluationError> {
    let case_paths = paths_with_suffix(corpus_dir, ".case.json")?;
    if case_paths.is_empty() {
        return Err(EvaluationError::EmptyCorpus(corpus_dir.to_path_buf()));
    }

    let mut cases = BTreeMap::new();
    for path in case_paths {
        let case: CorpusCase = read_typed_json(&path)?;
        validate_case_identity(&path, &case)?;
        let baseline = evaluate_case(&case)?;
        cases.insert(case.id, baseline);
    }
    Ok(cases)
}

fn validate_case_identity(path: &Path, case: &CorpusCase) -> Result<(), EvaluationError> {
    let case_id = identity_from_path(path, ".case.json")?;
    if !valid_case_id(&case_id) {
        return Err(EvaluationError::InvalidCaseId(case_id));
    }
    if !valid_case_id(&case.id) {
        return Err(EvaluationError::InvalidCaseId(case.id.clone()));
    }
    if case.id != case_id {
        return Err(EvaluationError::CaseIdMismatch {
            path: path.to_path_buf(),
            declared: case.id.clone(),
            filename: case_id,
        });
    }
    Ok(())
}

fn reject_orphan_baselines<'a>(
    corpus_dir: &Path,
    case_ids: impl Iterator<Item = &'a String>,
) -> Result<(), EvaluationError> {
    let case_ids = case_ids.map(String::as_str).collect::<HashSet<_>>();
    for path in paths_with_suffix(corpus_dir, ".baseline.json")? {
        let baseline_id = identity_from_path(&path, ".baseline.json")?;
        if !case_ids.contains(baseline_id.as_str()) {
            return Err(EvaluationError::OrphanBaseline(path));
        }
    }
    Ok(())
}

fn paths_with_suffix(corpus_dir: &Path, suffix: &str) -> Result<Vec<PathBuf>, EvaluationError> {
    let entries = fs::read_dir(corpus_dir).map_err(|source| EvaluationError::Io {
        path: corpus_dir.to_path_buf(),
        source,
    })?;
    let mut paths = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|source| EvaluationError::Io {
                path: corpus_dir.to_path_buf(),
                source,
            })?
            .path();
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(suffix))
        {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

fn identity_from_path(path: &Path, suffix: &str) -> Result<String, EvaluationError> {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_suffix(suffix))
        .map(str::to_string)
        .ok_or_else(|| EvaluationError::InvalidCorpusPath(path.to_path_buf()))
}

fn valid_case_id(case_id: &str) -> bool {
    !case_id.is_empty()
        && case_id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && case_id
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && case_id
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric)
}

/// One frozen case's Review Moments, addressed by ply, in the form Language
/// Layer authoring consumes.
#[derive(Debug, Clone)]
pub struct RecordedCommentCase {
    pub case_id: String,
    pub description: String,
    pub elo: u16,
    pub kind: RecordedCaseKind,
    pub moments: Vec<RecordedCommentMoment>,
}

/// Which operation a recorded case replays.
///
/// Carried because the two produce different authoring problems: a Neutral
/// moment and a Terminal played outcome are reachable only from a moment the
/// Player selected, so a caller measuring coverage has to be able to tell the
/// branches apart without re-reading the case file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordedCaseKind {
    FullGameReview,
    PlayerSelectedMoment,
}

#[derive(Debug, Clone)]
pub struct RecordedCommentMoment {
    pub ply: u16,
    pub facts: ReviewMomentCommentFacts,
}

/// Replays a frozen case into the Review Moments a Language Layer would author
/// against, using only the recorded provider evidence.
///
/// This is the capability the bake-off needs and the corpus did not have: the
/// corpus records whole games, and a measurement addresses **one moment**. It
/// runs no provider, so it is available whether or not the Local Pipeline
/// Runtime is up.
pub fn recorded_comment_case(
    corpus_dir: &Path,
    case_id: &str,
) -> Result<RecordedCommentCase, EvaluationError> {
    let case: CorpusCase = read_typed_json(&case_path(corpus_dir, case_id))?;
    let game = parse_pgn(&case.input.pgn)?;
    let elo = EloProfile::try_from(case.input.elo).map_err(EvaluationError::InvalidElo)?;
    let game_ref = game_ref(&case.input.pgn)?;

    let presented = match case.input.operation {
        EvaluationOperation::Game { review_side } => game_review::recorded_critical_moments(
            &game,
            elo,
            review_side,
            &case.evidence,
            &game_ref,
        )?,
        EvaluationOperation::SelectedMoment { selected_ply } => {
            if case.evidence.len() != 1 || case.evidence[0].ply != selected_ply {
                return Err(EvaluationError::InvalidSelectedEvidence { selected_ply });
            }
            game_review::recorded_selected_moment(&game, elo, &case.evidence, &game_ref)?
        }
    };

    let moments = presented
        .into_iter()
        .map(|moment| {
            let ply = moment.ply;
            ReviewMomentCommentFacts::try_from_moment(moment)
                .map(|facts| RecordedCommentMoment { ply, facts })
                .map_err(|error| EvaluationError::Contract(error.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(RecordedCommentCase {
        case_id: case.id,
        description: case.description,
        elo: case.input.elo,
        kind: match case.input.operation {
            EvaluationOperation::Game { .. } => RecordedCaseKind::FullGameReview,
            EvaluationOperation::SelectedMoment { .. } => RecordedCaseKind::PlayerSelectedMoment,
        },
        moments,
    })
}

/// Every case id in a corpus directory, in stable order.
pub fn corpus_case_ids(corpus_dir: &Path) -> Result<Vec<String>, EvaluationError> {
    paths_with_suffix(corpus_dir, ".case.json")?
        .iter()
        .map(|path| identity_from_path(path, ".case.json"))
        .collect()
}

fn evaluate_case(case: &CorpusCase) -> Result<Value, EvaluationError> {
    let game = parse_pgn(&case.input.pgn)?;
    let elo = EloProfile::try_from(case.input.elo).map_err(EvaluationError::InvalidElo)?;
    let evidence = case
        .evidence
        .iter()
        .map(ProviderEvidence::as_rule_evidence)
        .collect::<Vec<_>>();

    let evaluated = match case.input.operation {
        EvaluationOperation::Game { review_side } => {
            let extraction =
                rule_extractor::extract_with_trace(&game, elo, review_side, &evidence)?;
            let decision_explanations =
                replay_decision_explanations(case, &game, &extraction.facts)?;
            EvaluatedFacts {
                facts: serde_json::to_value(extraction.facts)?,
                selector_trace: Some(extraction.selector_trace),
                decision_explanations,
            }
        }
        EvaluationOperation::SelectedMoment { selected_ply } => {
            if evidence.len() != 1 || evidence[0].ply != selected_ply {
                return Err(EvaluationError::InvalidSelectedEvidence { selected_ply });
            }
            let selected = &evidence[0];
            EvaluatedFacts {
                facts: serde_json::to_value(rule_extractor::extract_selected_moment(
                    &game, elo, selected,
                )?)?,
                selector_trace: None,
                decision_explanations: Vec::new(),
            }
        }
    };
    Ok(serde_json::to_value(FactBaseline {
        case_id: case.id.clone(),
        provenance: case.provenance.clone(),
        facts: evaluated.facts,
        selector_trace: evaluated.selector_trace,
        decision_explanations: evaluated.decision_explanations,
    })?)
}

/// Rebuilds each Critical Moment's Decision Explanation from the recorded MultiPV
/// comparison, so the offline gate covers the candidate-comparison stage without an
/// engine.
fn replay_decision_explanations(
    case: &CorpusCase,
    game: &Game,
    facts: &RuleExtraction,
) -> Result<Vec<RecordedDecisionProof>, EvaluationError> {
    let game_ref = game_ref(&case.input.pgn)?;
    let comparable = multi_pv_recording::comparable_moments(
        &game_ref,
        game,
        facts,
        &case.evidence,
        case.engine_provenance.as_ref(),
    )
    .map_err(|source| EvaluationError::MultiPvRecording {
        case_id: case.id.clone(),
        source,
    })?;
    multi_pv_recording::decision_proofs(&game_ref, comparable, &case.multi_pv_evidence).map_err(
        |source| EvaluationError::MultiPvRecording {
            case_id: case.id.clone(),
            source,
        },
    )
}

fn game_ref(pgn: &str) -> Result<GameRef, EvaluationError> {
    GameRef::try_from(format!("sha256:{:x}", sha2::Sha256::digest(pgn.as_bytes())))
        .map_err(EvaluationError::InvalidGameRef)
}

fn baseline_path(corpus_dir: &Path, case_id: &str) -> PathBuf {
    corpus_dir.join(format!("{case_id}.baseline.json"))
}

fn case_path(corpus_dir: &Path, case_id: &str) -> PathBuf {
    corpus_dir.join(format!("{case_id}.case.json"))
}

fn read_typed_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, EvaluationError> {
    let bytes = fs::read(path).map_err(|source| EvaluationError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_slice(&bytes).map_err(|source| EvaluationError::Json {
        path: path.to_path_buf(),
        source,
    })
}

fn read_json(path: &Path) -> Result<Value, EvaluationError> {
    read_typed_json(path)
}

fn write_canonical_json(path: &Path, value: &Value) -> Result<(), EvaluationError> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    fs::write(path, bytes).map_err(|source| EvaluationError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn write_canonical_json_pair(
    case_path: &Path,
    case_value: &Value,
    baseline_path: &Path,
    baseline_value: &Value,
) -> Result<(), EvaluationError> {
    let case_staged = case_path.with_extension("json.next");
    let baseline_staged = baseline_path.with_extension("json.next");
    let case_backup = case_path.with_extension("json.previous");
    let baseline_backup = baseline_path.with_extension("json.previous");
    for artifact in [
        &case_staged,
        &baseline_staged,
        &case_backup,
        &baseline_backup,
    ] {
        remove_file_if_present(artifact)?;
    }
    write_canonical_json(&case_staged, case_value)?;
    if let Err(error) = write_canonical_json(&baseline_staged, baseline_value) {
        let _ = remove_file_if_present(&case_staged);
        return Err(error);
    }

    let baseline_existed = baseline_path.is_file();
    fs::rename(case_path, &case_backup).map_err(|source| EvaluationError::Io {
        path: case_path.to_path_buf(),
        source,
    })?;
    if baseline_existed {
        if let Err(source) = fs::rename(baseline_path, &baseline_backup) {
            let _ = fs::rename(&case_backup, case_path);
            return Err(EvaluationError::Io {
                path: baseline_path.to_path_buf(),
                source,
            });
        }
    }

    let publish = fs::rename(&case_staged, case_path)
        .and_then(|_| fs::rename(&baseline_staged, baseline_path));
    if let Err(source) = publish {
        let rollback = rollback_json_pair(
            case_path,
            &case_backup,
            baseline_path,
            &baseline_backup,
            baseline_existed,
        );
        return match rollback {
            Ok(()) => Err(EvaluationError::Io {
                path: baseline_path.to_path_buf(),
                source,
            }),
            Err(rollback) => Err(EvaluationError::AcceptanceRollback {
                cause: source.to_string(),
                rollback: rollback.to_string(),
            }),
        };
    }

    let _ = remove_file_if_present(&case_backup);
    let _ = remove_file_if_present(&baseline_backup);
    Ok(())
}

fn rollback_json_pair(
    case_path: &Path,
    case_backup: &Path,
    baseline_path: &Path,
    baseline_backup: &Path,
    baseline_existed: bool,
) -> std::io::Result<()> {
    let _ = fs::remove_file(case_path);
    let _ = fs::remove_file(baseline_path);
    fs::rename(case_backup, case_path)?;
    if baseline_existed {
        fs::rename(baseline_backup, baseline_path)?;
    }
    Ok(())
}

fn remove_file_if_present(path: &Path) -> Result<(), EvaluationError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(EvaluationError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn diff_json(expected: &Value, actual: &Value) -> String {
    let mut lines = Vec::new();
    collect_diff(
        "",
        expected,
        actual,
        expected,
        actual,
        DiffMode::Exact,
        &mut lines,
    );
    lines.join("\n")
}

fn diff_json_with_tolerances(
    expected: &Value,
    actual: &Value,
    tolerances: LiveTolerances,
) -> String {
    let mut lines = Vec::new();
    collect_diff(
        "",
        expected,
        actual,
        expected,
        actual,
        DiffMode::Live(tolerances),
        &mut lines,
    );
    lines.join("\n")
}

fn collect_diff(
    path: &str,
    expected: &Value,
    actual: &Value,
    expected_root: &Value,
    actual_root: &Value,
    mode: DiffMode,
    lines: &mut Vec<String>,
) {
    match (expected, actual) {
        (Value::Object(expected), Value::Object(actual)) => {
            let keys = expected.keys().chain(actual.keys()).collect::<HashSet<_>>();
            let mut keys = keys.into_iter().collect::<Vec<_>>();
            keys.sort();
            for key in keys {
                let child = format!("{}/{}", path, json_pointer_segment(key));
                match (expected.get(key), actual.get(key)) {
                    (Some(left), Some(right)) => {
                        collect_diff(&child, left, right, expected_root, actual_root, mode, lines)
                    }
                    (Some(left), None) => lines.push(format!("- {child}: {left}")),
                    (None, Some(right)) => lines.push(format!("+ {child}: {right}")),
                    (None, None) => {}
                }
            }
        }
        (Value::Array(expected), Value::Array(actual)) => {
            for index in 0..expected.len().max(actual.len()) {
                let child = format!("{path}/{index}");
                match (expected.get(index), actual.get(index)) {
                    (Some(left), Some(right)) => {
                        collect_diff(&child, left, right, expected_root, actual_root, mode, lines)
                    }
                    (Some(left), None) => lines.push(format!("- {child}: {left}")),
                    (None, Some(right)) => lines.push(format!("+ {child}: {right}")),
                    (None, None) => {}
                }
            }
        }
        _ if expected != actual
            && !within_live_tolerance(path, expected, actual, expected_root, actual_root, mode) =>
        {
            let path = if path.is_empty() { "/" } else { path };
            lines.push(format!("- {path}: {expected}"));
            lines.push(format!("+ {path}: {actual}"));
        }
        _ => {}
    }
}

fn within_live_tolerance(
    path: &str,
    expected: &Value,
    actual: &Value,
    expected_root: &Value,
    actual_root: &Value,
    mode: DiffMode,
) -> bool {
    let tolerances = match mode {
        DiffMode::Exact => return false,
        DiffMode::Live(tolerances) => tolerances,
    };
    let (Some(expected), Some(actual)) = (expected.as_f64(), actual.as_f64()) else {
        return false;
    };
    let field = path.rsplit('/').next().unwrap_or_default();
    if field == "probability" || field.ends_with("Probability") {
        return float_within_tolerance(expected, actual, tolerances.probability);
    }
    if path.ends_with("/centipawnLoss") {
        return (expected - actual).abs() <= f64::from(tolerances.centipawns);
    }
    let Some(parent) = path.strip_suffix("/value") else {
        return false;
    };
    let kind_path = format!("{parent}/kind");
    let expected_kind = expected_root.pointer(&kind_path).and_then(Value::as_str);
    let actual_kind = actual_root.pointer(&kind_path).and_then(Value::as_str);
    expected_kind == Some("centipawns")
        && actual_kind == Some("centipawns")
        && (expected - actual).abs() <= f64::from(tolerances.centipawns)
}

fn float_within_tolerance(expected: f64, actual: f64, tolerance: f64) -> bool {
    let difference = (expected - actual).abs();
    difference <= tolerance || (difference - tolerance).abs() <= 1e-12
}

fn json_pointer_segment(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

#[derive(Debug, thiserror::Error)]
pub enum EvaluationError {
    #[error("failed to access {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid JSON in {path}: {source}")]
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("invalid evaluation case id {0}; use lowercase letters, digits, and internal hyphens")]
    InvalidCaseId(String),
    #[error("case id {declared} in {path} does not match filename identity {filename}")]
    CaseIdMismatch {
        path: PathBuf,
        declared: String,
        filename: String,
    },
    #[error("baseline {0} has no matching case")]
    OrphanBaseline(PathBuf),
    #[error("evaluation corpus path {0} is not valid UTF-8 or has an invalid suffix")]
    InvalidCorpusPath(PathBuf),
    #[error("evaluation corpus {0} contains no cases")]
    EmptyCorpus(PathBuf),
    #[error("unsupported Game size: {plies} plies exceeds the certified maximum of {maximum}")]
    GameTooLarge { plies: usize, maximum: usize },
    #[error(
        "selected-moment case must contain exactly one evidence record for ply {selected_ply}"
    )]
    InvalidSelectedEvidence { selected_ply: usize },
    #[error("invalid evaluation Elo: {0}")]
    InvalidElo(String),
    #[error("recorded case does not yield authorable Review Moment facts: {0}")]
    Contract(String),
    #[error("case {case_id} cannot replay its MultiPV comparison: {source}")]
    MultiPvRecording {
        case_id: String,
        source: MultiPvRecordingError,
    },
    #[error("case {case_id} recorded Engine Analysis that the installed Stockfish does not reproduce at ply {ply}")]
    EngineDrift { case_id: String, ply: usize },
    #[error("the installed Stockfish reported no provenance for case {0}")]
    MissingEngineProvenance(String),
    #[error("evaluation case PGN has no valid Game Ref: {0}")]
    InvalidGameRef(ContractValueError),
    #[error(transparent)]
    Engine(#[from] EngineAnalysisError),
    #[error(transparent)]
    Pgn(#[from] PgnImportError),
    #[error(transparent)]
    Rule(#[from] RuleExtractorError),
    #[error(transparent)]
    Review(#[from] ReviewFactsError),
    #[error(
        "live acceptance failed: {cause}; restoring the previous pair also failed: {rollback}"
    )]
    AcceptanceRollback { cause: String, rollback: String },
    #[error(transparent)]
    Serialize(#[from] serde_json::Error),
}
