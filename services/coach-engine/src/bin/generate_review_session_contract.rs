use std::{fs, path::Path};

use chen_chess_coach_engine::{
    chess_com::chess_com_game_url_pattern,
    daily_coaching::{
        CheckPlayingProfileOutcome, CheckPlayingProfileRequest, CoachingHost,
        CoachingHostConnection, ConnectPlayingProfileOutcome, ConnectPlayingProfileRequest,
        DailyCoachingDashboardState, DailyCoachingDigestDetail, DailyCoachingMutationOutcome,
        DailyCoachingSetupState, RecentPlayingProfileGame, RecentPlayingProfileGamesOutcome,
        RemovePlayingProfileRequest, ReplacePlayingProfileRequest, SetDailyCoachingEnabledRequest,
    },
    decision_explanation::{explain_decision, DecisionExplanationBuild, DecisionExplanationInput},
    game_import::build_lichess_imported_game,
    imported_games::{ImportedGameListPage, PlayedOpeningsResult},
    lichess::{LichessExportResponse, LichessGameUrl, LICHESS_PGN_MEDIA_TYPE},
    opening_analysis::{OpeningAnalysisOutcome, OpeningAnalysisRequest, ResolveOpeningLineOutcome},
    opening_identification::{FindOpeningLinesRequest, OpeningLineFindResult},
    pgn::parse_pgn,
    review_session_contract::*,
    reviewed_games::{ReviewedGameSearchRequest, ReviewedGameSearchResult},
    shared_assets::SharedLimits,
};
use schemars::{schema_for, JsonSchema};
use serde::Serialize;
use serde_json::Value;
use ts_rs::{Config, TS};

#[path = "generate_review_session_contract/tree_io.rs"]
mod tree_io;

use tree_io::{publish_generated_tree, verify_generated_tree};

const PACKAGE_RELATIVE_DIR: &str = "packages/coach-engine-sdk";
const CANONICAL_DECISION_EXPLANATION_FEN: &str = "r2qk3/2p5/8/1N6/8/8/8/4K3 w - - 0 1";

/// The contract's roots, declared once.
///
/// Both publishers read this list: ts-rs walks it for the TypeScript types,
/// and schemars walks the catalog struct for the JSON Schema `$defs`. Keeping
/// them as two hand-written lists let them drift — `CoachingHost` and
/// `CoachingHostConnection` were exported as types while absent from the
/// catalog, and stayed in the schema only because the dashboard happened to
/// reach them.
macro_rules! contract_roots {
    ($($field:ident: $root:ty),+ $(,)?) => {
        #[derive(JsonSchema)]
        pub struct ReviewSessionSchemaCatalog {
            $(pub $field: $root,)+
        }

        fn export_contract_roots(config: &Config) -> anyhow::Result<()> {
            $(<$root>::export_all(config)?;)+
            Ok(())
        }
    };
}

contract_roots! {
    core: ReviewSessionCoreContract,
    command: ReviewSessionCommandEnvelope,
    event: ReviewSessionEventEnvelope,
    presentation: ReviewSessionPresentation,
    presentation_addition: ReviewSessionPresentationAddition,
    game_review_snapshot: GameReviewSnapshot,
    review_moment_snapshot: ReviewMomentSnapshot,
    move_sequence_snapshot: MoveSequenceSnapshot,
    move_sequence_presentation: MoveSequencePresentation,
    daily_coaching_setup_state: DailyCoachingSetupState,
    daily_coaching_dashboard_state: DailyCoachingDashboardState,
    coaching_host: CoachingHost,
    coaching_host_connection: CoachingHostConnection,
    daily_coaching_digest_detail: DailyCoachingDigestDetail,
    connect_playing_profile_request: ConnectPlayingProfileRequest,
    connect_playing_profile_outcome: ConnectPlayingProfileOutcome,
    check_playing_profile_request: CheckPlayingProfileRequest,
    check_playing_profile_outcome: CheckPlayingProfileOutcome,
    replace_playing_profile_request: ReplacePlayingProfileRequest,
    remove_playing_profile_request: RemovePlayingProfileRequest,
    set_daily_coaching_enabled_request: SetDailyCoachingEnabledRequest,
    daily_coaching_mutation_outcome: DailyCoachingMutationOutcome,
    imported_game_list_page: ImportedGameListPage,
    reviewed_game_search_request: ReviewedGameSearchRequest,
    reviewed_game_search_result: ReviewedGameSearchResult,
    recent_playing_profile_game: RecentPlayingProfileGame,
    recent_playing_profile_games_outcome: RecentPlayingProfileGamesOutcome,
    opening_analysis_request: OpeningAnalysisRequest,
    opening_analysis_outcome: OpeningAnalysisOutcome,
    resolve_opening_line_outcome: ResolveOpeningLineOutcome,
    find_opening_lines_request: FindOpeningLinesRequest,
    opening_line_find_result: OpeningLineFindResult,
    played_openings_result: PlayedOpeningsResult,
}

