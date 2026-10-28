//! Member grading shared by the guild tab and the daily report

use sf_api::gamestate::guild::{BattlesJoined, Guild, GuildMemberData};

use crate::guild_history::{GuildHistory, Progress};

/// What to do with a member
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Verdict {
    Keep,
    /// Officer material: earns one of the nine seats on contribution
    Promote,
    /// An officer who no longer earns their seat - somebody outside the
    /// nine is contributing more
    Demote,
    Watch,
    Kick,
}

/// How many of the best a promotion is picked from
pub const PROMOTE_TOP: usize = 10;

/// Officer seats to fill, unless the leader sets another number
/// (`Config::officer_seats`). The leader is not one of them, so a full
/// leadership is this many officers plus the leader. The seats are handed
/// out on contribution (see `contribution`): the ones who do most for the
/// guild hold them, and an officer who has fallen behind somebody outside
/// gets the seat taken back
pub const OFFICER_SEATS: u8 = 9;

/// The most seats the leader can ask for: a guild holds fifty and one of
/// them is the leader, so forty-nine is what the game allows. Whether a
/// leadership that size is a good idea is the leader's business, not
/// this program's - the first version of this capped it at twenty-five,
/// which was an opinion dressed up as a limit
pub const OFFICER_SEATS_MAX: u8 = 49;

/// What a member actually brings to the guild, on one 0-100 scale.
///
/// The grade (1-5) answers "should this person stay"; this answers "who
/// carries the guild", which is a different question and needs the things
/// a member *does* rather than the things they are. Everything counts:
///
/// | what | weight | where it comes from |
/// |---|---|---|
/// | guild attacks signed | 25 | the diary, keys "A:" |
/// | defenses signed | 20 | the diary, keys "D:" |
/// | portal swings | 20 | the diary, over `PORTAL_DAYS` |
/// | Hall of Knights | 20 | the member's knight level against the guild's best |
/// | levels gained | 10 | the diary, 7 and 30 days |
/// | treasure + instructor | 5 | against the guild's best |
///
/// A part with nothing behind it yet - no battle has been watched, the
/// diary is a day old - is left out and the rest is weighed again over
/// what is known. Otherwise a fresh diary would read as "nobody in this
/// guild does anything", which is the one answer that is certainly wrong.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Contribution {
    /// 0-100, the number the ladder is sorted by
    pub total: f32,
    /// Each part as a share 0-1, `None` when there is nothing to go on
    pub attacks: Option<f32>,
    pub defenses: Option<f32>,
    pub portal: Option<f32>,
    pub knights: Option<f32>,
    pub levels: Option<f32>,
    pub skills: Option<f32>,
    /// How much of the 100 points had anything behind them
    pub known_weight: f32,
}

/// The guild's best knight / treasure / instructor level - what everyone
/// else is measured against. Zero when nobody has any
#[must_use]
pub fn guild_bests(members: &[GuildMemberData]) -> (u16, u16, u16) {
    let max_of = |f: fn(&GuildMemberData) -> u16| {
        members.iter().map(f).max().unwrap_or(0)
    };
    (
        max_of(|m| u16::from(m.knights)),
        max_of(|m| m.treasure_skill),
        max_of(|m| m.instructor_skill),
    )
}

/// One member's contribution. `bests` is `guild_bests` for the guild
#[must_use]
pub fn contribution(
    m: &GuildMemberData,
    prog: &Progress,
    bests: (u16, u16, u16),
) -> Contribution {
    let (best_knights, best_treasure, best_instructor) = bests;
    let share = |(went, seen): (usize, usize)| -> Option<f32> {
        (seen > 0).then(|| went as f32 / seen as f32)
    };
    let against = |have: u16, best: u16| -> Option<f32> {
        (best > 0).then(|| f32::from(have.min(best)) / f32::from(best))
    };

    let attacks = share(prog.attacks);
    let defenses = share(prog.defenses);
    let portal = share(prog.portal);
    let knights = against(u16::from(m.knights), best_knights);
    // Levels: a week is what the guild feels, the month is the fallback
    // when the diary is younger than that. Six levels a week is a full
    // mark - past that it is the same "plays a lot" either way
    let levels = match (prog.levels_7d, prog.levels_30d) {
        (Some(l7), _) => Some((f32::from(l7) / 6.0).min(1.0)),
        (None, Some(l30)) => Some((f32::from(l30) / 20.0).min(1.0)),
        _ => None,
    };
    // The two skills the members pay for out of their own gold
    let skills = match (
        against(m.treasure_skill, best_treasure),
        against(m.instructor_skill, best_instructor),
    ) {
        (Some(t), Some(i)) => Some((t + i) / 2.0),
        (Some(x), None) | (None, Some(x)) => Some(x),
        (None, None) => None,
    };

    let parts: [(f32, Option<f32>); 6] = [
        (25.0, attacks),
        (20.0, defenses),
        (20.0, portal),
        (20.0, knights),
        (10.0, levels),
        (5.0, skills),
    ];
    let known_weight: f32 =
        parts.iter().filter(|(_, v)| v.is_some()).map(|(w, _)| w).sum();
    let earned: f32 = parts
        .iter()
        .filter_map(|(w, v)| Some(w * (*v)?))
        .sum();
    let total = if known_weight > 0.0 {
        earned / known_weight * 100.0
    } else {
        0.0
    };
    Contribution {
        total,
        attacks,
        defenses,
        portal,
        knights,
        levels,
        skills,
        known_weight,
    }
}

