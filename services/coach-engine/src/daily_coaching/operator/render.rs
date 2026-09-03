use std::fmt::Write;

use super::{OperatorDigestReport, RenderedOperatorDigest};
use crate::daily_coaching::runs::DailyCoachingRunOutcome;

pub(super) fn render_operator_digest(report: &OperatorDigestReport) -> RenderedOperatorDigest {
    let verdict = if report.is_escalating() {
        "ALERT"
    } else {
        "OK"
    };
    let total_runs: u32 = report.outcome_counts.values().sum();
    let subject = format!(
        "[chenchess] daily coaching {} - {verdict} {total_runs} terminal Runs",
        report.window.ends_at.date_naive()
    );
    let mut text = format!(
        "Daily Coaching Operator Digest\nUTC window: {} to {}\nVerdict: {verdict}\n\nOutcomes\n",
        report.window.starts_at, report.window.ends_at
    );
    let mut html = format!(
        "<h1>Daily Coaching Operator Digest</h1><p>UTC window: {} to {}</p><p>Verdict: <strong>{verdict}</strong></p><h2>Outcomes</h2><ul>",
        report.window.starts_at, report.window.ends_at
    );
    for outcome in [
        DailyCoachingRunOutcome::Published,
        DailyCoachingRunOutcome::NoDigest,
        DailyCoachingRunOutcome::Fenced,
        DailyCoachingRunOutcome::Abandoned,
        DailyCoachingRunOutcome::Skipped,
    ] {
        let count = report
            .outcome_counts
            .get(&outcome)
            .copied()
            .unwrap_or_default();
        let _ = writeln!(text, "{outcome:?}: {count}");
        let _ = write!(html, "<li>{outcome:?}: {count}</li>");
    }
    html.push_str("</ul><h2>Thresholds</h2><ul>");
    let _ = writeln!(
        text,
        "\nThresholds\nActive connections: {}\nRetry exhaustion: {}/{}",
        report.active_connections, report.retry_exhausted, report.attempted_games
    );
    let _ = write!(
        html,
        "<li>Active connections: {}</li><li>Retry exhaustion: {}/{}</li>",
        report.active_connections, report.retry_exhausted, report.attempted_games
    );
    for category in &report.escalating_categories {
        let _ = writeln!(text, "Escalating category: {category}");
        let _ = write!(
            html,
            "<li>Escalating category: {}</li>",
            escape_html(category)
        );
    }
    html.push_str("</ul><h2>Terminal Runs</h2><ul>");
    text.push_str("\nTerminal Runs\n");
    for run in &report.runs {
        let imports = run
            .counts
            .game_import_ids
            .iter()
            .map(|id| id.as_str())
            .collect::<Vec<_>>()
            .join(",");
        let line = format!(
            "player={} run={} window={}..{} finished={} outcome={:?} takeovers={} attempted={} permanent_failures={} retry_exhausted={} game_import_ids={}",
            run.player_id.as_str(),
            run.run_id,
            run.starts_at,
            run.ends_at,
            run.finished_at,
            run.outcome,
            run.takeover_count,
            run.counts.attempted_games,
            run.counts.permanent_game_failures,
            run.counts.retry_exhausted,
            imports,
        );
        let _ = writeln!(text, "{line}");
        let _ = write!(html, "<li>{}</li>", escape_html(&line));
    }
    html.push_str("</ul><h2>Profile unavailable flips</h2><ul>");
    text.push_str("\nProfile unavailable flips\n");
    for flip in &report.profile_unavailable {
        let line = format!(
            "player={} entered_at={} diagnostic_category=daily_coaching_profile_unavailable",
            flip.player_id.as_str(),
            flip.entered_at
        );
        let _ = writeln!(text, "{line}");
        let _ = write!(html, "<li>{}</li>", escape_html(&line));
    }
    html.push_str("</ul><h2>Digests published without a provider</h2><ul>");
    text.push_str("\nDigests published without a provider\n");
    for degraded in &report.degraded_providers {
        let line = format!(
            "player={} run={} provider={:?} observed_at={} reason={:?} diagnostic_category=daily_coaching_degraded_provider",
            degraded.player_id.as_str(),
            degraded.run_id,
            degraded.provider,
            degraded.observed_at,
            degraded.reason,
        );
        let _ = writeln!(text, "{line}");
        let _ = write!(html, "<li>{}</li>", escape_html(&line));
    }
    html.push_str(
        "</ul><p>Use correlated Railway logs for diagnosis. This email is not an audit record.</p>",
    );
    text.push_str(
        "\nUse correlated Railway logs for diagnosis. This email is not an audit record.\n",
    );
    RenderedOperatorDigest {
        subject,
        text,
        html,
    }
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