fn main() -> anyhow::Result<()> {
    let check = match std::env::args().skip(1).collect::<Vec<_>>().as_slice() {
        [] => false,
        [flag] if flag == "--check" => true,
        _ => anyhow::bail!("usage: generate_review_session_contract [--check]"),
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("API should have a repository parent");
    let staging = std::env::temp_dir().join(format!(
        "chenchess-review-session-contract-{}",
        std::process::id()
    ));
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    fs::create_dir_all(&staging)?;

    let result = generate(root, &staging).and_then(|()| {
        if check {
            verify_generated_tree(root, &staging)
        } else {
            publish_generated_tree(root, &staging)
        }
    });
    fs::remove_dir_all(&staging)?;
    result
}

fn generate(root: &Path, staging: &Path) -> anyhow::Result<()> {
    let package_dir = staging.join(PACKAGE_RELATIVE_DIR);
    let typescript_dir = package_dir.join("src");
    fs::create_dir_all(package_dir.join("fixtures"))?;
    fs::create_dir_all(&typescript_dir)?;

    let mut schema = serde_json::to_value(schema_for!(ReviewSessionSchemaCatalog))?;
    let object = schema
        .as_object_mut()
        .expect("root JSON Schema should be an object");
    for key in ["properties", "required", "type", "title"] {
        object.remove(key);
    }
    object.insert(
        "$ref".to_string(),
        Value::String("#/$defs/ReviewSessionCoreContract".to_string()),
    );
    object.insert(
        "$id".to_string(),
        Value::String("https://chenchess.local/coach-engine-sdk/schema.json".to_string()),
    );
    let schema_bytes = pretty_json(&schema)?;
    fs::write(
        typescript_dir.join("review-session.schema.json"),
        &schema_bytes,
    )?;

    let config = Config::new()
        .with_out_dir(&typescript_dir)
        .with_large_int("number")
        .with_import_extension(Some("js"));
    export_contract_roots(&config)?;
    brand_semantic_types(&typescript_dir)?;
    write_typescript_support(&typescript_dir)?;
    assert_host_turn_prior_turns_match_v1(root)?;

    let core = canonical_fixture(root)?;
    let (commands, events) = operation_fixtures(&core)?;
    write_fixture(&package_dir.join("fixtures/core-contract.json"), &core)?;
    write_fixture(
        &package_dir.join("fixtures/imported-game.json"),
        &core.imported_game,
    )?;
    write_fixture(
        &package_dir.join("fixtures/position-snapshot.json"),
        &core.position_snapshot,
    )?;
    write_fixture(
        &package_dir.join("fixtures/evidence-packet.json"),
        &core.evidence_packet,
    )?;
    write_fixture(&package_dir.join("fixtures/commands.json"), &commands)?;
    write_fixture(&package_dir.join("fixtures/events.json"), &events)?;
    fs::write(package_dir.join("README.md"), contract_readme())?;
    fs::write(package_dir.join("package.json"), package_manifest())?;
    fs::write(package_dir.join("tsconfig.json"), typescript_config())?;
    Ok(())
}

fn canonical_fixture(root: &Path) -> anyhow::Result<ReviewSessionCoreContract> {
    let fixture_root = root.join("packages/shared-assets/fixtures/Synthet1");
    let pgn = fs::read_to_string(fixture_root.join("lichess-export.pgn"))?;
    let raw_export = fs::read(fixture_root.join("lichess-export.raw.pgn"))?;
    let game = parse_pgn(&pgn)?;
    let first_fen = game
        .moves
        .first()
        .map(|game_move| game_move.position.as_str())
        .ok_or_else(|| anyhow::anyhow!("canonical Game should contain moves"))?;
    let position_snapshot = build_position_snapshot(first_fen, &[])?;
    let source = LichessGameUrl::parse("https://lichess.org/Synthet1Demo/black")?;
    let imported_game = build_lichess_imported_game(
        &source,
        LichessExportResponse {
            body: raw_export,
            content_type: LICHESS_PGN_MEDIA_TYPE.to_string(),
            captured_at: "2026-09-03T00:00:00Z".parse()?,
        },
        ReviewSide::Black,
        &RequestedEloProfile::FromImportedMetadata,
    )?;
    let game_ref = imported_game.game.game_ref.clone();
    let position_entry = EvidenceEntry::position(
        EvidenceMetadata::derived("canonical-position-snapshot/v1", Vec::new()),
        position_snapshot.clone(),
    );
    let position_evidence_id = position_entry.metadata().evidence_id.clone();
    let moment_id = CriticalMomentId::for_imported_game(&game_ref, 1);
    // The anchor is the Game's own first move, so replacing the canonical Game
    // cannot leave the fixture pointing at a move it does not contain.
    let first_move_uci = game
        .moves
        .first()
        .map(|game_move| game_move.uci.clone())
        .ok_or_else(|| anyhow::anyhow!("canonical Game should contain moves"))?;
    Ok(ReviewSessionCoreContract {
        request_id: RequestId::try_from("request:fixture:1".to_string())?,
        imported_game,
        position_snapshot: position_snapshot.clone(),
        review_moment: ReviewMomentOccurrence {
            moment_id: moment_id.clone(),
            ply: 1,
            preceding_move: None,
            selection: ReviewMomentSelection::PipelineCriticalMoment {
                critical_moment_id: moment_id.clone(),
            },
            game_ref,
        },
        coach_turn_context: CoachTurnContext {
            coach_turn_id: CoachTurnId::try_from("coach-turn:fixture:1".to_string())?,
            reviewed_move: ReviewedMoveAnchor {
                critical_moment_id: moment_id.clone(),
                ply: 1,
                side: Color::White,
                position_ref: position_snapshot.position_ref.clone(),
                played_move_uci: first_move_uci.clone(),
            },
            selected_position_ref: position_snapshot.position_ref,
            target: CoachTurnTarget::ImportedGameMove {
                critical_moment_id: moment_id,
                ply: 1,
                uci: first_move_uci,
            },
            required_evidence_refs: vec![position_evidence_id],
        },
        evidence_packet: ReviewSessionEvidencePacket {
            entries: vec![position_entry],
        },
    })
}

fn operation_fixtures(
    core: &ReviewSessionCoreContract,
) -> anyhow::Result<(
    Vec<ReviewSessionCommandEnvelope>,
    Vec<ReviewSessionEventEnvelope>,
)> {
    let web_request = RequestId::try_from("request:fixture:web-import".to_string())?;
    let web_operation = OperationId::try_from("operation:fixture:web-import".to_string())?;
    let coach_request = RequestId::try_from("request:fixture:coach-import".to_string())?;
    let coach_operation = OperationId::try_from("operation:fixture:coach-import".to_string())?;
    let coach_app_request = RequestId::try_from("request:fixture:coach-app-import".to_string())?;
    let coach_app_operation =
        OperationId::try_from("operation:fixture:coach-app-import".to_string())?;
    let start_request = RequestId::try_from("request:fixture:start".to_string())?;
    let start_operation = OperationId::try_from("operation:fixture:start".to_string())?;
    let game_import_id =
        GameImportId::try_from(format!("game-import:{}:{}", "a".repeat(64), "b".repeat(32)))?;
    let commands = vec![
        ReviewSessionCommandEnvelope {
            request_id: web_request.clone(),
            operation_id: web_operation.clone(),
            surface: DeliverySurface::Web,
            command: ReviewSessionCommand::ImportGame {
                source: GameInputSource::LichessUrl {
                    url: "https://lichess.org/Synthet1Demo/black".to_string(),
                },
                review_side: RequestedReviewSide::FromQualifiedUrl,
                elo_profile: RequestedEloProfile::FromImportedMetadata,
            },
        },
        ReviewSessionCommandEnvelope {
            request_id: coach_request,
            operation_id: coach_operation,
            surface: DeliverySurface::CoachSkill,
            command: ReviewSessionCommand::ImportGame {
                source: GameInputSource::LocalPgnFile {
                    path: "/private/player/game.pgn".to_string(),
                },
                review_side: RequestedReviewSide::Selected {
                    review_side: ReviewSide::Black,
                },
                elo_profile: RequestedEloProfile::PlayerProvided {
                    rating: EloRating::try_from(1246)?,
                },
            },
        },
        ReviewSessionCommandEnvelope {
            request_id: coach_app_request,
            operation_id: coach_app_operation,
            surface: DeliverySurface::CoachApp,
            command: ReviewSessionCommand::ImportGame {
                source: GameInputSource::ChessComUrl {
                    url: "https://www.chess.com/game/computer/1403674481".to_string(),
                },
                review_side: RequestedReviewSide::Selected {
                    review_side: ReviewSide::White,
                },
                elo_profile: RequestedEloProfile::FromImportedMetadata,
            },
        },
        ReviewSessionCommandEnvelope {
            request_id: start_request.clone(),
            operation_id: start_operation.clone(),
            surface: DeliverySurface::CoachApp,
            command: ReviewSessionCommand::StartReviewSession {
                game_import_id: game_import_id.clone(),
            },
        },
        ReviewSessionCommandEnvelope {
            request_id: RequestId::try_from("request:fixture:host-turn".to_string())?,
            operation_id: OperationId::try_from("operation:fixture:host-turn".to_string())?,
            surface: DeliverySurface::Web,
            command: ReviewSessionCommand::StartHostTurn {
                game_import_id: game_import_id.clone(),
                message: "Why was this move a mistake?".to_string(),
                prior_turns: vec![HostTurnPriorTurn {
                    message: "What should I have played instead?".to_string(),
                    answer: "Nf6 keeps the knight on a stable square.".to_string(),
                }],
                idempotency_key: IdempotencyKey::try_from(
                    "idempotency-key:fixture:host-turn".to_string(),
                )?,
            },
        },
    ];
    let review = fixture_game_review(core);
    let review_moment_learning_material = review.critical_moments[0].learning_material.clone();
    let game_import_id = GameImportId::try_from("game-import:fixture:1".to_string())?;
    let snapshot_review = review.clone();
    let snapshot_learning_material = review_moment_learning_material.clone();
    let snapshot_request = RequestId::try_from("request:fixture:snapshot".to_string())?;
    let snapshot_operation = OperationId::try_from("operation:fixture:snapshot".to_string())?;
    let moment_request = RequestId::try_from("request:fixture:review-moment".to_string())?;
    let moment_operation = OperationId::try_from("operation:fixture:review-moment".to_string())?;
    let explanation_request =
        RequestId::try_from("request:fixture:decision-explanation".to_string())?;
    let explanation_operation =
        OperationId::try_from("operation:fixture:decision-explanation".to_string())?;
    let host_turn_request = RequestId::try_from("request:fixture:host-turn".to_string())?;
    let host_turn_operation = OperationId::try_from("operation:fixture:host-turn".to_string())?;
    let decision_explanation = canonical_decision_explanation()?;
    let explanation_moment_id = decision_explanation.critical_moment_id.clone();
    let grounded_moment = chen_chess_coach_engine::grounded_review_moment::ground_review_moment(
        &game_import_id,
        &core.imported_game,
        &review.critical_moments[0],
        &core.position_snapshot,
        review.critical_moments[0].decision_explanation.as_ref(),
    );
    let events = vec![
        event_fixture(
            &web_request,
            &web_operation,
            0,
            ReviewSessionEvent::Accepted {
                operation: OperationKind::GameImport,
                limits: ReviewSessionLimits::default(),
            },
        ),
        event_fixture(
            &web_request,
            &web_operation,
            1,
            ReviewSessionEvent::Progress {
                stage: OperationProgress::Import {
                    stage: ImportProgressStage::FetchingGame,
                },
            },
        ),
        event_fixture(
            &web_request,
            &web_operation,
            2,
            ReviewSessionEvent::Completed {
                result: Box::new(OperationCompletion::GameImported {
                    game_import_id: game_import_id.clone(),
                    imported_game: Some(Box::new(core.imported_game.clone())),
                    review: Box::new(review.clone()),
                    timing: None,
                }),
            },
        ),
        event_fixture(
            &start_request,
            &start_operation,
            0,
            ReviewSessionEvent::Accepted {
                operation: OperationKind::ReviewSessionStart,
                limits: ReviewSessionLimits::default(),
            },
        ),
        event_fixture(
            &start_request,
            &start_operation,
            1,
            ReviewSessionEvent::Completed {
                result: Box::new(OperationCompletion::ReviewSessionStarted {
                    game_import_id: game_import_id.clone(),
                    session_revision: 1,
                    review: Box::new(review),
                    imported_game: Box::new(core.imported_game.clone()),
                    review_moments: vec![ReviewSessionMoment::prepared(
                        core.clone(),
                        review_moment_learning_material,
                        Some(ReviewMomentClassificationKind::ImprovementOpportunity),
                    )],
                }),
            },
        ),
        event_fixture(
            &snapshot_request,
            &snapshot_operation,
            0,
            ReviewSessionEvent::Completed {
                result: Box::new(OperationCompletion::GameReviewSnapshotRead {
                    game_import_id: game_import_id.clone(),
                    review: Box::new(snapshot_review),
                    imported_game: Box::new(core.imported_game.clone()),
                    review_moments: vec![ReviewSessionMoment::pending(
                        core,
                        snapshot_learning_material,
                        Some(ReviewMomentClassificationKind::ImprovementOpportunity),
                    )],
                    content_digest: ReviewContentDigest::try_from(format!(
                        "sha256:{}",
                        "3".repeat(64)
                    ))?,
                }),
            },
        ),
        event_fixture(
            &moment_request,
            &moment_operation,
            0,
            ReviewSessionEvent::Completed {
                result: Box::new(OperationCompletion::ReviewMomentDetailRead {
                    detail: Box::new(grounded_moment),
                    content_digest: ReviewContentDigest::try_from(format!(
                        "sha256:{}",
                        "5".repeat(64)
                    ))?,
                }),
            },
        ),
        event_fixture(
            &web_request,
            &web_operation,
            2,
            ReviewSessionEvent::Unavailable {
                operation: OperationKind::GameImport,
                reason: ProviderUnavailableReason::RateLimited {
                    retry_after_seconds: 60,
                },
                retry: RetryDirective::RetryAfter { seconds: 60 },
            },
        ),
        event_fixture(
            &web_request,
            &web_operation,
            2,
            ReviewSessionEvent::Cancelled {
                operation: OperationKind::GameImport,
            },
        ),
        event_fixture(
            &web_request,
            &web_operation,
            2,
            ReviewSessionEvent::Conflict {
                operation: OperationKind::GameImport,
                reason: OperationConflictReason::IdempotencyKeyMismatch,
            },
        ),
        event_fixture(
            &web_request,
            &web_operation,
            2,
            ReviewSessionEvent::Rejected {
                operation: OperationKind::GameImport,
                reason: CommandRejectionReason::ReviewSideRequired,
                recovery: RejectionRecovery::SelectReviewSide,
            },
        ),
        event_fixture(
            &explanation_request,
            &explanation_operation,
            0,
            ReviewSessionEvent::Completed {
                result: Box::new(OperationCompletion::ReviewMomentExplanationRead {
                    game_import_id: game_import_id.clone(),
                    review_moment_id: explanation_moment_id,
                    explanation: Box::new(decision_explanation),
                }),
            },
        ),
        event_fixture(
            &host_turn_request,
            &host_turn_operation,
            0,
            ReviewSessionEvent::Progress {
                stage: OperationProgress::HostTurn {
                    label: HostTurnStepLabel::LookingAtAnotherMoment,
                },
            },
        ),
        event_fixture(
            &host_turn_request,
            &host_turn_operation,
            1,
            ReviewSessionEvent::Progress {
                stage: OperationProgress::HostTurn {
                    label: HostTurnStepLabel::CheckingThatLine,
                },
            },
        ),
        event_fixture(
            &host_turn_request,
            &host_turn_operation,
            2,
            ReviewSessionEvent::Progress {
                stage: OperationProgress::HostTurn {
                    label: HostTurnStepLabel::Writing,
                },
            },
        ),
        event_fixture(
            &host_turn_request,
            &host_turn_operation,
            3,
            ReviewSessionEvent::Completed {
                result: Box::new(OperationCompletion::HostTurnCompleted {
                    answer: "The knight was hanging after that capture.".to_string(),
                    focus_moment: Some(20),
                    show_line: Some(HostTurnShowLine::EngineBest),
                }),
            },
        ),
        event_fixture(
            &host_turn_request,
            &host_turn_operation,
            3,
            ReviewSessionEvent::Completed {
                result: Box::new(OperationCompletion::HostTurnCompleted {
                    answer: "That alternative keeps the extra pawn.".to_string(),
                    focus_moment: None,
                    show_line: Some(HostTurnShowLine::AlternativeMove {
                        alternative_move_id: AlternativeMoveId::try_from(
                            "alternative-move:fixture:host-turn".to_string(),
                        )?,
                    }),
                }),
            },
        ),
        event_fixture(
            &host_turn_request,
            &host_turn_operation,
            3,
            ReviewSessionEvent::Completed {
                result: Box::new(OperationCompletion::HostTurnCompleted {
                    answer: "The played reply still holds.".to_string(),
                    focus_moment: None,
                    show_line: Some(HostTurnShowLine::PlayedMoveRefutation),
                }),
            },
        ),
        event_fixture(
            &host_turn_request,
            &host_turn_operation,
            3,
            ReviewSessionEvent::Completed {
                result: Box::new(OperationCompletion::HostTurnRefused {
                    reason: HostTurnRefusalReason::NotAboutThisReview,
                }),
            },
        ),
        event_fixture(
            &host_turn_request,
            &host_turn_operation,
            3,
            ReviewSessionEvent::Unavailable {
                operation: OperationKind::HostTurn,
                reason: ProviderUnavailableReason::LanguageLayer,
                retry: RetryDirective::RetryAllowed,
            },
        ),
    ];
    Ok((commands, events))
}

fn canonical_decision_explanation() -> anyhow::Result<DecisionExplanation> {
    let evaluation = |value| EngineEvaluation::Centipawns {
        value,
        perspective: Color::White,
    };
    let provenance = DecisionEngineProvenance {
        engine: "Stockfish 18 fixture".to_string(),
        binary_digest: ArtifactDigest::try_from(format!("sha256:{}", "2".repeat(64)))?,
        depth: 16,
        threads: 1,
        hash_mib: 16,
    };
    let candidate_evidence = CandidateEvidence::MultiPv {
        authoritative_single_pv: EngineCandidateEvidence {
            rank: 1,
            root_move_uci: "b5c7".to_string(),
            evaluation: evaluation(500),
            variation: ["b5c7", "e8d7", "c7a8"]
                .into_iter()
                .map(str::to_string)
                .collect(),
            provenance: provenance.clone(),
        },
        requested_count: 3,
        ranked_alternatives: vec![
            RankedAlternativeEvidence {
                rank: 2,
                root_move_uci: "b5d6".to_string(),
                gap: CandidateGap::Centipawns { behind_best: 200 },
                variation: ["b5d6", "e8d7"].into_iter().map(str::to_string).collect(),
                provenance: provenance.clone(),
            },
            RankedAlternativeEvidence {
                rank: 3,
                root_move_uci: "b5a7".to_string(),
                gap: CandidateGap::Centipawns { behind_best: 300 },
                variation: ["b5a7", "e8d7"].into_iter().map(str::to_string).collect(),
                provenance: provenance.clone(),
            },
        ],
        player_move: PlayerMoveEvidence {
            root_move_uci: "e1e2".to_string(),
            evaluation: evaluation(0),
            retained_variation: vec!["e1e2".to_string()],
            provenance,
        },
    };
    let build = explain_decision(DecisionExplanationInput {
        game_ref: GameRef::try_from(format!("sha256:{}", "1".repeat(64)))?,
        critical_moment_id: CriticalMomentId::try_from("review-moment:fixture:fork".to_string())?,
        position_snapshot: build_position_snapshot(CANONICAL_DECISION_EXPLANATION_FEN, &[])?,
        classification: GameReviewMomentClassification::ImprovementOpportunity {
            correction: ImprovementCorrection {
                better_move_uci: "b5c7".to_string(),
                better_move_san: "Nxc7+".to_string(),
                outcome: ImprovementOutcome::ImprovedAnalyzed {
                    better_evaluation: evaluation(500),
                },
            },
        },
        provenance: GameReviewMomentProvenance::Automatic,
        player_move_uci: "e1e2".to_string(),
        candidate_evidence,
    })?;
    let DecisionExplanationBuild::Durable { explanation, .. } = build else {
        anyhow::bail!("canonical Decision Explanation fixture must be durable")
    };
    let proof = explanation
        .selected_paths
        .iter()
        .find_map(|path| path.candidate_generation_proof.as_ref())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "canonical Decision Explanation fixture must prove candidate generation"
            )
        })?;
    match &proof.position_goal {
        PositionGoal::GainMaterial { targets } => anyhow::ensure!(
            !targets.is_empty(),
            "canonical Decision Explanation fixture must name a material target"
        ),
    }
    Ok(*explanation)
}