/// One member's numbers, relative to the rest of the guild
pub struct MemberStats<'a> {
    pub member: &'a GuildMemberData,
    /// 0-100: how much of the guild's best treasure+instructor this member has
    /// 0-100: composite of the daily duties
    pub activity: f32,
    pub portal_today: bool,
    pub potions: u8,
    pub online_hours: Option<i64>,
    pub verdict: Verdict,
    /// 1 = best, 5 = worst
    pub grade: u8,
    /// Why, in a few words
    pub reasons: Vec<String>,
    /// Higher = better reason to kick. Used for sorting
    pub kick_score: f32,
    pub progress: Progress,
    /// What this member does for the guild, 0-100 (see `contribution`) -
    /// what the officer seats are handed out on
    pub contribution: Contribution,
    /// Their place in the guild by that number, 1 = most. The leader is
    /// counted with everybody else; the seats skip them
    pub contribution_rank: usize,
    /// Raid wins/losses during member's tenure
    pub raid_wins: u16,
    pub raid_losses: u16,
    /// Genuinely new: they turned up *after* our diary had already started,
    /// and only within the last day. NOT the same as "we have only seen them
    /// once" - on the very first run every member has been seen only once, and
    /// treating the whole guild as brand new is exactly the false 24h-grace /
    /// "everyone joined" noise. So this stays false until we have watched the
    /// guild long enough to know who is actually new.
    pub just_joined: bool,
    /// Days since this member first showed up in our diary, when that was
    /// after the diary itself started - i.e. a real newcomer. None for the
    /// founding cohort and anyone older than the grace period below
    pub newcomer_days: Option<i64>,
}

/// How long a newcomer is left alone by the grading. The game blocks them
/// from raiding for a day, and our diary needs a week before "no level this
/// week" or "battles 0/2" means anything - judging them on day two was
/// exactly how fresh recruits landed on the kick list
pub const NEWCOMER_GRACE_DAYS: i64 = 7;

/// The rules, in plain words. What counts is movement, not gear:
/// * Level progress: levels gained in the last 7 / 30 days (from our own
///   diary of guild refreshes - the game does not remember this).
/// * Battles: how many of the guild attacks/defenses we saw planned the
///   member actually signed up for (or the one planned right now).
/// * Presence: days since last online, portal fight and potions today.
///
/// Every member gets a score; the grade 1..5 is the member's place in the
/// guild (best 15 % = 1, next 25 % = 2, middle 25 % = 3, next 20 % = 4,
/// last 15 % = 5), so the grades always spread out. Two weeks offline is a
/// 5 no matter what. Leader and officers are never suggested for a kick.
pub fn judge(
    m: &GuildMemberData,
    prog: &Progress,
    online_hours: Option<i64>,
    battle_now: (bool, bool, bool, bool),
    newcomer_days: Option<i64>,
) -> (Vec<String>, f32) {
    let mut reasons: Vec<String> = Vec::new();
    let days = online_hours.map(|h| h / 24).unwrap_or(999);
    // Higher = better
    let mut score = 0.0f32;

    // A newcomer gets a clean pass for their first week: the game blocks
    // them from raiding for a day, and our diary has nothing on them yet -
    // reading that empty history as "no levels, no battles" is exactly what
    // put fresh recruits on the kick list the day after they joined.
    // `newcomer_days` is None for the founding cohort (see member_stats), so
    // a fresh diary does not pardon the whole guild
    if let Some(d) = newcomer_days {
        reasons.push(format!(
            "new member - {d} d in guild, settling in ({NEWCOMER_GRACE_DAYS} d grace)"
        ));
        return (reasons, 2.0);
    }

    // --- presence
    match days {
        0 => score += 3.0,
        1 => {
            reasons.push("offline a day".into());
            score += 1.0;
        }
        2..=6 => {
            reasons.push("offline 2+ days - over the 48 h line".into());
            score -= 3.0;
        }
        7..=13 => {
            reasons.push("offline a week".into());
            score -= 4.0;
        }
        _ => {
            reasons.push("offline 2+ weeks".into());
            score -= 8.0;
        }
    }

    // --- level progress
    let enough_history = prog.days_seen >= 2;
    match (prog.levels_7d, prog.levels_30d) {
        (_, Some(0)) => {
            reasons.push("no level in 30 days".into());
            score -= 4.0;
        }
        (Some(0), Some(l30)) => {
            reasons.push(format!("no level this week (+{l30} in 30 d)"));
            score -= 1.0 + 0.2 * l30 as f32;
        }
        (Some(l7), _) => {
            reasons.push(format!("+{l7} level(s) this week"));
            score += (l7 as f32).min(6.0);
        }
        (None, Some(l30)) => {
            reasons.push(format!("+{l30} levels in 30 d"));
            score += (l30 as f32 / 4.0).min(6.0);
        }
        (None, None) if enough_history => {
            reasons.push(format!("tracked {} days so far", prog.days_seen))
        }
        _ => {}
    }

    // --- battles: our diary first, the battle planned right now as backup
    let (signed, seen) = prog.battles;
    let (attack_planned, in_attack, defense_planned, in_defense) = battle_now;
    if seen >= 2 {
        let rate = signed as f32 / seen as f32;
        reasons.push(format!("battles {signed}/{seen}"));
        score += rate * 6.0 - 3.0;
    } else {
        if attack_planned {
            if in_attack {
                score += 1.5;
            } else {
                reasons.push("not in the attack".into());
                score -= 2.0;
            }
        }
        if defense_planned {
            if in_defense {
                score += 1.5;
            } else {
                reasons.push("not in the defense".into());
                score -= 2.0;
            }
        }
    }

    // --- daily duties, small weight. The portal by its share over the
    // watched days once the diary has a few; today alone before that
    let today = chrono::Local::now().date_naive();
    let (fought, watched) = prog.portal;
    if watched >= 3 {
        let rate = fought as f32 / watched as f32;
        reasons.push(format!("portal {fought}/{watched} d"));
        score += rate * 2.0 - 1.0;
    } else if m.portal_fought.is_some_and(|t| t.date_naive() == today) {
        score += 0.5;
    }
    let potions = m.potions.iter().filter(|p| p.is_some()).count();
    score += potions as f32 * 0.25;

    (reasons, score)
}

