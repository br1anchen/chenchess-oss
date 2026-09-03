use std::fmt::Write as _;

use crate::review_session_contract::{LearningResourceRole, LearningTrackPurpose};

use super::{DailyCoachingDigestDetail, DailyCoachingProvider, RenderedDigestEmail};

pub(super) fn render_digest_email(
    digest: &DailyCoachingDigestDetail,
    digest_url: &str,
    unsubscribe_url: &str,
) -> RenderedDigestEmail {
    let subject = format!("Your ChenChess digest for {}", digest.coverage_date);
    let mut text = format!(
        "Your coaching digest for {}\n\n{} games reviewed.\n",
        digest.coverage_date, digest.game_count
    );
    let mut html = format!(
        "<h1>Your coaching digest for {}</h1><p>{} games reviewed.</p>",
        escape_html(&digest.coverage_date),
        digest.game_count
    );
    for (index, priority) in digest.priorities.iter().enumerate() {
        let purpose = match priority.purpose {
            LearningTrackPurpose::Improvement => "Improve",
            LearningTrackPurpose::Reinforcement => "Reinforce",
        };
        let games = if priority.supporting_game_count == 1 {
            "one of your games".to_string()
        } else {
            format!("{} of your games", priority.supporting_game_count)
        };
        let context = format!("Spotted in {games}. Bring it into your next ones.");
        let _ = write!(
            text,
            "\n{}. {} — {}\n{}\n",
            index + 1,
            priority.title,
            purpose,
            context
        );
        let _ = write!(
            html,
            "<section><h2>{}. {}</h2><p><strong>{}</strong> · from {}</p><p>{}</p><ul>",
            index + 1,
            escape_html(&priority.title),
            purpose,
            escape_html(&games),
            escape_html(&context),
        );
        for resource in &priority.resources {
            let role = match resource.role {
                LearningResourceRole::Learn => "Learn",
                LearningResourceRole::Drill => "Drill",
            };
            let _ = writeln!(
                text,
                "{role}: {} — {}",
                resource.title, resource.canonical_url
            );
            let _ = write!(
                html,
                "<li><a href=\"{}\">{}: {}</a></li>",
                escape_html(&resource.canonical_url),
                role,
                escape_html(&resource.title)
            );
        }
        html.push_str("</ul></section>");
    }
    let _ = write!(
        text,
        "\nOpen this digest: {digest_url}\n\nStop digest email: {unsubscribe_url}\n"
    );
    let _ = write!(
        html,
        "<p><a href=\"{}\">Open this digest</a></p><footer><a href=\"{}\">Stop digest email</a></footer>",
        escape_html(digest_url),
        escape_html(unsubscribe_url)
    );
    RenderedDigestEmail {
        subject,
        text,
        html,
    }
}

pub(super) fn render_profile_unavailable_email(
    provider: DailyCoachingProvider,
    dashboard_url: &str,
    unsubscribe_url: &str,
) -> RenderedDigestEmail {
    let provider = match provider {
        DailyCoachingProvider::Lichess => "Lichess",
        DailyCoachingProvider::ChessCom => "Chess.com",
    };
    let subject = format!("Your {provider} profile is unavailable for Daily Coaching");
    let text = format!(
        "Daily Coaching is paused: we can't find your {provider} profile any more.\n\nUpdate your profile link: {dashboard_url}\n\nStop Daily Coaching email: {unsubscribe_url}\n"
    );
    let html = format!(
        "<h1>Daily Coaching is paused</h1><p>We can't find your {provider} profile any more.</p><p><a href=\"{}\">Update your profile link</a></p><footer><a href=\"{}\">Stop Daily Coaching email</a></footer>",
        escape_html(dashboard_url),
        escape_html(unsubscribe_url)
    );
    RenderedDigestEmail {
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
