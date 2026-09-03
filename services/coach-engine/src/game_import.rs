use std::fs;

use sha2::{Digest, Sha256};

use crate::{
    chess_com::{
        fetch_contract_version, ChessComGameClient, ChessComGameUrl, ChessComUrlError,
        ReqwestChessComGameClient, CHESS_COM_PUBAPI_ARCHIVE_CONTRACT_VERSION,
    },
    chess_com_import::{
        prepare_chess_com_archive_game, ChessComImportError, ChessComImportGateway,
        PreparedChessComGame,
    },
    game_eligibility::{completed_standard_outcome, GameEligibilityError},
    lichess::{
        LichessExportClient, LichessExportResponse, LichessGameUrl, LichessSide, LichessUrlError,
        LICHESS_EXPORT_CONTRACT_VERSION,
    },
    lichess_import::{
        prepare_lichess_game, LichessImportError, LichessImportGateway, PreparedLichessGame,
    },
    opening_identification::identify_opening,
    pgn::{parse_pgn_with_metadata, ParsedPgn, PgnImportError},
    profile_game_feed::{DailyGameInputSource, DailyGameReviewRequest},
    review_session_contract::*,
    types::{Game, MoveSide},
};

pub struct GameImporter<C, H = ReqwestChessComGameClient> {
    lichess: LichessImportGateway<C>,
    chess_com: ChessComImportGateway<H>,
}

pub(crate) struct ReviewImport {
    pub(crate) imported_game: ImportedGame,
    pub(crate) pgn: String,
}

impl<C> GameImporter<C, ReqwestChessComGameClient> {
    pub fn new(lichess: C) -> Self {
        Self::with_chess_com(lichess, ReqwestChessComGameClient)
    }
}

impl<C, H> GameImporter<C, H> {
    pub fn with_chess_com(lichess: C, chess_com: H) -> Self {
        Self {
            lichess: LichessImportGateway::new(lichess),
            chess_com: ChessComImportGateway::new(chess_com),
        }
    }
}