fn event_fixture(
    request_id: &RequestId,
    operation_id: &OperationId,
    sequence: u32,
    event: ReviewSessionEvent,
) -> ReviewSessionEventEnvelope {
    ReviewSessionEventEnvelope {
        request_id: request_id.clone(),
        operation_id: operation_id.clone(),
        sequence,
        event,
    }
}

fn fixture_game_review(core: &ReviewSessionCoreContract) -> GameReview {
    let reviewed = &core.coach_turn_context.reviewed_move;
    let game_move = core
        .imported_game
        .game
        .moves
        .iter()
        .find(|game_move| game_move.ply == reviewed.ply)
        .expect("fixture reviewed move belongs to the imported Game");
    let evaluation = EngineEvaluation::Centipawns {
        value: 0,
        perspective: reviewed.side,
    };
    // The illustrative lines are taken from the canonical Game itself rather
    // than authored, so replacing the Game cannot leave the fixture describing
    // a move it no longer contains.
    let line_move = |ply: u16| {
        let played = core
            .imported_game
            .game
            .moves
            .iter()
            .find(|game_move| game_move.ply == ply)
            .expect("fixture lines stay inside the canonical Game");
        GameReviewLineMove {
            uci: played.uci.clone(),
            san: played.san.clone(),
        }
    };
    let destination_square = game_move
        .uci
        .get(2..4)
        .expect("a UCI move names its destination square")
        .to_string();
    let critical_moment = GameReviewCriticalMoment {
        critical_moment_id: reviewed.critical_moment_id.clone(),
        ply: reviewed.ply,
        move_number: game_move.move_number,
        side: reviewed.side,
        played_san: game_move.san.clone(),
        position_phase: PositionPhase {
            policy_version: PositionPhasePolicyVersion::V1,
            phase: PositionPhaseKind::Opening,
        },
        classification: GameReviewMomentClassification::PositiveHighlight {
            qualification: PositiveHighlightQualification {
                reasons: vec![PositiveHighlightQualificationReason::Objective {
                    reason: ObjectiveExcellenceReason::ExactBestMajorAchievement,
                }],
                achievements: vec![PositiveHighlightAchievement::AdvancedPassedPawn {
                    to_square: destination_square,
                }],
            },
            grade: PositiveHighlightGrade::Good,
        },
        provenance: GameReviewMomentProvenance::Automatic,
        category: GameReviewCriticalMomentCategory::Tactical,
        objective: GameReviewObjectiveComparison {
            best_move_uci: game_move.uci.clone(),
            played_move_uci: game_move.uci.clone(),
            best_evaluation: evaluation.clone(),
            played_evaluation: evaluation.clone(),
            centipawn_loss: Some(0),
            principal_variation: vec![game_move.uci.clone()],
            lines: Some(GameReviewObjectiveLines {
                best: vec![line_move(1), line_move(2), line_move(3)],
                // The opponent's reply is a quiet developing move, which in
                // this fixture position stands in front of nothing and takes
                // nothing, so it carries no effects. That is the common case
                // and the one worth showing: a quiet reply supports no claim
                // about it.
                refutation_effects: Vec::new(),
                // And the best line opens on the played move, which from the
                // starting position takes and hits nothing.
                best_move_effects: Vec::new(),
                refutation: vec![line_move(2), line_move(3), line_move(4)],
            }),
        },
        human: GameReviewHumanComparison {
            most_likely_move_uci: game_move.uci.clone(),
            most_likely_probability: Probability::try_from(0.5).unwrap(),
            played_move_probability: Some(Probability::try_from(0.5).unwrap()),
            played_move_rank: Some(1),
            played_move_is_human_likely: true,
        },
        effects: Vec::new(),
        residual_outcome: GameReviewResidualOutcome {
            standing_before: GameReviewAdvantageStanding::Balanced,
            standing_after: GameReviewAdvantageStanding::Balanced,
            classification: GameReviewResidualClassification::StandingKept,
        },
        played_move_outcome: PlayedMoveOutcomeEvidence::Analyzed {
            played_evaluation: evaluation.clone(),
            centipawn_loss: Some(0),
            residual_outcome: GameReviewResidualOutcome {
                standing_before: GameReviewAdvantageStanding::Balanced,
                standing_after: GameReviewAdvantageStanding::Balanced,
                classification: GameReviewResidualClassification::StandingKept,
            },
        },
        mechanism: None,
        teaching: GameReviewTeachingFacts {
            vocabulary_version: GameReviewTeachingVocabularyVersion::V1,
            themes: Vec::new(),
            opening_principles: Vec::new(),
        },
        decision_explanation_ref: None,
        decision_explanation: None,
        decision_learning_outcome: DecisionLearningOutcome::Abstained {
            reason: DecisionLearningAbstentionReason::NoProofValidConcept,
        },
        learning_material: ReviewMomentLearningMaterial::empty(),
        display: GameReviewMomentDisplay {
            played_annotation: Some("!".to_string()),
            best_evaluation: GameReviewEvaluationDisplay {
                score: "0.0".to_string(),
                label: "Roughly balanced".to_string(),
            },
            played_evaluation: GameReviewEvaluationDisplay {
                score: "0.0".to_string(),
                label: "Roughly balanced".to_string(),
            },
            loss_pawns: Some("0.0".to_string()),
        },
        comment: Some(critical_moment_comment_fixture(core, reviewed, game_move)),
    };
    GameReview {
        summary: "Fixture Game Review".to_string(),
        player_profile: GameReviewPlayerProfile {
            elo: core.imported_game.elo_profile.rating,
            level: GameReviewPlayerLevel::Intermediate,
            coaching_focus: "Review candidate moves before committing.".to_string(),
        },
        critical_moments: vec![critical_moment],
        position_views: vec![GameReviewPositionView {
            critical_moment_id: reviewed.critical_moment_id.clone(),
            ply: reviewed.ply,
            position_snapshot: core.position_snapshot.clone(),
            text_board: "fixture coordinate board".to_string(),
            evaluation: evaluation.clone(),
        }],
        evaluation_timeline: vec![GameReviewEvaluationPoint {
            ply: reviewed.ply,
            evaluation,
        }],
        learning_plan: LearningPlan::empty(),
    }
}

