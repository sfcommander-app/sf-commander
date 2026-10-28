//! How often the bot has to look in, worked out from the game rather than
//! guessed at.
//!
//! Everything the app collects falls into one of two shapes:
//!
//! * a **store** - the fortress's wood, stone and experience, the
//!   underworld's soul extractor, the underworld's own gold and thirst.
//!   It fills at a known rate into a known cap, and once it is full the
//!   production **is thrown away**. So the store names a deadline: visit
//!   within `limit / per_hour` and nothing is lost; visit later and the
//!   overshoot is gone for good.
//! * a **cooldown** - the free arena fight, the free dungeon fight, the
//!   pets' feeding. Nothing spoils, but one visit consumes one of them, so
//!   the number you get in a day is the number of visits you make.
//!
//! The recommendation is therefore the shortest store deadline: the longest
//! gap at which nothing is thrown away. Fewer sessions is better for every
//! other reason, so the answer is that number and not a minute less. The
//! cooldowns come back as a second answer - what a day at that pace yields -
//! because they are a reason to go faster that has nothing to do with waste.
//!
//! Nothing here reads the clock or the network: it is arithmetic on figures
//! the game has already sent, which is what makes it testable.

use crate::i18n::t;

/// One thing the bot collects, and what waiting costs.
#[derive(Debug, Clone, PartialEq)]
pub struct Producer {
    /// What it is called on screen
    pub name: String,
    /// What it makes in an hour, in its own units
    pub per_hour: u64,
    /// What it holds before the rest is thrown away. `None` for a cooldown,
    /// which holds exactly one
    pub cap: Option<u64>,
    /// How long between two of them, for a cooldown
    pub every_minutes: Option<u32>,
}

impl Producer {
    /// A store: fills at `per_hour` into `cap`
    #[must_use]
    pub fn store(name: impl Into<String>, per_hour: u64, cap: u64) -> Self {
        Producer {
            name: name.into(),
            per_hour,
            cap: Some(cap),
            every_minutes: None,
        }
    }

    /// A cooldown: one every `minutes`, nothing spoils
    #[must_use]
    pub fn cooldown(name: impl Into<String>, minutes: u32) -> Self {
        Producer {
            name: name.into(),
            per_hour: 0,
            cap: None,
            every_minutes: Some(minutes),
        }
    }

    /// Minutes from empty to full - the longest gap that loses nothing.
    /// A store that produces nothing, or has no room, never binds
    #[must_use]
    pub fn full_in_minutes(&self) -> Option<u32> {
        let cap = self.cap?;
        if self.per_hour == 0 || cap == 0 {
            return None;
        }
        let minutes = (cap as f64 / self.per_hour as f64) * 60.0;
        Some(minutes.round().clamp(1.0, f64::from(u32::MAX)) as u32)
    }

    /// What a gap of `gap` minutes throws away, as a share of everything
    /// this producer makes in that time. A cooldown wastes nothing
    #[must_use]
    pub fn wasted_share(&self, gap_minutes: u32) -> f64 {
        let Some(full) = self.full_in_minutes() else {
            return 0.0;
        };
        if gap_minutes <= full {
            return 0.0;
        }
        1.0 - f64::from(full) / f64::from(gap_minutes)
    }

    /// How many of a cooldown's turns a day of `gap`-minute visits collects.
    /// One visit takes one, however long it has been waiting
    #[must_use]
    pub fn per_day_at(&self, gap_minutes: u32) -> Option<u32> {
        let every = self.every_minutes?;
        let gap = gap_minutes.max(1);
        let visits = 24 * 60 / gap;
        // A visit can only take one if one has come round since the last
        let available = 24 * 60 / every.max(1);
        Some(visits.min(available))
    }
}

/// What the numbers add up to: the gap to aim for and why it is that one
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// The longest gap, in minutes, at which no store overflows
    pub gap_minutes: u32,
    /// The store that runs out of room first - the reason for the gap
    pub binding: Option<String>,
    /// Where the safety slider has to sit to get that gap
    pub slider: u8,
    /// What the slider actually delivers at that position. It moves in
    /// steps, so this is the gap that will really be used
    pub slider_gap_minutes: u32,
    /// Visits in a day at the recommended gap
    pub visits_per_day: u32,
}

/// The shortest gap any store allows, and the slider position that keeps to
/// it. `floor` is the fastest the app is ever willing to recommend - going
/// under it is a pattern no person has, whatever the arithmetic says
#[must_use]
pub fn plan(producers: &[Producer], floor_minutes: u32) -> Plan {
    let binding = producers
        .iter()
        .filter_map(|p| p.full_in_minutes().map(|m| (m, p.name.clone())))
        .min_by_key(|(m, _)| *m);

    // With nothing that can overflow there is nothing to hurry for, so the
    // slowest the slider offers is the right answer
    let (raw_gap, name) = match binding {
        Some((m, name)) => (m, Some(name)),
        None => (crate::stealth::gap_minutes(1), None),
    };
    let gap = raw_gap.max(floor_minutes);
    let slider = slider_for_gap(gap);
    Plan {
        gap_minutes: gap,
        binding: name,
        slider,
        slider_gap_minutes: crate::stealth::gap_minutes(slider),
        visits_per_day: (24 * 60 / gap.max(1)).max(1),
    }
}

/// The lowest slider position whose average gap is no longer than `gap` -
/// the fewest sessions that still keep up. The slider is a ladder of fixed
/// averages, so this rounds down to a rung rather than inventing one
#[must_use]
pub fn slider_for_gap(gap_minutes: u32) -> u8 {
    for level in 1..=crate::stealth::MAX_LEVEL {
        if crate::stealth::gap_minutes(level) <= gap_minutes {
            return level;
        }
    }
    crate::stealth::MAX_LEVEL
}

