use std::{collections::HashSet, fs, path::Path};

use chen_chess_coach_engine::{
    pgn::parse_pgn, pipeline_evaluation::run_fast_evaluation, shared_assets::canonical_game_dir,
};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use shakmaty::{fen::Fen, uci::UciMove, CastlingMode, Chess, EnPassantMode, Position};

const GAME_ID: &str = "Synthet1";
const WIRE_SHA256: &str = "0d104931a04c38c414fa7ba6a2c544ac2d67f70fd3c0acb33ad8074a3c274bd0";
const NORMALIZED_SHA256: &str = "7eb4b0803c2b4fca8d80b3968928fe856bf15999626a402d9651694c0e80c799";
const PROVIDER_CASE_SHA256: &str =
    "47777081e8b3363e3ae12196173fb3515dee75fdbeacda8e3d8d507a45b4187a";
const PROVIDER_BASELINE_SHA256: &str =
    "f0732b9b546717977d09bf15020d00b64573eb7fd26aecfd4c77cc16d8221664";
const STOCKFISH_ARCHIVE_SHA256: &str =
    "4d77c4aa3ad9bd1ea8111f2ac5a4620fe7ebf998d6893bf828d49ccd579c8cb0";
const STOCKFISH_BINARY_SHA256: &str =
    "bc0cac905ecdf2147fe22055c733bcd999b1e3f7c399fbaf7fb9055786563590";
const MAIA_IMAGE: &str =
    "maia-runtime@sha256:ab3b6dc16b75c3602f2e6c4002dc0f99ef77c8c042641cffea66fc1c23482972";