impl<C, H> GameImporter<C, H>
where
    C: LichessExportClient + 'static,
    H: ChessComGameClient,
{
    pub async fn import(
        &self,
        source: &GameInputSource,
        requested_review_side: RequestedReviewSide,
        requested_elo: &RequestedEloProfile,
    ) -> Result<ImportedGame, GameImportError> {
        self.import_with_progress(source, requested_review_side, requested_elo, |_| {})
            .await
    }

    pub async fn import_with_progress<F>(
        &self,
        source: &GameInputSource,
        requested_review_side: RequestedReviewSide,
        requested_elo: &RequestedEloProfile,
        progress: F,
    ) -> Result<ImportedGame, GameImportError>
    where
        F: Fn(ImportProgressStage),
    {
        Ok(self
            .import_review_with_progress(source, requested_review_side, requested_elo, progress)
            .await?
            .imported_game)
    }

    pub(crate) async fn import_review_with_progress<F>(
        &self,
        source: &GameInputSource,
        requested_review_side: RequestedReviewSide,
        requested_elo: &RequestedEloProfile,
        progress: F,
    ) -> Result<ReviewImport, GameImportError>
    where
        F: Fn(ImportProgressStage),
    {
        progress(ImportProgressStage::ValidatingSource);
        match source {
            GameInputSource::LichessUrl { url } => {
                let source = LichessGameUrl::parse(url)?;
                let review_side = resolve_lichess_review_side(&source, requested_review_side)?;
                let prepared = self.lichess.import(&source, &progress).await?;
                progress(ImportProgressStage::BuildingSnapshot);
                let pgn = std::str::from_utf8(&prepared.pgn)?.to_string();
                let imported_game = build_prepared_lichess_game(
                    &source,
                    (*prepared).clone(),
                    review_side,
                    requested_elo,
                )?;
                Ok(ReviewImport { imported_game, pgn })
            }
            GameInputSource::ChessComUrl { url } => {
                let source = ChessComGameUrl::parse(url)?;
                let review_side = resolve_unqualified_url_review_side(requested_review_side)?;
                let prepared = self.chess_com.import(&source, &progress).await?;
                progress(ImportProgressStage::BuildingSnapshot);
                let pgn = std::str::from_utf8(&prepared.pgn)?.to_string();
                let imported_game = build_prepared_chess_com_game(
                    &source,
                    (*prepared).clone(),
                    review_side,
                    requested_elo,
                    fetch_contract_version(&source),
                )?;
                Ok(ReviewImport { imported_game, pgn })
            }
            GameInputSource::PastedPgn { pgn } => {
                progress(ImportProgressStage::ValidatingGame);
                let imported_game = build_pgn_game_import(
                    pgn.as_bytes(),
                    resolve_local_review_side(requested_review_side)?,
                    requested_elo,
                    LocalPgnKind::Pasted,
                    || progress(ImportProgressStage::BuildingSnapshot),
                )?;
                Ok(ReviewImport {
                    imported_game,
                    pgn: pgn.clone(),
                })
            }
            GameInputSource::LocalPgnFile { path } => {
                let pgn = fs::read(path).map_err(|source| GameImportError::LocalPgnRead {
                    path: path.clone(),
                    source,
                })?;
                progress(ImportProgressStage::ValidatingGame);
                let imported_game = build_pgn_game_import(
                    &pgn,
                    resolve_local_review_side(requested_review_side)?,
                    requested_elo,
                    LocalPgnKind::File,
                    || progress(ImportProgressStage::BuildingSnapshot),
                )?;
                Ok(ReviewImport {
                    imported_game,
                    pgn: std::str::from_utf8(&pgn)?.to_string(),
                })
            }
        }
    }

    pub(crate) async fn import_daily_review_with_progress<F>(
        &self,
        request: &DailyGameReviewRequest,
        progress: F,
    ) -> Result<ReviewImport, GameImportError>
    where
        F: Fn(ImportProgressStage),
    {
        match &request.source {
            DailyGameInputSource::LichessUrl { url } => {
                self.import_review_with_progress(
                    &GameInputSource::LichessUrl { url: url.clone() },
                    request.review_side,
                    &request.elo_profile,
                    progress,
                )
                .await
            }
            DailyGameInputSource::ChessComArchive {
                url,
                pgn,
                captured_at,
                response_digest,
            } => {
                progress(ImportProgressStage::ValidatingSource);
                let source = ChessComGameUrl::parse(url)?;
                let review_side = resolve_unqualified_url_review_side(request.review_side)?;
                progress(ImportProgressStage::ValidatingGame);
                let prepared = prepare_chess_com_archive_game(
                    pgn.clone(),
                    *captured_at,
                    response_digest.clone(),
                )?;
                progress(ImportProgressStage::BuildingSnapshot);
                let pgn = std::str::from_utf8(&prepared.pgn)?.to_string();
                let imported_game = build_prepared_chess_com_game(
                    &source,
                    prepared,
                    review_side,
                    &request.elo_profile,
                    CHESS_COM_PUBAPI_ARCHIVE_CONTRACT_VERSION,
                )?;
                Ok(ReviewImport { imported_game, pgn })
            }
        }
    }
}

/// Local source validation that precedes Game Import admission.
///
/// A parsed URL or PGN is enough to start import work. Provider fetch,
/// eligibility, and persistence happen after the import window is charged.
pub(crate) fn validate_import_boundary(
    source: &GameInputSource,
    requested_review_side: RequestedReviewSide,
) -> Result<(), GameImportError> {
    match source {
        GameInputSource::LichessUrl { url } => {
            let source = LichessGameUrl::parse(url)?;
            resolve_lichess_review_side(&source, requested_review_side)?;
            Ok(())
        }
        GameInputSource::ChessComUrl { url } => {
            ChessComGameUrl::parse(url)?;
            resolve_unqualified_url_review_side(requested_review_side)?;
            Ok(())
        }
        GameInputSource::PastedPgn { pgn } => {
            require_pgn_size(pgn.len())?;
            parse_pgn_with_metadata(pgn)?;
            resolve_local_review_side(requested_review_side)?;
            Ok(())
        }
        GameInputSource::LocalPgnFile { path } => {
            let pgn = fs::read(path).map_err(|source| GameImportError::LocalPgnRead {
                path: path.clone(),
                source,
            })?;
            require_pgn_size(pgn.len())?;
            parse_pgn_with_metadata(std::str::from_utf8(&pgn)?)?;
            resolve_local_review_side(requested_review_side)?;
            Ok(())
        }
    }
}