fn critical_moment_comment_fixture(
    _core: &ReviewSessionCoreContract,
    _reviewed: &ReviewedMoveAnchor,
    game_move: &CanonicalGameMove,
) -> CriticalMomentComment {
    let question = "My best guess is that you played this move to keep the center flexible. Was that your idea, or did you have another plan?".to_string();
    CriticalMomentComment {
        text: format!(
            "After {}, compare the reported 0.0 evaluation with 0.0. {question}",
            game_move.san
        ),
    }
}

fn brand_semantic_types(directory: &Path) -> anyhow::Result<()> {
    let paths = fs::read_dir(directory)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|extension| extension.to_str()) == Some("ts"))
        .collect::<Vec<_>>();
    let mut branded = 0;
    for path in paths {
        let Some(type_name) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        let source = fs::read_to_string(&path)?;
        for primitive in ["string", "number"] {
            let declaration = format!("export type {type_name} = {primitive};");
            if source.contains(&declaration) {
                let branded_declaration = format!(
                    "export type {type_name} = {primitive} & {{ readonly __brand: \"{type_name}\" }};"
                );
                fs::write(&path, source.replace(&declaration, &branded_declaration))?;
                branded += 1;
                break;
            }
        }
    }
    if branded == 0 {
        anyhow::bail!("ts-rs emitted no primitive aliases to brand")
    }
    Ok(())
}

