use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    profile_game_feed::{ChessProfileProvider, ProfileGameTimeControlClass},
    review_session_contract::{
        GameImportId, LearningResource, LearningResourceRole, LearningTrackPurpose, ReviewSide,
    },
};

use super::{
    digest::{CoachingDigest, CoachingDigestPriority, DigestedGameCard, PlayerRelativeOutcome},
    runs::{DailyCoachingRunDocument, DailyCoachingRunOutcome},
    DailyCoachingDocument, DailyCoachingProvider, DailyCoachingSetupState,
    PlayingProfileConnection,
};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
pub enum CoachingHost {
    #[serde(rename = "chatgpt")]
    ChatGpt,
    #[serde(rename = "claude")]
    Claude,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CoachingHostConnection {
    pub host: CoachingHost,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum DailyCoachingDashboardState {
    NotConnected {
        archive: Vec<DailyCoachingDigestSummary>,
        host_connections: Vec<CoachingHostConnection>,
    },
    Connected {
        enabled: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        digest_email_enabled: Option<bool>,
        timezone: String,
        connections: Vec<PlayingProfileConnection>,
        host_connections: Vec<CoachingHostConnection>,
        lead: DailyCoachingLeadState,
        archive: Vec<DailyCoachingDigestSummary>,
    },
}

impl DailyCoachingDashboardState {
    pub(super) fn with_digest_email_enabled(mut self, enabled: Option<bool>) -> Self {
        if let Self::Connected {
            digest_email_enabled,
            ..
        } = &mut self
        {
            *digest_email_enabled = enabled;
        }
        self
    }

    pub(super) fn with_host_connections(
        mut self,
        host_connections: Vec<CoachingHostConnection>,
    ) -> Self {
        let host_connections = unique_sorted_host_connections(host_connections);
        match &mut self {
            Self::NotConnected {
                host_connections: slot,
                ..
            }
            | Self::Connected {
                host_connections: slot,
                ..
            } => *slot = host_connections,
        }
        self
    }
}

fn unique_sorted_host_connections(
    connections: Vec<CoachingHostConnection>,
) -> Vec<CoachingHostConnection> {
    let mut hosts: Vec<CoachingHost> = connections
        .into_iter()
        .map(|connection| connection.host)
        .collect();
    hosts.sort();
    hosts.dedup();
    hosts
        .into_iter()
        .map(|host| CoachingHostConnection { host })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum DailyCoachingLeadState {
    Disabled,
    ProfileUnavailable,
    PreparingFirstDigest,
    NoEligibleGamesYet,
    InitialBackfillUnavailable,
    NoEligibleGames { coverage_date: String },
    Digest { digest_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DailyCoachingDigestSummary {
    pub digest_id: String,
    pub coverage_date: String,
    pub published_at: String,
    pub game_count: u8,
    pub learning_path_count: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DailyCoachingDigestDetail {
    pub digest_id: String,
    pub coverage_date: String,
    pub published_at: String,
    pub timezone: String,
    pub game_count: u8,
    pub learning_path_count: u16,
    pub priorities: Vec<DailyCoachingPriority>,
    pub games: Vec<DailyCoachingGameCard>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DailyCoachingPriority {
    pub title: String,
    pub purpose: LearningTrackPurpose,
    pub supporting_game_count: u8,
    pub supporting_game_import_ids: Vec<GameImportId>,
    pub resources: Vec<LearningResource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DailyCoachingGameCard {
    pub game_import_id: GameImportId,
    pub provider: DailyCoachingProvider,
    pub outcome: DailyCoachingGameOutcome,
    pub review_side: DailyCoachingReviewSide,
    pub opening: Option<DailyCoachingOpening>,
    pub opponent_name: Option<String>,
    pub ended_at: String,
    pub time_control_raw: String,
    pub time_control_class: DailyCoachingTimeControlClass,
    pub learning_path_count: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum DailyCoachingReviewSide {
    White,
    Black,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DailyCoachingOpening {
    pub eco: String,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum DailyCoachingGameOutcome {
    Win,
    Loss,
    Draw,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum DailyCoachingTimeControlClass {
    Classical,
    Correspondence,
    Rapid,
    Blitz,
    Bullet,
    UltraBullet,
}

pub(super) fn project_dashboard(
    state: DailyCoachingDocument,
    latest_visible: Option<DailyCoachingRunDocument>,
    archive: Vec<CoachingDigest>,
) -> DailyCoachingDashboardState {
    let has_unresolved_initial_backfill = state.has_unresolved_initial_backfill();
    let has_only_empty_completed_backfills = state.has_only_empty_completed_backfills();
    let has_unavailable_initial_backfill = state.has_unavailable_initial_backfill();
    let all_profiles_unavailable = state.all_profiles_unavailable();
    let setup = state.project();
    let DailyCoachingSetupState::Connected {
        enabled,
        timezone,
        connections,
    } = setup
    else {
        return DailyCoachingDashboardState::NotConnected {
            archive: archive.iter().map(project_summary).collect(),
            host_connections: Vec::new(),
        };
    };

    DailyCoachingDashboardState::Connected {
        enabled,
        digest_email_enabled: None,
        timezone,
        connections,
        host_connections: Vec::new(),
        lead: project_lead(
            enabled,
            all_profiles_unavailable,
            has_unresolved_initial_backfill,
            has_only_empty_completed_backfills,
            has_unavailable_initial_backfill,
            latest_visible.as_ref(),
            &archive,
        ),
        archive: archive.iter().map(project_summary).collect(),
    }
}

pub(super) fn project_digest(
    digest: CoachingDigest,
    cards: Vec<DigestedGameCard>,
) -> DailyCoachingDigestDetail {
    DailyCoachingDigestDetail {
        digest_id: digest.digest_id,
        coverage_date: digest.coverage_date.to_string(),
        published_at: digest.published_at.to_rfc3339(),
        timezone: digest.timezone,
        game_count: digest.game_count,
        learning_path_count: digest.learning_path_count,
        priorities: digest.priorities.iter().map(project_priority).collect(),
        games: cards.iter().map(project_game).collect(),
    }
}

fn project_lead(
    enabled: bool,
    all_profiles_unavailable: bool,
    has_unresolved_initial_backfill: bool,
    has_only_empty_completed_backfills: bool,
    has_unavailable_initial_backfill: bool,
    latest_run: Option<&DailyCoachingRunDocument>,
    archive: &[CoachingDigest],
) -> DailyCoachingLeadState {
    if !enabled {
        return DailyCoachingLeadState::Disabled;
    }
    if all_profiles_unavailable {
        return DailyCoachingLeadState::ProfileUnavailable;
    }
    let latest_digest = archive
        .iter()
        .max_by_key(|digest| (digest.coverage_date, &digest.digest_id));
    if latest_digest.is_none() && has_unresolved_initial_backfill {
        return DailyCoachingLeadState::PreparingFirstDigest;
    }
    if latest_digest.is_none() && has_only_empty_completed_backfills {
        return DailyCoachingLeadState::NoEligibleGamesYet;
    }
    if latest_digest.is_none() && has_unavailable_initial_backfill {
        return DailyCoachingLeadState::InitialBackfillUnavailable;
    }
    let latest_no_digest =
        latest_run.filter(|run| matches!(run.outcome(), Some(DailyCoachingRunOutcome::NoDigest)));
    match (latest_digest, latest_no_digest) {
        (Some(digest), Some(run)) if run.coverage_date() > digest.coverage_date => {
            DailyCoachingLeadState::NoEligibleGames {
                coverage_date: run.coverage_date().to_string(),
            }
        }
        (Some(digest), _) => DailyCoachingLeadState::Digest {
            digest_id: digest.digest_id.clone(),
        },
        (None, Some(run)) => DailyCoachingLeadState::NoEligibleGames {
            coverage_date: run.coverage_date().to_string(),
        },
        (None, None) => DailyCoachingLeadState::PreparingFirstDigest,
    }
}

fn project_summary(digest: &CoachingDigest) -> DailyCoachingDigestSummary {
    DailyCoachingDigestSummary {
        digest_id: digest.digest_id.clone(),
        coverage_date: digest.coverage_date.to_string(),
        published_at: digest.published_at.to_rfc3339(),
        game_count: digest.game_count,
        learning_path_count: digest.learning_path_count,
    }
}

fn project_priority(priority: &CoachingDigestPriority) -> DailyCoachingPriority {
    let purpose = if priority
        .supporting_games
        .iter()
        .any(|support| support.purpose == LearningTrackPurpose::Improvement)
    {
        LearningTrackPurpose::Improvement
    } else {
        LearningTrackPurpose::Reinforcement
    };
    DailyCoachingPriority {
        title: priority
            .resources
            .iter()
            .find(|resource| resource.role == LearningResourceRole::Learn)
            .or_else(|| priority.resources.first())
            .expect("a validated digest priority has exact resources")
            .title
            .clone(),
        purpose,
        supporting_game_count: u8::try_from(priority.supporting_games.len())
            .expect("a validated digest has at most ten supporting Games"),
        supporting_game_import_ids: priority
            .supporting_games
            .iter()
            .map(|support| support.game_import_id.clone())
            .collect(),
        resources: priority.resources.clone(),
    }
}

fn project_game(card: &DigestedGameCard) -> DailyCoachingGameCard {
    DailyCoachingGameCard {
        game_import_id: card.game_import_id.clone(),
        provider: match card.source_identity.provider {
            ChessProfileProvider::Lichess => DailyCoachingProvider::Lichess,
            ChessProfileProvider::ChessCom => DailyCoachingProvider::ChessCom,
        },
        outcome: match card.player_outcome {
            PlayerRelativeOutcome::Win => DailyCoachingGameOutcome::Win,
            PlayerRelativeOutcome::Loss => DailyCoachingGameOutcome::Loss,
            PlayerRelativeOutcome::Draw => DailyCoachingGameOutcome::Draw,
        },
        review_side: match card.review_side {
            ReviewSide::White => DailyCoachingReviewSide::White,
            ReviewSide::Black => DailyCoachingReviewSide::Black,
            ReviewSide::Both => {
                unreachable!("validated digest cards always have one Player review side")
            }
        },
        opening: match (&card.opening_eco, &card.opening_name) {
            (Some(eco), Some(name)) => Some(DailyCoachingOpening {
                eco: eco.clone(),
                name: name.clone(),
            }),
            (None, None) => None,
            (Some(_), None) | (None, Some(_)) => {
                unreachable!("validated digest cards keep opening metadata paired")
            }
        },
        opponent_name: card.opponent_name.clone(),
        ended_at: card.ended_at.to_rfc3339(),
        time_control_raw: card.time_control_raw.clone(),
        time_control_class: match card.time_control_class {
            ProfileGameTimeControlClass::Classical => DailyCoachingTimeControlClass::Classical,
            ProfileGameTimeControlClass::Correspondence => {
                DailyCoachingTimeControlClass::Correspondence
            }
            ProfileGameTimeControlClass::Rapid => DailyCoachingTimeControlClass::Rapid,
            ProfileGameTimeControlClass::Blitz => DailyCoachingTimeControlClass::Blitz,
            ProfileGameTimeControlClass::Bullet => DailyCoachingTimeControlClass::Bullet,
            ProfileGameTimeControlClass::UltraBullet => DailyCoachingTimeControlClass::UltraBullet,
        },
        learning_path_count: card.learning_path_count,
    }
}