const MAIA_MODEL_SHA256: &str = "65aae8465eed5e65df66a24ea7370715579f9e5435098d06fe18bdb1e267e997";
const MAIA_CONFIG_SHA256: &str = "4b06a5e6917dba8a55defaf3947ce97a73edca3ae2c9d225779a620353c1371b";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaptureMetadata {
    game_id: String,
    source: SourceCapture,
    normalized_pgn: NormalizedPgn,
    expected_game: ExpectedGame,
    provider_recording: ProviderRecording,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SourceCapture {
    side_qualified_url: String,
    canonical_url: String,
    export_request: ExportRequest,
    captured_at: String,
    response: HttpResponseCapture,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportRequest {
    url: String,
    accept: String,
    followed_redirects: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HttpResponseCapture {
    #[serde(flatten)]
    artifact: FileArtifact,
    content_type: String,
}

#[derive(Deserialize)]
struct NormalizedPgn {
    #[serde(flatten)]
    artifact: FileArtifact,
    normalization: String,
}

#[derive(Deserialize)]
struct FileArtifact {
    path: String,
    bytes: usize,
    sha256: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExpectedGame {
    variant: String,
    speed: String,
    time_control: String,
    review_side: String,
    player_elo: u16,
    plies: usize,
    result: String,
    termination: String,
    opening: Opening,
}

#[derive(Deserialize)]
struct Opening {
    eco: String,
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderRecording {
    captured_at: String,
    case: FileArtifact,
    baseline: FileArtifact,
    runtime: RuntimeProvenance,
    multi_pv_comparison: MultiPvComparison,
}

/// The comparison searches were recorded after the provider capture, from Stockfish
/// alone: a candidate comparison never consults the Human Move Model, so rerunning Maia
/// would have cost a provider run and risked drift for evidence the comparison ignores.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MultiPvComparison {
    requested_variations: u8,
    compared_moments: usize,
    authoritative_root_disagreements: usize,
    unit_version: String,
    stockfish_binary_sha256: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeProvenance {
    unit_version: String,
    stockfish: StockfishProvenance,
    maia: MaiaProvenance,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StockfishProvenance {
    version: String,
    depth: u8,
    threads: u8,
    hash_mib: u16,
    archive_sha256: String,
    binary_sha256: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MaiaProvenance {
    image: String,
    package: String,
    model: String,
    model_sha256: String,
    config_sha256: String,
    player_elo: u16,
    opponent_elo: u16,
    zero_probability_candidates: usize,
}

#[test]
fn canonical_game_and_provider_recording_are_self_consistent() {
    let fixture = fixture_root();
    let metadata: CaptureMetadata = read_json(&fixture.join("capture.json"));

    assert_capture_contract(&metadata);

    let wire_bytes = read_verified_artifact(&fixture, &metadata.source.response.artifact);
    let pgn_bytes = read_verified_artifact(&fixture, &metadata.normalized_pgn.artifact);
    assert_eq!(wire_bytes.strip_suffix(b"\n"), Some(pgn_bytes.as_slice()));
    let pgn = std::str::from_utf8(&pgn_bytes).expect("canonical PGN should be UTF-8");
    assert_minimal_export(pgn, &metadata.expected_game);

    let game = parse_pgn(pgn).expect("canonical PGN should parse as one legal Game");
    assert_eq!(game.moves.len(), metadata.expected_game.plies);
    assert_eq!(
        game.result.as_deref(),
        Some(metadata.expected_game.result.as_str())
    );
    assert_legal_game_positions(&game);

    let measurement_pgn = fs::read_to_string(fixture.join("game-66ply.pgn"))
        .expect("66-ply measurement PGN should be readable");
    let measurement_game =
        parse_pgn(&measurement_pgn).expect("66-ply measurement PGN should be legal");
    assert_eq!(measurement_game.moves, game.moves[..66]);
    assert_eq!(measurement_game.final_position, game.moves[66].position);
    assert_eq!(measurement_game.result.as_deref(), Some("0-1"));
    assert!(!measurement_game.is_terminal);

    let provider_case_path = artifact_path(&fixture, &metadata.provider_recording.case);
    let provider_case_bytes = read_verified_artifact(&fixture, &metadata.provider_recording.case);
    let provider_case: Value =
        serde_json::from_slice(&provider_case_bytes).expect("provider case should be JSON");
    assert_provider_recording(&provider_case, pgn, &game, &metadata);

    read_verified_artifact(&fixture, &metadata.provider_recording.baseline);

    let report = run_fast_evaluation(
        provider_case_path
            .parent()
            .expect("provider case should have a directory"),
    )
    .expect("recorded provider evidence should evaluate");
    assert_eq!(report.evaluated_cases, 1);
    assert!(report.differences.is_empty());
}

fn assert_capture_contract(metadata: &CaptureMetadata) {
    assert_eq!(metadata.game_id, GAME_ID);
    assert_eq!(
        metadata.source.side_qualified_url,
        "https://lichess.org/Synthet1Demo/black"
    );
    assert_eq!(
        metadata.source.canonical_url,
        format!("https://lichess.org/{GAME_ID}")
    );
    assert_eq!(
        metadata.source.export_request.url,
        "https://lichess.org/game/export/Synthet1?clocks=false&evals=false&accuracy=false&literate=false&opening=true"
    );
    assert_eq!(
        metadata.source.export_request.accept,
        "application/x-chess-pgn"
    );
    assert!(!metadata.source.export_request.followed_redirects);
    assert_eq!(metadata.source.captured_at, "2026-09-03T00:00:00Z");
    assert_eq!(
        metadata.source.response.content_type,
        "application/x-chess-pgn"
    );
    assert_eq!(
        metadata.source.response.artifact.path,
        "lichess-export.raw.pgn"
    );
    assert_eq!(metadata.source.response.artifact.bytes, 912);
    assert_eq!(metadata.source.response.artifact.sha256, WIRE_SHA256);
    assert_eq!(metadata.normalized_pgn.artifact.path, "lichess-export.pgn");
    assert_eq!(metadata.normalized_pgn.artifact.bytes, 911);
    assert_eq!(metadata.normalized_pgn.artifact.sha256, NORMALIZED_SHA256);
    assert_eq!(
        metadata.normalized_pgn.normalization,
        "Removed one trailing blank line from the HTTP response."
    );
    assert_eq!(
        metadata.provider_recording.captured_at,
        "2026-09-03T00:00:00Z"
    );
    assert_eq!(
        metadata.provider_recording.case.sha256,
        PROVIDER_CASE_SHA256
    );
    assert_eq!(
        metadata.provider_recording.case.path,
        "provider-recordings/full-game.case.json"
    );
    assert_eq!(metadata.provider_recording.case.bytes, 167_971);
    assert_eq!(
        metadata.provider_recording.baseline.sha256,
        PROVIDER_BASELINE_SHA256
    );
    assert_eq!(
        metadata.provider_recording.baseline.path,
        "provider-recordings/full-game.baseline.json"
    );
    assert_eq!(metadata.provider_recording.baseline.bytes, 55_024);

    let runtime = &metadata.provider_recording.runtime;
    assert_eq!(runtime.unit_version, "0.2.0-local-coach.4");
    assert_eq!(runtime.stockfish.version, "18");
    assert_eq!(runtime.stockfish.depth, 16);
    assert_eq!(runtime.stockfish.threads, 1);
    assert_eq!(runtime.stockfish.hash_mib, 16);
    assert_eq!(runtime.stockfish.archive_sha256, STOCKFISH_ARCHIVE_SHA256);
    assert_eq!(runtime.stockfish.binary_sha256, STOCKFISH_BINARY_SHA256);
    assert_eq!(runtime.maia.image, MAIA_IMAGE);
    assert_eq!(runtime.maia.package, "maia2==0.11.0");
    assert_eq!(runtime.maia.model, "rapid");
    assert_eq!(runtime.maia.model_sha256, MAIA_MODEL_SHA256);
    assert_eq!(runtime.maia.config_sha256, MAIA_CONFIG_SHA256);
    assert_eq!(runtime.maia.player_elo, 1246);
    assert_eq!(runtime.maia.opponent_elo, 1246);
    assert_eq!(runtime.maia.zero_probability_candidates, 0);

    let comparison = &metadata.provider_recording.multi_pv_comparison;
    assert_eq!(comparison.requested_variations, 3);
    assert_eq!(comparison.compared_moments, 7);
    assert_eq!(comparison.authoritative_root_disagreements, 1);
    assert_eq!(comparison.unit_version, "0.2.0-local-coach.4");
    assert_eq!(comparison.stockfish_binary_sha256, STOCKFISH_BINARY_SHA256);
}

fn assert_minimal_export(pgn: &str, expected: &ExpectedGame) {
    for excluded in ["{%clk", "{%eval", "Accuracy", "Annotator", "{"] {
        assert!(
            !pgn.contains(excluded),
            "export contains excluded data: {excluded}"
        );
    }
    assert_eq!(header(pgn, "GameId"), Some(GAME_ID));
    assert_eq!(header(pgn, "Site"), Some("https://lichess.org/Synthet1"));
    assert_eq!(header(pgn, "Variant"), Some(expected.variant.as_str()));
    assert_eq!(
        header(pgn, "TimeControl"),
        Some(expected.time_control.as_str())
    );
    assert_eq!(header(pgn, "Result"), Some(expected.result.as_str()));
    assert_eq!(header(pgn, "BlackElo"), Some("1246"));
    assert_eq!(header(pgn, "ECO"), Some(expected.opening.eco.as_str()));
    assert_eq!(header(pgn, "Opening"), Some(expected.opening.name.as_str()));
    assert_eq!(expected.speed, "rapid");
    assert!(header(pgn, "Event").is_some_and(|event| event.contains(expected.speed.as_str())));
    assert_eq!(expected.review_side, "black");
    assert_eq!(expected.player_elo, 1246);
    assert_eq!(expected.termination, "checkmate");
}

fn assert_legal_game_positions(game: &chen_chess_coach_engine::types::Game) {
    let mut position = Chess::default();
    for (index, game_move) in game.moves.iter().enumerate() {
        assert_eq!(game_move.ply, index + 1);
        assert_eq!(game_move.position, position_fen(&position));
        let uci = UciMove::from_ascii(game_move.uci.as_bytes())
            .unwrap_or_else(|_| panic!("invalid UCI at ply {}", game_move.ply));
        let chess_move = uci
            .to_move(&position)
            .unwrap_or_else(|_| panic!("illegal move at ply {}", game_move.ply));
        position.play_unchecked(&chess_move);
    }
    assert_eq!(game.final_position, position_fen(&position));
    assert!(position.is_checkmate());
    assert!(game.is_terminal);
}

fn assert_provider_recording(
    provider_case: &Value,
    pgn: &str,
    game: &chen_chess_coach_engine::types::Game,
    metadata: &CaptureMetadata,
) {
    assert_eq!(provider_case["id"], "full-game");
    assert_eq!(provider_case["input"]["pgn"], pgn);
    assert_eq!(provider_case["input"]["elo"], 1246);
    assert_eq!(provider_case["input"]["operation"]["reviewSide"], "black");
    assert_eq!(
        provider_case["provenance"]["stockfish"],
        format!(
            "Stockfish {} depth {}",
            metadata.provider_recording.runtime.stockfish.version,
            metadata.provider_recording.runtime.stockfish.depth
        )
    );
    assert_eq!(
        provider_case["provenance"]["maiaImage"],
        metadata.provider_recording.runtime.maia.image
    );
    assert_eq!(
        provider_case["provenance"]["maiaPackage"],
        metadata.provider_recording.runtime.maia.package
    );
    assert_eq!(
        provider_case["provenance"]["maiaModel"],
        metadata.provider_recording.runtime.maia.model
    );

    /* capture.json names the runtime the recording was taken under, and the
    recording names the binary that actually answered. Nothing tied the two
    together, so a re-capture under a different Stockfish left capture.json
    describing the old one and every digest downstream stayed self-consistent
    about the wrong binary. Tie them here, at both places the recording carries
    a binary: the whole-case provenance and each comparison search. */
    let recorded_binary = &metadata.provider_recording.runtime.stockfish.binary_sha256;
    assert_eq!(
        provider_case["engineProvenance"]["binarySha256"]
            .as_str()
            .expect("case evidence should record its engine binary"),
        recorded_binary
    );
    for comparison in provider_case["multiPvEvidence"]
        .as_array()
        .expect("comparison evidence should be an array")
    {
        assert_eq!(
            comparison["output"]["provenance"]["binarySha256"]
                .as_str()
                .expect("each comparison should record its engine binary"),
            recorded_binary,
            "comparison at ply {} was searched by a different binary",
            comparison["ply"]
        );
    }

    let evidence = provider_case["evidence"]
        .as_array()
        .expect("provider evidence should be an array");
    assert_eq!(evidence.len(), game.moves.len());
    let mut zero_probability_candidates = 0;
    for (index, item) in evidence.iter().enumerate() {
        let ply = index + 1;
        assert_eq!(item["ply"], ply);
        assert_eq!(item["engineBefore"]["depth"], 16);

        let position = position_from_fen(&game.moves[index].position);
        let best_move = item["engineBefore"]["bestMove"]
            .as_str()
            .expect("best move should be UCI");
        assert_legal_uci(&position, best_move, ply);
        let principal_variation = item["engineBefore"]["principalVariation"]
            .as_array()
            .expect("principal variation should be an array");
        assert_eq!(
            principal_variation.first().and_then(Value::as_str),
            Some(best_move)
        );
        assert_legal_line(&position, principal_variation, ply);

        let candidates = item["humanBefore"]["candidates"]
            .as_array()
            .expect("Maia candidates should be an array");
        assert!(!candidates.is_empty());
        assert!(candidates.len() <= 5);
        let mut previous_probability = 1.0;
        let mut moves = HashSet::new();
        let mut substantive_candidates = 0;
        for (candidate_index, candidate) in candidates.iter().enumerate() {
            assert_eq!(candidate["rank"], candidate_index + 1);
            let probability = candidate["probability"]
                .as_f64()
                .expect("candidate probability should be numeric");
            assert!((0.0..=1.0).contains(&probability));
            assert!(probability <= previous_probability);
            previous_probability = probability;
            let candidate_move = candidate["uci"]
                .as_str()
                .expect("candidate move should be UCI");
            if probability == 0.0 {
                zero_probability_candidates += 1;
            } else {
                substantive_candidates += 1;
            }
            assert_legal_uci(&position, candidate_move, ply);
            assert!(moves.insert(candidate_move));
        }
        assert!(substantive_candidates > 0);
        let win_probability = item["humanBefore"]["winProbability"]
            .as_f64()
            .expect("win probability should be numeric");
        assert!((0.0..=1.0).contains(&win_probability));

        let terminal = item["afterMove"]["kind"] == "terminal";
        assert_eq!(terminal, ply == game.moves.len());
        if let Some(next) = evidence.get(index + 1) {
            assert_eq!(
                item["afterMove"]["evaluation"],
                next["engineBefore"]["evaluation"]
            );
        }
    }
    assert_eq!(
        zero_probability_candidates,
        metadata
            .provider_recording
            .runtime
            .maia
            .zero_probability_candidates
    );

    assert_multi_pv_evidence(provider_case, game, metadata);
}

/// The comparison search is what a Ranked Alternative's gap is measured in, so the
/// recording has to stand on its own: contiguous ranks from one, distinct legal roots, and
/// legal lines. Rank one is allowed to disagree with the authoritative single-PV best move
/// — the two searches are separate — but only the recorded number of times, because each
/// disagreement costs the Critical Moment its Decision Explanation (ADR 0041).
fn assert_multi_pv_evidence(
    provider_case: &Value,
    game: &chen_chess_coach_engine::types::Game,
    metadata: &CaptureMetadata,
) {
    let comparison = &metadata.provider_recording.multi_pv_comparison;
    let evidence = provider_case["evidence"]
        .as_array()
        .expect("provider evidence should be an array");
    let recordings = provider_case["multiPvEvidence"]
        .as_array()
        .expect("recorded MultiPV evidence should be an array");
    assert_eq!(recordings.len(), comparison.compared_moments);

    let mut previous_ply = 0;
    let mut authoritative_root_disagreements = 0;
    for recording in recordings {
        let ply = recording["ply"]
            .as_u64()
            .expect("recorded MultiPV ply should be numeric") as usize;
        assert!(ply > previous_ply, "recorded MultiPV plies should ascend");
        previous_ply = ply;
        assert_eq!(
            recording["output"]["requestedVariations"],
            u64::from(comparison.requested_variations)
        );

        let position = position_from_fen(&game.moves[ply - 1].position);
        let variations = recording["output"]["variations"]
            .as_array()
            .expect("MultiPV variations should be an array");
        assert!(!variations.is_empty());
        assert!(variations.len() <= usize::from(comparison.requested_variations));
        let mut roots = HashSet::new();
        for (index, variation) in variations.iter().enumerate() {
            assert_eq!(variation["rank"], index + 1);
            let best_move = variation["analysis"]["bestMove"]
                .as_str()
                .expect("MultiPV best move should be UCI");
            assert!(roots.insert(best_move), "MultiPV roots should be distinct");
            assert_legal_uci(&position, best_move, ply);
            let line = variation["analysis"]["principalVariation"]
                .as_array()
                .expect("MultiPV principal variation should be an array");
            assert_eq!(line.first().and_then(Value::as_str), Some(best_move));
            assert_legal_line(&position, line, ply);
        }

        let single_pv = &evidence[ply - 1];
        assert_eq!(single_pv["ply"], ply);
        if variations[0]["analysis"]["bestMove"] != single_pv["engineBefore"]["bestMove"] {
            authoritative_root_disagreements += 1;
        }
    }
    assert_eq!(
        authoritative_root_disagreements,
        comparison.authoritative_root_disagreements
    );
}

fn assert_legal_line(position: &Chess, line: &[Value], source_ply: usize) {
    let mut position = position.clone();
    for move_value in line {
        let uci = move_value
            .as_str()
            .expect("principal variation move should be UCI");
        let chess_move = UciMove::from_ascii(uci.as_bytes())
            .unwrap_or_else(|_| panic!("malformed principal variation at ply {source_ply}"))
            .to_move(&position)
            .unwrap_or_else(|_| panic!("illegal principal variation at ply {source_ply}: {uci}"));
        position.play_unchecked(&chess_move);
    }
}

fn assert_legal_uci(position: &Chess, uci: &str, source_ply: usize) {
    UciMove::from_ascii(uci.as_bytes())
        .unwrap_or_else(|_| panic!("malformed best move at ply {source_ply}"))
        .to_move(position)
        .unwrap_or_else(|_| panic!("illegal best move at ply {source_ply}: {uci}"));
}

fn position_from_fen(fen: &str) -> Chess {
    Fen::from_ascii(fen.as_bytes())
        .expect("recorded Position should be FEN")
        .into_position(CastlingMode::Standard)
        .expect("recorded Position should be legal standard chess")
}

fn position_fen(position: &Chess) -> String {
    Fen::from_position(position.clone(), EnPassantMode::Legal).to_string()
}

fn artifact_path(fixture: &Path, artifact: &FileArtifact) -> std::path::PathBuf {
    fixture.join(&artifact.path)
}

fn read_verified_artifact(fixture: &Path, artifact: &FileArtifact) -> Vec<u8> {
    let bytes = fs::read(artifact_path(fixture, artifact)).expect("fixture artifact should exist");
    assert_eq!(bytes.len(), artifact.bytes);
    assert_eq!(hex_sha256(&bytes), artifact.sha256);
    bytes
}

fn header<'a>(pgn: &'a str, name: &str) -> Option<&'a str> {
    let prefix = format!("[{name} \"");
    pgn.lines()
        .find_map(|line| line.strip_prefix(&prefix)?.strip_suffix("\"]"))
}

fn hex_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> T {
    serde_json::from_slice(&fs::read(path).expect("fixture JSON should be readable"))
        .expect("fixture JSON should match its schema")
}

fn fixture_root() -> std::path::PathBuf {
    canonical_game_dir()
}