fn build_prepared_chess_com_game(
    source: &ChessComGameUrl,
    prepared: PreparedChessComGame,
    review_side: ReviewSide,
    requested_elo: &RequestedEloProfile,
    fetch_contract_version: &str,
) -> Result<ImportedGame, GameImportError> {
    let provenance = ImportProvenance::ChessCom {
        canonical_game_id: CanonicalGameId::try_from(source.canonical_game_id().to_string())?,
        canonical_url: source.canonical_url(),
        fetch_contract_version: fetch_contract_version.to_string(),
        captured_at: prepared
            .captured_at
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        response_digest: prepared.response_digest,
        pgn_digest: prepared.pgn_digest,
    };
    build_imported_game(
        prepared.parsed,
        &prepared.pgn,
        review_side,
        requested_elo,
        provenance,
        prepared.outcome,
    )
}

pub fn build_lichess_imported_game(
    source: &LichessGameUrl,
    response: LichessExportResponse,
    review_side: ReviewSide,
    requested_elo: &RequestedEloProfile,
) -> Result<ImportedGame, GameImportError> {
    let prepared = prepare_lichess_game(source, response)?;
    build_prepared_lichess_game(source, prepared, review_side, requested_elo)
}

fn build_prepared_lichess_game(
    source: &LichessGameUrl,
    prepared: PreparedLichessGame,
    review_side: ReviewSide,
    requested_elo: &RequestedEloProfile,
) -> Result<ImportedGame, GameImportError> {
    let side = lichess_side(review_side).ok_or(GameImportError::ReviewSideRequired)?;
    let canonical_url = source.canonical_url();
    let provenance = ImportProvenance::Lichess {
        canonical_game_id: CanonicalGameId::try_from(source.canonical_game_id().to_string())?,
        side_qualified_url: source.side_qualified_url(side),
        canonical_url,
        export_contract_version: LICHESS_EXPORT_CONTRACT_VERSION.to_string(),
        captured_at: prepared
            .captured_at
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        response_digest: prepared.response_digest,
        pgn_digest: prepared.pgn_digest,
    };
    build_imported_game(
        prepared.parsed,
        &prepared.pgn,
        review_side,
        requested_elo,
        provenance,
        prepared.outcome,
    )
}

fn build_pgn_game_import<F>(
    bytes: &[u8],
    review_side: ReviewSide,
    requested_elo: &RequestedEloProfile,
    source: LocalPgnKind,
    on_validated: F,
) -> Result<ImportedGame, GameImportError>
where
    F: FnOnce(),
{
    require_pgn_size(bytes.len())?;
    let pgn = std::str::from_utf8(bytes)?;
    let mut parsed = parse_pgn_with_metadata(pgn)?;
    let outcome = completed_standard_outcome(&parsed)?;
    let pgn_digest = artifact_digest(bytes)?;
    let provenance = match source {
        LocalPgnKind::Pasted => {
            discard_identity_bearing_headers(&mut parsed);
            ImportProvenance::PastedPgn { pgn_digest }
        }
        LocalPgnKind::File => ImportProvenance::LocalPgn { pgn_digest },
    };
    on_validated();
    build_imported_game(
        parsed,
        bytes,
        review_side,
        requested_elo,
        provenance,
        outcome,
    )
}

fn discard_identity_bearing_headers(parsed: &mut ParsedPgn) {
    parsed.game.white = None;
    parsed.game.black = None;
    parsed.game.event = None;
    parsed.game.site = None;
}