fn assert_host_turn_prior_turns_match_v1(root: &Path) -> anyhow::Result<()> {
    let path = root.join("packages/shared-assets/limits.json");
    let limits: SharedLimits = serde_json::from_str(&fs::read_to_string(&path)?)?;
    let expected = ReviewSessionLimits::V1.max_host_turn_prior_turns;
    anyhow::ensure!(
        limits.host_turn_max_prior_turns == expected,
        "{} hostTurnMaxPriorTurns ({}) must equal ReviewSessionLimits::V1.max_host_turn_prior_turns ({expected})",
        path.display(),
        limits.host_turn_max_prior_turns
    );
    Ok(())
}

/// The authored TypeScript the contract ships beside its generated types.
///
/// Read from the directory rather than named one `include_str!` at a time, so
/// adding a template publishes it. Publishing is not exporting: `index.ts`
/// below and the re-export block at the foot of `client.ts` are still the
/// authored registry, so a new template reaches the package but reaches
/// consumers only once one of those names it.
///
/// Must run after `write_typescript_support` has scanned `directory` for
/// ts-rs stems, or the templates would be emitted as `export type` lines.
fn publish_contract_templates(directory: &Path) -> anyhow::Result<()> {
    let templates = Path::new(env!("CARGO_MANIFEST_DIR")).join("crates/contract/src/templates");
    let mut published = 0;
    for entry in fs::read_dir(&templates)? {
        let path = entry?.path();
        if path.extension().and_then(|end| end.to_str()) != Some("ts") {
            continue;
        }
        let name = path.file_name().expect("a read_dir entry has a file name");
        fs::write(directory.join(name), fs::read_to_string(&path)?)?;
        published += 1;
    }
    anyhow::ensure!(
        published > 0,
        "no contract templates found in {}",
        templates.display()
    );
    Ok(())
}

