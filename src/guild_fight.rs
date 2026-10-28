//! Rough odds for guild battles. The game only tells us levels and the
//! guild skills of the other side, so this is an estimate, not a
//! simulation: in a guild fight every member fights in turn and the winner
//! keeps their remaining life, so what decides is the total "mass" of both
//! sides. Life and damage both grow about linearly with level, hence level²
//! per member, boosted by the instructor bonus (+x % on every attribute).

use sf_api::gamestate::{
    guild::{FightableGuild, Guild},
    social::OtherGuild,
};

/// Fighting mass of a guild from the game's list of possible targets,
/// which gives only the member count and the lowest, highest and average
/// level. The spread adds a little on top of the average: level² grows
/// faster than level, so a guild of 150s and 250s is heavier than a guild
/// of 200s. Their instructor bonus is not in the list - the estimate
/// leaves it out, which makes it a touch optimistic
#[must_use]
pub fn fightable_power(g: &FightableGuild) -> f64 {
    let avg = f64::from(g.members_average_level);
    let spread = f64::from(g.members_max_level.saturating_sub(g.members_min_level)) / 4.0;
    f64::from(g.number_of_members) * (avg * avg + spread * spread)
}

/// One suggested target: index into the list, the estimated chance
#[derive(Debug, Clone, PartialEq)]
pub struct Suggestion {
    pub index: usize,
    pub chance: f64,
}

/// The targets worth attacking, best first (6.1): ones the estimate gives
/// at least 60 %, the most honour first - a guild ranked above us that
/// "has no business there" is the one worth the most. With nobody at 60 %
/// the three best chances are offered instead, so the list is never empty
/// while there is a list
#[must_use]
pub fn suggest_targets(own_power: f64, list: &[FightableGuild]) -> Vec<Suggestion> {
    let mut all: Vec<(Suggestion, u32)> = list
        .iter()
        .enumerate()
        .filter(|(_, g)| g.number_of_members > 0)
        .map(|(index, g)| {
            (
                Suggestion {
                    index,
                    chance: win_chance(own_power, fightable_power(g)),
                },
                g.honor,
            )
        })
        .collect();
    let mut good: Vec<(Suggestion, u32)> =
        all.iter().filter(|(s, _)| s.chance >= 0.6).cloned().collect();
    if good.is_empty() {
        all.sort_by(|a, b| b.0.chance.partial_cmp(&a.0.chance).unwrap_or(std::cmp::Ordering::Equal));
        return all.into_iter().take(3).map(|(s, _)| s).collect();
    }
    good.sort_by(|a, b| {
        b.1.cmp(&a.1).then(
            b.0.chance
                .partial_cmp(&a.0.chance)
                .unwrap_or(std::cmp::Ordering::Equal),
        )
    });
    good.into_iter().take(5).map(|(s, _)| s).collect()
}

/// Fighting mass of one member
pub fn member_power(level: u16, instructor_pct: u16) -> f64 {
    let bonus = 1.0 + f64::from(instructor_pct) / 100.0;
    f64::from(level).powi(2) * bonus * bonus
}

/// Fighting mass of a whole guild
pub fn guild_power(
    levels: impl Iterator<Item = u16>,
    instructor_pct: u16,
) -> f64 {
    levels.map(|l| member_power(l, instructor_pct)).sum()
}

/// Fighting mass of the whole guild - every member, signed up or not
pub fn own_power(guild: &Guild) -> f64 {
    guild_power(
        guild.members.iter().map(|m| m.level),
        guild.total_instructor_skill,
    )
}

/// Fighting mass of the members signed up for the attack right now, and
/// how many that is. The whole-guild figure answers "if everybody
/// fights"; this one answers "as things stand" - and a raid the app
/// gave 43 % to with the whole guild was an open question at thirty of
/// fifty signed up
pub fn signed_power(guild: &Guild) -> (f64, usize) {
    use sf_api::gamestate::guild::BattlesJoined;
    let signed: Vec<u16> = guild
        .members
        .iter()
        .filter(|m| {
            matches!(
                m.battles_joined,
                Some(BattlesJoined::Attack | BattlesJoined::Both)
            )
        })
        .map(|m| m.level)
        .collect();
    let n = signed.len();
    (guild_power(signed.into_iter(), guild.total_instructor_skill), n)
}

pub fn other_power(guild: &OtherGuild) -> f64 {
    let instructor: u16 = guild.members.iter().map(|m| m.instructor_lvl).sum();
    guild_power(guild.members.iter().map(|m| m.level), instructor)
}

/// 0.0 - 1.0. Equal mass = 50 %, 20 % more mass ≈ 70 %, double ≈ 94 %
pub fn win_chance(own: f64, enemy: f64) -> f64 {
    if enemy <= 0.0 {
        return 1.0;
    }
    if own <= 0.0 {
        return 0.0;
    }
    let ratio = own / enemy;
    1.0 / (1.0 + ratio.powi(-4))
}

/// A line like "42 members, avg level 187, +36 % instructor"
pub fn describe(guild: &Guild) -> String {
    let n = guild.members.len().max(1);
    let avg = guild
        .members
        .iter()
        .map(|m| u32::from(m.level))
        .sum::<u32>()
        / n as u32;
    format!(
        "{} members, avg level {avg}, +{} % instructor",
        guild.members.len(),
        guild.total_instructor_skill
    )
}

pub fn describe_other(guild: &OtherGuild) -> String {
    let n = guild.members.len().max(1);
    let avg = guild
        .members
        .iter()
        .map(|m| u32::from(m.level))
        .sum::<u32>()
        / n as u32;
    let instructor: u16 = guild.members.iter().map(|m| m.instructor_lvl).sum();
    format!(
        "{} members, avg level {avg}, +{instructor} % instructor",
        guild.members.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chance_is_symmetric_and_monotone() {
        assert!((win_chance(100.0, 100.0) - 0.5).abs() < 1e-9);
        assert!(win_chance(120.0, 100.0) > 0.65);
        assert!(win_chance(200.0, 100.0) > 0.9);
        assert!(
            (win_chance(100.0, 120.0) + win_chance(120.0, 100.0) - 1.0).abs()
                < 1e-9
        );
    }
}

#[cfg(test)]
mod suggest_tests {
    use super::*;

    fn g(members: u8, avg: u32, honor: u32) -> FightableGuild {
        FightableGuild {
            number_of_members: members,
            members_min_level: avg,
            members_max_level: avg,
            members_average_level: avg,
            honor,
            ..Default::default()
        }
    }

    #[test]
    fn the_richest_beatable_guild_comes_first() {
        let own = guild_power(std::iter::repeat_n(200u16, 40), 0);
        let list = vec![g(40, 300, 9000), g(20, 150, 5000), g(30, 180, 7000)];
        let s = suggest_targets(own, &list);
        assert_eq!(s.first().map(|s| s.index), Some(2));
        assert!(s.iter().all(|s| s.index != 0), "a heavier guild is no target");
        // Nobody beatable: the three best chances anyway
        let own = guild_power(std::iter::repeat_n(50u16, 5), 0);
        assert_eq!(suggest_targets(own, &list).len(), 3);
    }
}