fn build_imported_game(
    parsed: ParsedPgn,
    pgn_bytes: &[u8],
    review_side: ReviewSide,
    requested_elo: &RequestedEloProfile,
    provenance: ImportProvenance,
    outcome: CompletedGameOutcome,
) -> Result<ImportedGame, GameImportError> {
    let white_rating = rating_metadata(parsed.metadata.white_elo.as_deref());
    let black_rating = rating_metadata(parsed.metadata.black_elo.as_deref());
    let elo_profile =
        resolve_elo_profile(requested_elo, review_side, &white_rating, &black_rating)?;
    let opening = identify_opening(&parsed.metadata, &provenance, &parsed.game);
    let game = canonical_game(
        parsed.game,
        pgn_bytes,
        white_rating,
        black_rating,
        opening,
        outcome,
    )?;
    Ok(ImportedGame {
        game,
        review_side,
        elo_profile,
        provenance,
    })
}

fn canonical_game(
    game: Game,
    pgn_bytes: &[u8],
    white_rating: RatingMetadata,
    black_rating: RatingMetadata,
    opening: OpeningMetadata,
    outcome: CompletedGameOutcome,
) -> Result<CanonicalCompletedGame, GameImportError> {
    let mut history = Vec::with_capacity(game.moves.len());
    let mut position_refs = Vec::with_capacity(game.moves.len() + 1);
    for game_move in &game.moves {
        position_refs.push(
            build_position_snapshot(&game_move.position, &history)
                .map_err(|_| GameImportError::InvalidGame("invalid pre-move Position"))?
                .position_ref,
        );
        history.push(game_move.position.as_str());
    }
    position_refs.push(
        build_position_snapshot(&game.final_position, &history)
            .map_err(|_| GameImportError::InvalidGame("invalid final Position"))?
            .position_ref,
    );
    let moves = game
        .moves
        .iter()
        .zip(position_refs.windows(2))
        .map(|(game_move, positions)| {
            Ok(CanonicalGameMove {
                ply: u16::try_from(game_move.ply)
                    .map_err(|_| GameImportError::InvalidGame("Game exceeds v1 ply limits"))?,
                move_number: u16::try_from(game_move.move_number).map_err(|_| {
                    GameImportError::InvalidGame("Game exceeds v1 move-number limits")
                })?,
                side: move_color(game_move.side),
                san: game_move.san.clone(),
                uci: game_move.uci.clone(),
                before_position_ref: positions[0].clone(),
                after_position_ref: positions[1].clone(),
            })
        })
        .collect::<Result<Vec<_>, GameImportError>>()?;
    let final_position_ref = position_refs
        .last()
        .expect("completed Game has at least one move")
        .clone();
    Ok(CanonicalCompletedGame {
        game_ref: GameRef::try_from(sha256(pgn_bytes))?,
        white: PlayerMetadata {
            name: metadata_text(game.white),
            rating: white_rating,
        },
        black: PlayerMetadata {
            name: metadata_text(game.black),
            rating: black_rating,
        },
        event: metadata_text(game.event),
        site: metadata_text(game.site),
        opening,
        outcome,
        moves,
        final_position_ref,
    })
}

fn resolve_lichess_review_side(
    source: &LichessGameUrl,
    requested: RequestedReviewSide,
) -> Result<ReviewSide, GameImportError> {
    match (source.side(), requested) {
        (Some(side), RequestedReviewSide::FromQualifiedUrl) => Ok(review_side(side)),
        (
            Some(_),
            RequestedReviewSide::Selected {
                review_side: selected @ (ReviewSide::White | ReviewSide::Black),
            },
        ) => Ok(selected),
        (
            None,
            RequestedReviewSide::Selected {
                review_side: ReviewSide::White,
            },
        ) => Ok(ReviewSide::White),
        (
            None,
            RequestedReviewSide::Selected {
                review_side: ReviewSide::Black,
            },
        ) => Ok(ReviewSide::Black),
        _ => Err(GameImportError::ReviewSideRequired),
    }
}

fn resolve_local_review_side(
    requested: RequestedReviewSide,
) -> Result<ReviewSide, GameImportError> {
    match requested {
        RequestedReviewSide::Selected { review_side } => Ok(review_side),
        RequestedReviewSide::FromQualifiedUrl | RequestedReviewSide::Required => {
            Err(GameImportError::ReviewSideRequired)
        }
    }
}