/// Turns the scores into grades by rank inside the guild
/// Cumulative shares of the guild that get grade 1, 2, 3, 4 (the rest is
/// 5), for the given strictness 0 (lenient) .. 100 (strict)
pub fn grade_cuts(strictness: u8) -> [f32; 4] {
    let t = f32::from(strictness.min(100)) / 100.0;
    // lenient: 30/30/25/10/5, strict: 5/15/25/25/30
    let lenient = [0.30, 0.60, 0.85, 0.95];
    let strict = [0.05, 0.20, 0.45, 0.70];
    let mut cuts = [0.0; 4];
    for i in 0..4 {
        cuts[i] = lenient[i] + (strict[i] - lenient[i]) * t;
    }
    cuts
}

/// Days offline that force grade 4 / grade 5. The guild leader's line:
/// nobody who has been away for more than 48 hours stays, whatever the
/// strictness slider says - so two days is a 5 and one day is at least a 4.
/// The slider still decides everything else
pub fn offline_floors(_strictness: u8) -> (i64, i64) {
    (1, 2)
}

fn grade_by_rank(scores: &[f32], strictness: u8) -> Vec<u8> {
    let cuts = grade_cuts(strictness);
    let n = scores.len();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| {
        scores[b]
            .partial_cmp(&scores[a])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut grades = vec![3u8; n];
    for (place, &idx) in order.iter().enumerate() {
        let p = if n <= 1 {
            0.0
        } else {
            place as f32 / (n - 1) as f32
        };
        grades[idx] = match p {
            p if p <= cuts[0] => 1,
            p if p <= cuts[1] => 2,
            p if p <= cuts[2] => 3,
            p if p <= cuts[3] => 4,
            _ => 5,
        };
    }
    grades
}

pub fn member_stats<'a>(
    members: &'a [GuildMemberData],
    attack_planned: bool,
    defense_planned: bool,
    history: &GuildHistory,
    history_key: &str,
    strictness: u8,
    seats: u8,
) -> Vec<MemberStats<'a>> {
    use sf_api::gamestate::guild::GuildRank;
    let now = chrono::Local::now();
    let today = now.date_naive();
    // Every member's progress first, so we can tell how long the diary itself
    // has been running. `track_start` is the earliest day we have on record for
    // anyone - our own day one. A member only counts as "just joined" when they
    // first appear *after* that, never merely because a fresh diary has seen
    // the whole guild exactly once.
    let progresses: Vec<Progress> = members
        .iter()
        .map(|m| history.progress(history_key, &m.name, now))
        .collect();
    let track_start = progresses.iter().filter_map(|p| p.first_seen).min();

    let mut stats: Vec<MemberStats<'a>> = members
        .iter()
        .zip(progresses)
        .map(|(m, progress)| {
            let portal_today =
                m.portal_fought.is_some_and(|t| t.date_naive() == today);
            let potions =
                m.potions.iter().filter(|p| p.is_some()).count() as u8;
            let online_hours = m.last_online.map(|t| (now - t).num_hours());
            let in_attack = matches!(
                m.battles_joined,
                Some(BattlesJoined::Attack | BattlesJoined::Both)
            );
            let in_defense = matches!(
                m.battles_joined,
                Some(BattlesJoined::Defense | BattlesJoined::Both)
            );

            // Every duty is worth the same. Battles only count while one is
            // actually planned, so nobody gets punished for a quiet week
            let mut parts: Vec<f32> = Vec::new();
            parts.push(if portal_today { 1.0 } else { 0.0 });
            parts.push(f32::from(potions) / 3.0);
            parts.push(match online_hours {
                Some(h) if h < 24 => 1.0,
                Some(h) if h < 72 => 0.5,
                _ => 0.0,
            });
            if attack_planned {
                parts.push(if in_attack { 1.0 } else { 0.0 });
            }
            if defense_planned {
                parts.push(if in_defense { 1.0 } else { 0.0 });
            }
            let activity =
                parts.iter().sum::<f32>() / parts.len() as f32 * 100.0;
            // A real newcomer showed up after our diary had already started.
            // How long ago that was decides both flags: under a day they can
            // not even raid yet; under a week they are not graded at all
            let arrived = progress
                .first_seen
                .zip(track_start)
                .filter(|(f, ts)| f > ts)
                .map(|(f, _)| (today - f).num_days().max(0));
            let just_joined = progress.days_seen <= 1 && arrived.is_some();
            let newcomer_days = arrived.filter(|&d| d < NEWCOMER_GRACE_DAYS);
            let (reasons, score) = judge(
                m,
                &progress,
                online_hours,
                (attack_planned, in_attack, defense_planned, in_defense),
                newcomer_days,
            );

            // Raid wins/losses during member's tenure
            let (raid_wins, raid_losses) = history
                .raids
                .get(history_key)
                .map(|raids| {
                    let (wins, losses) = raids
                        .iter()
                        .filter(|r| {
                            // Only count raids during member's active time
                            progress.first_seen.is_none_or(|fs| r.when.date_naive() >= fs)
                        })
                        .partition::<Vec<_>, _>(|r| r.won);
                    (wins.len() as u16, losses.len() as u16)
                })
                .unwrap_or((0, 0));

            MemberStats {
                member: m,
                activity,
                portal_today,
                potions,
                online_hours,
                verdict: Verdict::Keep,
                grade: 3,
                reasons,
                // Sorting wants "worse first" - flip the sign
                kick_score: -score,
                progress,
                contribution: Contribution::default(),
                contribution_rank: 0,
                raid_wins,
                raid_losses,
                just_joined,
                newcomer_days,
            }
        })
        .collect();

    // What everybody actually does for the guild, and where that puts
    // them. The officer seats are handed out on this and nothing else -
    // a rank is a job, and the job goes to whoever is doing it
    let bests = guild_bests(members);
    for s in &mut stats {
        s.contribution = contribution(s.member, &s.progress, bests);
    }
    {
        let mut by_contribution: Vec<usize> = (0..stats.len()).collect();
        by_contribution.sort_by(|&a, &b| {
            stats[b]
                .contribution
                .total
                .partial_cmp(&stats[a].contribution.total)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for (place, &i) in by_contribution.iter().enumerate() {
            stats[i].contribution_rank = place + 1;
        }
    }
    // The seats, best first. The leader holds none of them; everybody
    // else is ranked on contribution and nothing else.
    //
    // Newcomers are *not* left out of the ranking. They used to be, and
    // that produced the one thing the advice must never do: contradict
    // itself. A four-day-old officer on 80 was outside the seats because
    // of their age, so the line read "demote Darkblitzz 80" next to
    // "promote Flath 79". Their week of grace belongs in the verdict -
    // they are neither promoted nor demoted while it lasts - not in the
    // ordering
    let seat_holders: std::collections::HashSet<usize> = {
        let mut eligible: Vec<usize> = (0..stats.len())
            .filter(|&i| !matches!(stats[i].member.guild_rank, GuildRank::Leader))
            .collect();
        eligible.sort_by(|&a, &b| {
            stats[b]
                .contribution
                .total
                .partial_cmp(&stats[a].contribution.total)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        eligible.into_iter().take(usize::from(seats)).collect()
    };

    let scores: Vec<f32> = stats.iter().map(|s| -s.kick_score).collect();
    let grades = grade_by_rank(&scores, strictness);
    let (floor4, floor5) = offline_floors(strictness);
    // The ten best by score - where an officer is looked for
    let mut order: Vec<usize> = (0..scores.len()).collect();
    order.sort_by(|&a, &b| {
        scores[b]
            .partial_cmp(&scores[a])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let top: Vec<bool> = {
        let mut t = vec![false; scores.len()];
        for &i in order.iter().take(PROMOTE_TOP) {
            t[i] = true;
        }
        t
    };
    let seat_flags: Vec<bool> =
        (0..stats.len()).map(|i| seat_holders.contains(&i)).collect();
    for (((s, grade), in_top), holds_seat) in
        stats.iter_mut().zip(grades).zip(top).zip(seat_flags)
    {
        // A newcomer (first week) is never kicked or watched: the game
        // blocks them from raiding at first, and often there is no
        // last_online on record yet, which would read as "offline forever".
        // None for the founding cohort, so a fresh diary pardons nobody
        let just_joined = s.newcomer_days.is_some();
        let days = s.online_hours.map(|h| h / 24).unwrap_or(999);
        let mut grade = grade;
        // Absolute floors: nobody offline for weeks is "fine" - but skip this
        // for the just-joined, whose empty history must not force a 5
        if !just_joined {
            if days >= floor5 {
                grade = 5;
            } else if days >= floor4 {
                grade = grade.max(4);
            }
        } else {
            grade = grade.min(3);
        }
        s.grade = grade;
        // A 5 is the bottom of the guild by rank - somebody is always
        // there, in a perfect guild too. A kick needs a hard reason on top:
        // over the 48 h line, no level in a month, or signed up for fewer
        // than half the battles we saw. Without one the 5 is a "watch",
        // and the kick list is short enough for an officer to act on
        let (signed, seen) = s.progress.battles;
        let hard_reason = days >= floor5
            || s.progress.levels_30d == Some(0)
            || (seen >= 2 && signed * 2 < seen);
        // Officer material: among the ten best, seen today, in every
        // battle - the diary's, or the one planned right now when the
        // diary has none yet - and all the daily duties done
        let in_attack = matches!(
            s.member.battles_joined,
            Some(BattlesJoined::Attack | BattlesJoined::Both)
        );
        let in_defense = matches!(
            s.member.battles_joined,
            Some(BattlesJoined::Defense | BattlesJoined::Both)
        );
        let every_battle = if seen >= 2 {
            signed == seen
        } else {
            (attack_planned || defense_planned)
                && (!attack_planned || in_attack)
                && (!defense_planned || in_defense)
        };
        // The old "top ten, every battle, duties done" test is gone:
        // the seats are a headcount now, so being good is not enough -
        // somebody has to be ninth or better
        let _ = (in_top, every_battle, PROMOTE_TOP);
        let is_leader = matches!(s.member.guild_rank, GuildRank::Leader);
        let is_officer = matches!(s.member.guild_rank, GuildRank::Officer);
        let place = s.contribution_rank;
        let points = s.contribution.total;
        // The nine seats, handed out on contribution. An officer who still
        // earns theirs keeps it; one who has fallen behind somebody outside
        // gives it back. Officers are still never suggested for a kick -
        // taking the rank away is the whole of the advice
        s.verdict = if is_leader {
            s.reasons.insert(0, "guild leader".into());
            Verdict::Keep
        } else if holds_seat {
            if is_officer {
                s.reasons
                    .insert(0, format!("officer - earns the seat ({points:.0}/100, #{place})"));
                Verdict::Keep
            } else if just_joined {
                // Inside the seats on the numbers, but the numbers are a
                // few days old. The rank can wait for the grace week
                s.reasons.insert(
                    0,
                    format!(
                        "new member - #{place} by contribution \
                         ({points:.0}/100) already, worth a seat once the \
                         first week is up"
                    ),
                );
                Verdict::Keep
            } else {
                s.reasons.insert(
                    0,
                    format!(
                        "officer material - #{place} in the guild by \
                         contribution ({points:.0}/100), one of the \
                         {seats} seats"
                    ),
                );
                Verdict::Promote
            }
        } else if is_officer {
            if just_joined {
                // Made an officer on the day they joined: nothing is known
                // about them yet, so nothing is taken away yet either
                s.reasons
                    .insert(0, "new officer - left alone for the first week".into());
                Verdict::Keep
            } else {
                s.reasons.insert(
                    0,
                    format!(
                        "officer outside the {seats} seats - #{place} by \
                         contribution ({points:.0}/100)"
                    ),
                );
                Verdict::Demote
            }
        } else if just_joined {
            Verdict::Keep
        } else {
            match grade {
                5 if hard_reason => Verdict::Kick,
                4 | 5 => Verdict::Watch,
                _ => Verdict::Keep,
            }
        };
        if s.reasons.is_empty() {
            s.reasons.push("pulls their weight".into());
        }
    }
    stats
}

/// How the ladder is cut up: the twenty-five who carry the guild, the
/// twenty behind them, and the five at the bottom. A guild holds fifty,
/// so with a full one this is the whole roster in three blocks; with a
/// smaller one the last five are still the last five and the middle
/// block is what is left over
pub const LADDER_HEAD: usize = 25;
pub const LADDER_TAIL: usize = 5;

/// The daily "who carries the guild" mail: the whole roster ranked by
/// contribution (see `contribution`) - attacks, defenses, portal swings,
/// the Hall of Knights, levels and the two paid skills, all of it on one
/// 0-100 scale.
///
/// It used to be the best five and the worst five by grade, which said
/// nothing about the forty in between and nothing at all about what
/// anybody actually did for the guild.
pub fn build_report(
    guild: &Guild,
    history: &GuildHistory,
    history_key: &str,
    strictness: u8,
    seats: u8,
) -> String {
    use crate::i18n::{Lang, lang};
    let stats = member_stats(
        &guild.members,
        guild.attacking.is_some(),
        guild.defending.is_some(),
        history,
        history_key,
        strictness,
        seats,
    );
    let bests = guild_bests(&guild.members);
    // Most useful first
    let mut rows: Vec<(String, Contribution)> = stats
        .iter()
        .map(|st| {
            (
                titlecase::titlecase(&st.member.name),
                contribution(st.member, &st.progress, bests),
            )
        })
        .collect();
    rows.sort_by(|a, b| {
        b.1.total
            .partial_cmp(&a.1.total)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });

    let n = rows.len();
    let avg = if n == 0 {
        0.0
    } else {
        rows.iter().map(|r| r.1.total).sum::<f32>() / n as f32
    };
    // The last five are always the last five; the head takes what it can
    // of the twenty-five in front of them, the middle is the remainder
    let tail = LADDER_TAIL.min(n);
    let head = LADDER_HEAD.min(n - tail);
    let block = |from: usize, to: usize| -> String {
        rows[from..to]
            .iter()
            .enumerate()
            .map(|(i, (name, c))| {
                format!("{}. {name} {:.0}", from + i + 1, c.total)
            })
            .collect::<Vec<_>>()
            .join(", ")
    };

    let cs = lang() == Lang::Cs;
    let date = chrono::Local::now().format("%d.%m.");
    let mut out = if cs {
        format!(
            "Přínos cechu {date} (100 = nejvíc; útoky 25, obrany 20, portál \
             20, Síň rytířů 20, levely 10, poklad+instruktor 5). Průměr \
             {avg:.0}/100, členů {n}."
        )
    } else {
        format!(
            "Guild contribution {date} (100 = most; attacks 25, defenses 20, \
             portal 20, Hall of Knights 20, levels 10, treasure+instructor \
             5). Average {avg:.0}/100, {n} members."
        )
    };
    if head > 0 {
        out.push_str(&format!(
            "\n{}: {}",
            if cs { "NEJAKTIVNĚJŠÍCH 25" } else { "MOST ACTIVE 25" },
            block(0, head)
        ));
    }
    let mid_to = n - tail;
    if mid_to > head {
        out.push_str(&format!(
            "\n{}: {}",
            if cs { "DALŠÍCH 20" } else { "NEXT 20" },
            block(head, mid_to)
        ));
    }
    if tail > 0 {
        out.push_str(&format!(
            "\n{}: {}",
            if cs { "POSLEDNÍCH 5" } else { "LAST 5" },
            block(mid_to, n)
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use sf_api::gamestate::guild::GuildRank;

    fn member(name: &str, hours_ago: i64) -> GuildMemberData {
        GuildMemberData {
            name: name.to_string(),
            level: 100,
            last_online: Some(
                chrono::Local::now() - chrono::Duration::hours(hours_ago),
            ),
            guild_rank: GuildRank::Member,
            ..Default::default()
        }
    }

    /// The bottom of the guild by rank is a "watch", not a kick: a kick
    /// needs a hard reason. Twelve members all seen today, nobody has one;
    /// the one who has been away three days does
    #[test]
    fn a_five_without_a_hard_reason_is_a_watch_not_a_kick() {
        let history = GuildHistory::default();
        let mut members: Vec<GuildMemberData> =
            (0..12).map(|i| member(&format!("m{i}"), i)).collect();
        let stats = member_stats(&members, false, false, &history, "k", 40, OFFICER_SEATS);
        assert!(stats.iter().all(|s| s.verdict != Verdict::Kick));
        assert!(stats.iter().any(|s| s.grade == 5));
        members[11] = member("gone", 72);
        let stats = member_stats(&members, false, false, &history, "k", 40, OFFICER_SEATS);
        let gone = stats.iter().find(|s| s.member.name == "gone").unwrap();
        assert_eq!(gone.verdict, Verdict::Kick);
        assert_eq!(gone.grade, 5);
        assert_eq!(
            stats.iter().filter(|s| s.verdict == Verdict::Kick).count(),
            1
        );
    }

    /// The officer seats are a headcount: the nine who contribute most
    /// hold them, whoever they are. An officer inside the nine keeps the
    /// rank; one who has fallen outside is told to give it back
    #[test]
    fn the_nine_best_contributors_hold_the_officer_seats() {
        let history = GuildHistory::default();
        // Twelve members, told apart by their Hall of Knights level -
        // m11 contributes most, m0 least
        let mut members: Vec<GuildMemberData> = (0..12)
            .map(|i| {
                let mut m = member(&format!("m{i}"), 0);
                m.knights = i as u8;
                m
            })
            .collect();
        let stats = member_stats(&members, false, false, &history, "k", 40, OFFICER_SEATS);
        let promoted: Vec<&str> = stats
            .iter()
            .filter(|s| s.verdict == Verdict::Promote)
            .map(|s| s.member.name.as_str())
            .collect();
        assert_eq!(promoted.len(), usize::from(OFFICER_SEATS), "{promoted:?}");
        // The nine best by contribution, not the nine best by anything else
        assert!(promoted.contains(&"m11") && promoted.contains(&"m3"));
        assert!(!promoted.contains(&"m2") && !promoted.contains(&"m0"));

        // The best of them is already an officer: the seat is earned, so
        // there is nothing to advise
        members[11].guild_rank = GuildRank::Officer;
        let stats = member_stats(&members, false, false, &history, "k", 40, OFFICER_SEATS);
        let m11 = stats.iter().find(|s| s.member.name == "m11").unwrap();
        assert_eq!(m11.verdict, Verdict::Keep);
        assert_eq!(m11.contribution_rank, 1);
        assert!(m11.reasons[0].contains("earns the seat"), "{:?}", m11.reasons);
        // ...and one seat fewer is left for the rest
        assert_eq!(
            stats.iter().filter(|s| s.verdict == Verdict::Promote).count(),
            usize::from(OFFICER_SEATS) - 1
        );

        // An officer who contributes least of all gives the seat back
        members[0].guild_rank = GuildRank::Officer;
        let stats = member_stats(&members, false, false, &history, "k", 40, OFFICER_SEATS);
        let m0 = stats.iter().find(|s| s.member.name == "m0").unwrap();
        assert_eq!(m0.verdict, Verdict::Demote);
        assert!(m0.reasons[0].contains("outside"), "{:?}", m0.reasons);
        // An officer is still never on the kick list
        assert!(stats.iter().all(|s| {
            !matches!(s.member.guild_rank, GuildRank::Officer)
                || s.verdict != Verdict::Kick
        }));
    }

    /// The leader holds no seat and is never advised about
    #[test]
    fn the_leader_is_left_out_of_the_bench() {
        let history = GuildHistory::default();
        let mut members: Vec<GuildMemberData> = (0..12)
            .map(|i| {
                let mut m = member(&format!("m{i}"), 0);
                m.knights = i as u8;
                m
            })
            .collect();
        members[11].guild_rank = GuildRank::Leader;
        let stats = member_stats(&members, false, false, &history, "k", 40, OFFICER_SEATS);
        let boss = stats.iter().find(|s| s.member.name == "m11").unwrap();
        assert_eq!(boss.verdict, Verdict::Keep);
        assert!(boss.reasons[0].contains("leader"), "{:?}", boss.reasons);
        // The nine seats go to the nine best under them
        let promoted: Vec<&str> = stats
            .iter()
            .filter(|s| s.verdict == Verdict::Promote)
            .map(|s| s.member.name.as_str())
            .collect();
        assert_eq!(promoted.len(), usize::from(OFFICER_SEATS));
        assert!(promoted.contains(&"m10") && promoted.contains(&"m2"));
    }
}

#[cfg(test)]
mod contribution_tests {
    use super::*;
    use crate::guild_history::Progress;
    use sf_api::gamestate::guild::{GuildMemberData, GuildRank};

    fn prog(
        attacks: (usize, usize),
        defenses: (usize, usize),
        portal: (usize, usize),
        levels_7d: Option<u16>,
    ) -> Progress {
        Progress {
            levels_7d,
            levels_30d: None,
            battles: (attacks.0 + defenses.0, attacks.1 + defenses.1),
            days_seen: 10,
            first_seen: None,
            portal,
            attacks,
            defenses,
        }
    }

    fn member(name: &str, knights: u8, treasure: u16) -> GuildMemberData {
        GuildMemberData {
            name: name.to_string(),
            level: 100,
            knights,
            treasure_skill: treasure,
            instructor_skill: treasure,
            guild_rank: GuildRank::Member,
            ..Default::default()
        }
    }

    /// Everything done, nothing missed: a full hundred. Nothing done at
    /// all: nothing
    #[test]
    fn the_scale_runs_from_nothing_to_everything() {
        let bests = (12u16, 20u16, 20u16);
        let best = member("best", 12, 20);
        let full = contribution(
            &best,
            &prog((8, 8), (6, 6), (14, 14), Some(9)),
            bests,
        );
        assert!((full.total - 100.0).abs() < 0.01, "{}", full.total);
        let idle = member("idle", 0, 0);
        let none =
            contribution(&idle, &prog((0, 8), (0, 6), (0, 14), Some(0)), bests);
        assert!(none.total < 0.01, "{}", none.total);
    }

    /// The Hall of Knights counts, and it counts a lot: two members who
    /// do the same in every battle are told apart by it
    #[test]
    fn the_hall_of_knights_separates_two_equal_fighters() {
        let bests = (10u16, 0u16, 0u16);
        let p = prog((4, 8), (3, 6), (7, 14), Some(3));
        let donor = contribution(&member("donor", 10, 0), &p, bests);
        let freeloader = contribution(&member("free", 0, 0), &p, bests);
        assert!(donor.total > freeloader.total + 15.0,
            "{} vs {}", donor.total, freeloader.total);
        assert_eq!(donor.knights, Some(1.0));
        assert_eq!(freeloader.knights, Some(0.0));
    }

    /// An attack missed costs more than a defense missed, and both cost
    /// more than a portal day
    #[test]
    fn attacks_weigh_most_defenses_next_portal_after() {
        let bests = (0u16, 0u16, 0u16);
        let all = prog((4, 4), (4, 4), (4, 4), None);
        let base = contribution(&member("m", 0, 0), &all, bests).total;
        let no_attacks =
            contribution(&member("m", 0, 0), &prog((0, 4), (4, 4), (4, 4), None), bests).total;
        let no_defenses =
            contribution(&member("m", 0, 0), &prog((4, 4), (0, 4), (4, 4), None), bests).total;
        let no_portal =
            contribution(&member("m", 0, 0), &prog((4, 4), (4, 4), (0, 4), None), bests).total;
        assert!((base - 100.0).abs() < 0.01);
        assert!(no_attacks < no_defenses, "{no_attacks} vs {no_defenses}");
        assert!(no_defenses <= no_portal, "{no_defenses} vs {no_portal}");
    }

    /// A diary with nothing in it yet does not read as "this member does
    /// nothing" - the parts with no data are left out of the sum
    #[test]
    fn an_empty_diary_is_not_a_zero() {
        let bests = (0u16, 0u16, 0u16);
        let blank = Progress {
            levels_7d: None,
            levels_30d: None,
            battles: (0, 0),
            days_seen: 0,
            first_seen: None,
            portal: (0, 0),
            attacks: (0, 0),
            defenses: (0, 0),
        };
        let c = contribution(&member("new", 0, 0), &blank, bests);
        assert_eq!(c.known_weight, 0.0);
        assert_eq!(c.total, 0.0);
        // With one part known, that part is the whole mark
        let one = contribution(&member("new", 0, 0), &prog((3, 4), (0, 0), (0, 0), None), bests);
        assert_eq!(one.known_weight, 25.0);
        assert!((one.total - 75.0).abs() < 0.01, "{}", one.total);
    }
}

#[cfg(test)]
mod seat_order_tests {
    use super::*;
    use crate::guild_history::{GuildHistory, MemberHistory};
    use sf_api::gamestate::guild::{GuildMemberData, GuildRank};
    use std::collections::BTreeMap;

    fn member(name: &str, knights: u8) -> GuildMemberData {
        GuildMemberData {
            name: name.to_string(),
            level: 100,
            knights,
            last_online: Some(chrono::Local::now()),
            guild_rank: GuildRank::Member,
            ..Default::default()
        }
    }

    /// The advice must never contradict itself: nobody is demoted while
    /// somebody with a lower contribution is promoted. A newcomer's week
    /// of grace belongs in the verdict, not in the ordering - putting it
    /// in the ordering is what produced "demote 80, promote 79"
    #[test]
    fn a_newcomer_never_pushes_a_better_member_out_of_the_seats() {
        let today = chrono::Local::now().date_naive();
        let mut history = GuildHistory::default();
        let guild = history.guilds.entry("k".to_string()).or_default();
        // The diary has been running a fortnight for everybody...
        for i in 0..6 {
            let mut levels = BTreeMap::new();
            for d in 0..14 {
                levels.insert(today - chrono::Duration::days(d), 100);
            }
            guild.insert(
                format!("m{i}"),
                MemberHistory { levels, ..Default::default() },
            );
        }
        // ...except the best of them, who turned up four days ago
        let mut levels = BTreeMap::new();
        for d in 0..4 {
            levels.insert(today - chrono::Duration::days(d), 100);
        }
        guild.insert(
            "fresh".to_string(),
            MemberHistory { levels, ..Default::default() },
        );

        let mut members: Vec<GuildMemberData> =
            (0..6).map(|i| member(&format!("m{i}"), i as u8)).collect();
        // The newcomer contributes more than anybody, and already holds
        // the rank
        let mut fresh = member("fresh", 20);
        fresh.guild_rank = GuildRank::Officer;
        members.push(fresh);

        let stats = member_stats(&members, false, false, &history, "k", 40, 3);
        let fresh = stats.iter().find(|s| s.member.name == "fresh").unwrap();
        assert_eq!(fresh.contribution_rank, 1, "{:?}", fresh.contribution);
        // Top of the guild: the seat is not taken away
        assert_ne!(
            fresh.verdict,
            Verdict::Demote,
            "a top contributor must never be demoted: {:?}",
            fresh.reasons
        );
        // ...and nobody below them is promoted while they are pushed out
        let promoted_worse = stats.iter().any(|s| {
            s.verdict == Verdict::Promote
                && s.contribution.total < fresh.contribution.total
                && fresh.verdict == Verdict::Demote
        });
        assert!(!promoted_worse);
    }
}
