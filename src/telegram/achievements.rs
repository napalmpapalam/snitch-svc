use std::collections::HashSet;

use chrono::{Timelike, Utc};
use chrono_tz::Europe::Kyiv;
use serenity::model::id::ChannelId;

use crate::events::{DisplayName, Username, VoiceEvent};

use super::state::{
    AchievementKind, RecentLeaves, SessionInfo, Sessions, ShownAchievement, ShownAchievements,
};

/// A detected achievement ready for display.
pub(crate) struct Achievement {
    pub kind: AchievementKind,
    #[allow(dead_code)]
    pub username: Username,
    pub display_name: DisplayName,
}

/// Detects immediate achievements triggered by a join/leave event.
///
/// `pre_channel_count` is the number of members in the affected channel *before* session update.
/// `removed_session` is the session that was removed on leave (for Speed run detection).
pub(crate) fn detect_event_achievements(
    event: &VoiceEvent,
    sessions: &Sessions,
    recent_leaves: &RecentLeaves,
    pre_channel_count: usize,
    removed_session: Option<&SessionInfo>,
    shown_achievements: &mut ShownAchievements,
) -> Vec<Achievement> {
    let update = event.update();
    let username = &update.username;
    let display = &update.display_name;
    let channel_id = update.channel_id;
    let now = Utc::now();
    let today = now.with_timezone(&Kyiv).date_naive();
    let today_shown = shown_achievements.entry(today).or_default();
    let mut achievements = Vec::new();

    match event {
        VoiceEvent::Joined(_) => {
            // Party starter: channel was empty before this join
            if pre_channel_count == 0 {
                try_add(
                    &mut achievements,
                    today_shown,
                    AchievementKind::PartyStarter,
                    username,
                    display,
                );
            }

            // Dynamic duo: channel now has exactly 2 members
            let post_count = count_in_channel(sessions, channel_id);
            if post_count == 2 {
                try_add(
                    &mut achievements,
                    today_shown,
                    AchievementKind::DynamicDuo,
                    username,
                    display,
                );
            }

            // Full house: channel has 5+ members
            if post_count >= 5 {
                try_add(
                    &mut achievements,
                    today_shown,
                    AchievementKind::FullHouse,
                    username,
                    display,
                );
            }

            // Boomerang: left same channel < 5 min ago
            if let Some(leave) = recent_leaves.get(username) {
                let mins_since = (now - leave.left_at).num_minutes();
                if leave.channel_id == channel_id && mins_since < 5 {
                    try_add(
                        &mut achievements,
                        today_shown,
                        AchievementKind::Boomerang,
                        username,
                        display,
                    );
                }
                // Channel hopper: left different channel < 1 min ago
                if leave.channel_id != channel_id && mins_since < 1 {
                    try_add(
                        &mut achievements,
                        today_shown,
                        AchievementKind::ChannelHopper,
                        username,
                        display,
                    );
                }
            }

            // Time-of-day achievements
            let kyiv_hour = now.with_timezone(&Kyiv).time().hour();
            if kyiv_hour < 4 {
                try_add(
                    &mut achievements,
                    today_shown,
                    AchievementKind::NightOwl,
                    username,
                    display,
                );
            }
            if (5..8).contains(&kyiv_hour) {
                try_add(
                    &mut achievements,
                    today_shown,
                    AchievementKind::EarlyBird,
                    username,
                    display,
                );
            }
            if (12..13).contains(&kyiv_hour) {
                try_add(
                    &mut achievements,
                    today_shown,
                    AchievementKind::LunchBreak,
                    username,
                    display,
                );
            }
        }
        VoiceEvent::Left(_) => {
            // Speed run: was in channel < 1 min
            if let Some(session) = removed_session
                && (now - session.joined_at).num_seconds() < 60
            {
                try_add(
                    &mut achievements,
                    today_shown,
                    AchievementKind::SpeedRun,
                    username,
                    display,
                );
            }

            // Last one standing: channel now has exactly 1 member
            let post_count = count_in_channel(sessions, channel_id);
            if post_count == 1 {
                try_add(
                    &mut achievements,
                    today_shown,
                    AchievementKind::LastOneStanding,
                    username,
                    display,
                );
            }
        }
        VoiceEvent::InitialState(_) => {}
    }

    achievements
}

/// Detects duration/time-of-day achievements on tick. Deduped via shown_achievements.
pub(crate) fn detect_tick_achievements(
    sessions: &Sessions,
    shown_achievements: &mut ShownAchievements,
) -> Vec<Achievement> {
    let now = Utc::now();
    let today = now.with_timezone(&Kyiv).date_naive();
    let kyiv_hour = now.with_timezone(&Kyiv).time().hour();
    let today_shown = shown_achievements.entry(today).or_default();
    let mut achievements = Vec::new();

    for (username, session) in sessions {
        let hours_in_channel = (now - session.joined_at).num_hours();

        // Duration achievements (highest first so all applicable are shown)
        let duration_checks: &[(i64, AchievementKind)] = &[
            (12, AchievementKind::LivingHere),
            (8, AchievementKind::Melted),
            (5, AchievementKind::Overtime),
            (3, AchievementKind::Marathon),
        ];

        for (threshold, kind) in duration_checks {
            if hours_in_channel >= *threshold {
                try_add(
                    &mut achievements,
                    today_shown,
                    kind.clone(),
                    username,
                    &session.display_name,
                );
            }
        }

        // Late shift: still in channel after 23:00 Ukraine time
        if kyiv_hour >= 23 {
            try_add(
                &mut achievements,
                today_shown,
                AchievementKind::LateShift,
                username,
                &session.display_name,
            );
        }
    }

    achievements
}

/// Removes stale data: old shown_achievements (not today) and expired recent_leaves (>5 min).
pub(crate) fn cleanup_stale(
    shown_achievements: &mut ShownAchievements,
    recent_leaves: &mut RecentLeaves,
) {
    let now = Utc::now();
    let today = now.with_timezone(&Kyiv).date_naive();

    shown_achievements.retain(|date, _| *date == today);
    recent_leaves.retain(|_, leave| (now - leave.left_at).num_minutes() < 5);
}

fn make(kind: AchievementKind, username: &Username, display_name: &DisplayName) -> Achievement {
    Achievement {
        kind,
        username: username.clone(),
        display_name: display_name.clone(),
    }
}

fn try_add(
    achievements: &mut Vec<Achievement>,
    today_shown: &mut HashSet<ShownAchievement>,
    kind: AchievementKind,
    username: &Username,
    display_name: &DisplayName,
) {
    let key = ShownAchievement {
        username: username.clone(),
        kind: kind.clone(),
    };
    if today_shown.insert(key) {
        achievements.push(make(kind, username, display_name));
    }
}

fn count_in_channel(sessions: &Sessions, channel_id: ChannelId) -> usize {
    sessions
        .values()
        .filter(|s| s.channel_id == channel_id)
        .count()
}