fn resolve_unqualified_url_review_side(
    requested: RequestedReviewSide,
) -> Result<ReviewSide, GameImportError> {
    match requested {
        RequestedReviewSide::Selected {
            review_side: side @ (ReviewSide::White | ReviewSide::Black),
        } => Ok(side),
        RequestedReviewSide::Selected {
            review_side: ReviewSide::Both,
        }
        | RequestedReviewSide::FromQualifiedUrl
        | RequestedReviewSide::Required => Err(GameImportError::ReviewSideRequired),
    }
}

fn resolve_elo_profile(
    requested: &RequestedEloProfile,
    review_side: ReviewSide,
    white: &RatingMetadata,
    black: &RatingMetadata,
) -> Result<ResolvedEloProfile, GameImportError> {
    match requested {
        RequestedEloProfile::PlayerProvided { rating } => Ok(ResolvedEloProfile {
            rating: *rating,
            source: EloSource::PlayerProvided,
        }),
        RequestedEloProfile::FromImportedMetadata => {
            let (rating, review_side) = match review_side {
                ReviewSide::White => (white, Color::White),
                ReviewSide::Black => (black, Color::Black),
                ReviewSide::Both => return Err(GameImportError::EloProfileRequired),
            };
            let RatingMetadata::Present { rating } = rating else {
                return Err(GameImportError::EloProfileRequired);
            };
            Ok(ResolvedEloProfile {
                rating: *rating,
                source: EloSource::ImportedMetadata { review_side },
            })
        }
    }
}

fn rating_metadata(value: Option<&str>) -> RatingMetadata {
    value
        .and_then(|value| value.parse::<u16>().ok())
        .and_then(|value| EloRating::try_from(value).ok())
        .map_or(RatingMetadata::Absent, |rating| RatingMetadata::Present {
            rating,
        })
}

fn metadata_text(value: Option<String>) -> MetadataText {
    value.map_or(MetadataText::Absent, |value| MetadataText::Present {
        value,
    })
}

fn require_pgn_size(bytes: usize) -> Result<(), GameImportError> {
    if bytes <= usize::try_from(ReviewSessionLimits::V1.max_pgn_bytes).unwrap() {
        Ok(())
    } else {
        Err(GameImportError::PgnTooLarge)
    }
}

pub(crate) fn artifact_digest(bytes: &[u8]) -> Result<ArtifactDigest, ContractValueError> {
    ArtifactDigest::try_from(sha256(bytes))
}