/// Everything this character produces, read off the game state. Buildings
/// that are not built, and stores with no room, are left out - a producer
/// that cannot overflow is not a reason to visit
#[must_use]
pub fn producers(gs: &sf_api::gamestate::GameState) -> Vec<Producer> {
    use sf_api::gamestate::underworld::UnderworldResourceType as UR;
    let mut out = Vec::new();

    if let Some(fortress) = gs.fortress.as_ref() {
        for (typ, res) in &fortress.resources {
            let name = match format!("{typ:?}").as_str() {
                "Wood" => t("Wood"),
                "Stone" => t("Stone"),
                "Experience" => t("Experience"),
                _ => continue,
            };
            out.push(Producer::store(
                name,
                res.production.per_hour,
                res.production.limit,
            ));
        }
    }

    if let Some(uw) = gs.underworld.as_ref() {
        let souls = &uw.production[UR::Souls];
        out.push(Producer::store(
            t("Soul Extractor"),
            souls.per_hour,
            souls.limit,
        ));
        let gold = &uw.production[UR::Silver];
        out.push(Producer::store(t("Gold Pit"), gold.per_hour, gold.limit));
    }

    // The two free fights. They spoil nothing, so they never set the gap -
    // but how many of them a day comes out of it, and that is worth saying
    out.push(Producer::cooldown(t("Free arena fight"), 10));
    out.push(Producer::cooldown(t("Free dungeon fight"), 10));
    out.push(Producer::cooldown(t("Pet feeding"), 30));

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_store_names_the_hour_it_starts_throwing_things_away() {
        // 1000 an hour into a 3000 store: full in three hours
        let p = Producer::store("wood", 1000, 3000);
        assert_eq!(p.full_in_minutes(), Some(180));
        // Inside that, nothing is lost
        assert_eq!(p.wasted_share(180), 0.0);
        assert_eq!(p.wasted_share(60), 0.0);
        // Twice as long as it takes to fill throws away half the day
        assert!((p.wasted_share(360) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn a_store_that_makes_nothing_never_asks_for_a_visit() {
        assert_eq!(Producer::store("x", 0, 5000).full_in_minutes(), None);
        assert_eq!(Producer::store("x", 500, 0).full_in_minutes(), None);
        assert_eq!(Producer::store("x", 0, 5000).wasted_share(10_000), 0.0);
    }

    #[test]
    fn the_tightest_store_is_the_one_that_sets_the_pace() {
        let ps = vec![
            Producer::store("wood", 1000, 8000),  // 8 h
            Producer::store("stone", 1000, 3000), // 3 h  <- binds
            Producer::store("xp", 100, 9000),     // 90 h
            Producer::cooldown("arena", 10),
        ];
        let plan = plan(&ps, 30);
        assert_eq!(plan.gap_minutes, 180);
        assert_eq!(plan.binding.as_deref(), Some("stone"));
        assert_eq!(plan.visits_per_day, 8);
    }

    #[test]
    fn the_floor_beats_the_arithmetic() {
        // A store that fills in four minutes would ask for fifteen visits
        // an hour. Nobody plays like that, so the floor wins
        let ps = vec![Producer::store("tiny", 60_000, 4_000)];
        assert_eq!(plan(&ps, 45).gap_minutes, 45);
    }

    #[test]
    fn nothing_to_overflow_means_the_slowest_setting() {
        let ps = vec![Producer::cooldown("arena", 10)];
        let plan = plan(&ps, 30);
        assert!(plan.binding.is_none());
        assert_eq!(plan.gap_minutes, crate::stealth::gap_minutes(1));
        assert_eq!(plan.slider, 1);
    }

    #[test]
    fn the_slider_rounds_towards_fewer_sessions() {
        // The ladder is 480, 360, 240, 180, 120, 90, 70, 55, 45, 36
        // A three-hour allowance takes the 180 rung, not the 120 one
        assert_eq!(crate::stealth::gap_minutes(slider_for_gap(180)), 180);
        // Two and a half hours is not a rung: the next one *down* is taken,
        // because a longer gap would start throwing things away
        assert_eq!(crate::stealth::gap_minutes(slider_for_gap(150)), 120);
        // More than the slowest rung still parks at the slowest
        assert_eq!(slider_for_gap(10_000), 1);
        // Less than the fastest rung parks at the fastest
        assert_eq!(slider_for_gap(1), crate::stealth::MAX_LEVEL);
    }

    #[test]
    fn a_cooldown_pays_once_per_visit_and_never_more_than_it_has() {
        let arena = Producer::cooldown("arena", 10);
        // Eight visits a day take eight fights
        assert_eq!(arena.per_day_at(180), Some(8));
        // Visiting faster than the cooldown cannot take more than it makes
        assert_eq!(arena.per_day_at(1), Some(144));
        // A store is not a cooldown
        assert_eq!(Producer::store("wood", 1, 2).per_day_at(60), None);
    }

    #[test]
    fn the_plan_reports_what_the_slider_will_really_do() {
        // A store that fills in 150 minutes cannot be met exactly: the
        // slider's nearest rung that keeps up is 120
        let ps = vec![Producer::store("stone", 100, 250)];
        let plan = plan(&ps, 30);
        assert_eq!(plan.gap_minutes, 150);
        assert_eq!(plan.slider_gap_minutes, 120);
    }
}