/// Publishes the accepted Chess.com Game URL forms as one TypeScript constant.
///
/// The kinds are authored once, in the contract crate. The web import field and
/// the Coach App tool schema read this constant rather than restating the
/// pattern, so `--check` fails the moment a new Game kind reaches the Engine
/// without reaching the surfaces that admit it. This is a value, not a wire
/// type: it is absent from `review-session.schema.json` and from the ts-rs
/// exports, because a provider's URL grammar is not a Game Review concept.
///
/// Must run after `write_typescript_support` has scanned `directory` for ts-rs
/// stems, for the same reason the templates do.
fn publish_chess_com_game_url_pattern(directory: &Path) -> anyhow::Result<()> {
    let pattern = serde_json::to_string(&chess_com_game_url_pattern())?;
    fs::write(
        directory.join("chess-com-url.ts"),
        format!(
            "// @generated by generate_review_session_contract; do not edit.\n\n\
// Every Chess.com shared Game URL form ChenChess imports, anchored.\n\
// Looser than the Engine on Game id range: the Engine stays the authority and\n\
// rejects what this admits with a typed reason.\n\
export const CHESS_COM_GAME_URL_PATTERN = {pattern};\n"
        ),
    )?;
    Ok(())
}

fn write_typescript_support(directory: &Path) -> anyhow::Result<()> {
    let mut type_names = fs::read_dir(directory)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|extension| extension.to_str()) == Some("ts"))
        .filter_map(|path| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .map(str::to_string)
        })
        .collect::<Vec<_>>();
    type_names.sort();
    let mut index =
        String::from("// @generated by generate_review_session_contract; do not edit.\n\n");
    for type_name in &type_names {
        index.push_str(&format!(
            "export type {{ {type_name} }} from \"./{type_name}.js\";\n"
        ));
    }
    index.push_str("export * from \"./chess-com-url.js\";\n");
    index.push_str("export * from \"./decoder.js\";\n");
    index.push_str("export * from \"./fixtures.js\";\n");
    index.push_str("export * from \"./client.js\";\n");
    index.push_str("export * from \"./construct.js\";\n");
    fs::write(directory.join("index.ts"), index)?;
    publish_contract_templates(directory)?;
    publish_chess_com_game_url_pattern(directory)?;
    fs::write(
        directory.join("fixtures.ts"),
        "// @generated by generate_review_session_contract; do not edit.\n\n\
import commands from \"../fixtures/commands.json\";\n\
import coreContract from \"../fixtures/core-contract.json\";\n\
import evidencePacket from \"../fixtures/evidence-packet.json\";\n\
import events from \"../fixtures/events.json\";\n\
import importedGame from \"../fixtures/imported-game.json\";\n\
import positionSnapshot from \"../fixtures/position-snapshot.json\";\n\n\
export { commands, coreContract, evidencePacket, events, importedGame, positionSnapshot };\n",
    )?;
    Ok(())
}