fn sha256(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn move_color(side: MoveSide) -> Color {
    match side {
        MoveSide::White => Color::White,
        MoveSide::Black => Color::Black,
    }
}

fn review_side(side: LichessSide) -> ReviewSide {
    match side {
        LichessSide::White => ReviewSide::White,
        LichessSide::Black => ReviewSide::Black,
    }
}

fn lichess_side(side: ReviewSide) -> Option<LichessSide> {
    match side {
        ReviewSide::White => Some(LichessSide::White),
        ReviewSide::Black => Some(LichessSide::Black),
        ReviewSide::Both => None,
    }
}

#[derive(Debug, Clone, Copy)]
enum LocalPgnKind {
    Pasted,
    File,
}

#[derive(Debug, thiserror::Error)]
pub enum GameImportError {
    #[error("invalid Lichess Game URL")]
    InvalidLichessUrl,
    #[error("invalid Chess.com shared Game URL")]
    InvalidChessComUrl,
    #[error("select White or Black before importing this Game")]
    ReviewSideRequired,
    #[error("provide a supported Elo Profile before importing this Game")]
    EloProfileRequired,
    #[error("PGN exceeds the v1 import limit")]
    PgnTooLarge,
    #[error("could not read local PGN {path}: {source}")]
    LocalPgnRead {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Lichess(#[from] LichessImportError),
    #[error(transparent)]
    ChessCom(#[from] ChessComImportError),
    #[error("invalid PGN encoding: {0}")]
    PgnEncoding(#[from] std::str::Utf8Error),
    #[error(transparent)]
    Pgn(#[from] PgnImportError),
    #[error("Game is ongoing")]
    OngoingGame,
    #[error("Game was aborted")]
    AbortedGame,
    #[error("Game uses an unsupported variant")]
    UnsupportedVariant,
    #[error("invalid Game import: {0}")]
    InvalidGame(&'static str),
    #[error(transparent)]
    Contract(#[from] ContractValueError),
}

impl From<LichessUrlError> for GameImportError {
    fn from(_: LichessUrlError) -> Self {
        Self::InvalidLichessUrl
    }
}

impl From<ChessComUrlError> for GameImportError {
    fn from(_: ChessComUrlError) -> Self {
        Self::InvalidChessComUrl
    }
}

impl From<GameEligibilityError> for GameImportError {
    fn from(error: GameEligibilityError) -> Self {
        match error {
            GameEligibilityError::UnsupportedVariant => Self::UnsupportedVariant,
            GameEligibilityError::Ongoing => Self::OngoingGame,
            GameEligibilityError::Aborted => Self::AbortedGame,
            GameEligibilityError::Invalid(message) => Self::InvalidGame(message),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GameImportTerminal {
    event: ReviewSessionEvent,
    player_message: String,
}

impl GameImportTerminal {
    pub fn event(&self) -> &ReviewSessionEvent {
        &self.event
    }

    pub fn player_message(&self) -> &str {
        &self.player_message
    }
}

impl GameImportError {
    pub fn terminal(&self) -> GameImportTerminal {
        const NOT_REVIEWABLE: &str =
            "This link must point to one public, completed standard-chess Game.";

        match self {
            Self::InvalidLichessUrl => rejected(
                CommandRejectionReason::InvalidLichessUrl,
                RejectionRecovery::CorrectInput,
                NOT_REVIEWABLE,
            ),
            Self::InvalidChessComUrl => rejected(
                CommandRejectionReason::InvalidChessComUrl,
                RejectionRecovery::CorrectInput,
                NOT_REVIEWABLE,
            ),
            Self::ReviewSideRequired => rejected(
                CommandRejectionReason::ReviewSideRequired,
                RejectionRecovery::SelectReviewSide,
                "Select White or Black before importing this Game.",
            ),
            Self::EloProfileRequired => rejected(
                CommandRejectionReason::EloProfileRequired,
                RejectionRecovery::ProvideEloProfile,
                "Provide a supported Elo Profile before importing this Game.",
            ),
            Self::PgnTooLarge
            | Self::Lichess(LichessImportError::ResponseTooLarge)
            | Self::ChessCom(ChessComImportError::ResponseTooLarge) => rejected(
                CommandRejectionReason::ResponseTooLarge,
                RejectionRecovery::CorrectInput,
                NOT_REVIEWABLE,
            ),
            Self::LocalPgnRead { .. }
            | Self::PgnEncoding(_)
            | Self::Pgn(_)
            | Self::InvalidGame(_)
            | Self::Lichess(LichessImportError::InvalidPgn)
            | Self::ChessCom(ChessComImportError::InvalidPgn) => rejected(
                CommandRejectionReason::InvalidPgn,
                RejectionRecovery::CorrectInput,
                NOT_REVIEWABLE,
            ),
            Self::OngoingGame
            | Self::Lichess(LichessImportError::OngoingGame)
            | Self::ChessCom(ChessComImportError::OngoingGame) => rejected(
                CommandRejectionReason::OngoingGame,
                RejectionRecovery::None,
                NOT_REVIEWABLE,
            ),
            Self::AbortedGame
            | Self::Lichess(LichessImportError::AbortedGame)
            | Self::ChessCom(ChessComImportError::AbortedGame) => rejected(
                CommandRejectionReason::AbortedGame,
                RejectionRecovery::None,
                NOT_REVIEWABLE,
            ),
            Self::UnsupportedVariant
            | Self::Lichess(LichessImportError::UnsupportedVariant)
            | Self::ChessCom(ChessComImportError::UnsupportedVariant) => rejected(
                CommandRejectionReason::UnsupportedVariant,
                RejectionRecovery::CorrectInput,
                NOT_REVIEWABLE,
            ),
            Self::Lichess(LichessImportError::GameNotFound) => rejected(
                CommandRejectionReason::GameNotFound,
                RejectionRecovery::CorrectInput,
                NOT_REVIEWABLE,
            ),
            Self::Lichess(LichessImportError::PrivateGame) => rejected(
                CommandRejectionReason::PrivateGame,
                RejectionRecovery::CorrectInput,
                NOT_REVIEWABLE,
            ),
            Self::Lichess(LichessImportError::MalformedResponse) => rejected(
                CommandRejectionReason::MalformedProviderResponse,
                RejectionRecovery::CorrectInput,
                NOT_REVIEWABLE,
            ),
            Self::ChessCom(ChessComImportError::GameNotFound) => rejected(
                CommandRejectionReason::GameNotFound,
                RejectionRecovery::CorrectInput,
                NOT_REVIEWABLE,
            ),
            Self::ChessCom(ChessComImportError::PrivateGame) => rejected(
                CommandRejectionReason::PrivateGame,
                RejectionRecovery::CorrectInput,
                NOT_REVIEWABLE,
            ),
            Self::ChessCom(ChessComImportError::MalformedResponse) => rejected(
                CommandRejectionReason::MalformedProviderResponse,
                RejectionRecovery::CorrectInput,
                NOT_REVIEWABLE,
            ),
            Self::Lichess(LichessImportError::Transport) => unavailable(
                ProviderUnavailableReason::LichessTransport,
                RetryDirective::RetryAllowed,
                "Lichess is unavailable right now. Try this Game again.",
            ),
            Self::Lichess(LichessImportError::Timeout) => unavailable(
                ProviderUnavailableReason::Timeout {
                    provider: ProviderKind::Lichess,
                },
                RetryDirective::RetryAllowed,
                "Lichess did not respond in time. Try this Game again.",
            ),
            Self::Lichess(LichessImportError::RateLimited {
                retry_after_seconds,
                retry_at,
            }) => unavailable(
                ProviderUnavailableReason::RateLimited {
                    retry_after_seconds: *retry_after_seconds,
                },
                RetryDirective::RetryAfter {
                    seconds: *retry_after_seconds,
                },
                format!(
                    "Lichess asked us to slow down. Try this Game again after {}.",
                    retry_at.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true)
                ),
            ),
            Self::ChessCom(ChessComImportError::Transport) => unavailable(
                ProviderUnavailableReason::ChessComTransport,
                RetryDirective::RetryAllowed,
                "Chess.com is unavailable right now. Try this Game again.",
            ),
            Self::ChessCom(ChessComImportError::Timeout) => unavailable(
                ProviderUnavailableReason::Timeout {
                    provider: ProviderKind::ChessCom,
                },
                RetryDirective::RetryAllowed,
                "Chess.com did not respond in time. Try this Game again.",
            ),
            Self::ChessCom(ChessComImportError::RateLimited {
                retry_after_seconds,
                retry_at,
            }) => unavailable(
                ProviderUnavailableReason::RateLimited {
                    retry_after_seconds: *retry_after_seconds,
                },
                RetryDirective::RetryAfter {
                    seconds: *retry_after_seconds,
                },
                format!(
                    "Chess.com asked us to slow down. Try this Game again after {}.",
                    retry_at.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true)
                ),
            ),
            Self::Contract(_) => rejected(
                CommandRejectionReason::InvalidCommand,
                RejectionRecovery::CorrectInput,
                "The Game import request is invalid.",
            ),
        }
    }
}

fn rejected(
    reason: CommandRejectionReason,
    recovery: RejectionRecovery,
    player_message: impl Into<String>,
) -> GameImportTerminal {
    GameImportTerminal {
        event: ReviewSessionEvent::Rejected {
            operation: OperationKind::GameImport,
            reason,
            recovery,
        },
        player_message: player_message.into(),
    }
}

fn unavailable(
    reason: ProviderUnavailableReason,
    retry: RetryDirective,
    player_message: impl Into<String>,
) -> GameImportTerminal {
    GameImportTerminal {
        event: ReviewSessionEvent::Unavailable {
            operation: OperationKind::GameImport,
            reason,
            retry,
        },
        player_message: player_message.into(),
    }
}

#[cfg(test)]
#[path = "game_import_tests.rs"]
mod tests;