fn contract_readme() -> &'static str {
    include_str!("../../crates/contract/src/templates/README.md")
}

fn package_manifest() -> &'static str {
    r#"{
  "name": "@chenchess/coach-engine-sdk",
  "private": true,
  "license": "AGPL-3.0-or-later",
  "version": "0.2.0",
  "type": "module",
  "exports": {
    ".": "./src/index.ts",
    "./chess-com-url": "./src/chess-com-url.ts",
    "./contract-runtime": "./src/contract-runtime.ts",
    "./schema": "./src/review-session.schema.json"
  },
  "scripts": {
    "check": "tsc --project tsconfig.json",
    "test": "vitest run src/decoder.test.ts src/client.test.ts src/construct.test.ts"
  },
  "dependencies": {
    "chessops": "0.15.0",
    "valibot": "1.4.2"
  },
  "devDependencies": {
    "typescript": "^5.7.2",
    "vitest": "^2.1.8"
  }
}
"#
}

fn typescript_config() -> &'static str {
    r#"{
  "extends": "../../tsconfig.json",
  "compilerOptions": {
    "lib": ["ES2022", "DOM", "DOM.Iterable"]
  },
  "include": ["src"],
  "exclude": ["src/**/*.test.ts"]
}
"#
}

fn write_fixture(path: &Path, value: &impl Serialize) -> anyhow::Result<()> {
    fs::write(path, pretty_json(value)?)?;
    Ok(())
}

fn pretty_json(value: &impl Serialize) -> anyhow::Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}
