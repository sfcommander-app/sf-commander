//! A few dungeon enemies - the whole twister among them - ship with no
//! damage figures in the library's tables, so their fight can not be
//! simulated and the odds show up as "no data". Rather than leaving those
//! fights untouched forever, the app takes one swing a day and remembers
//! how it went in `probe_history.json` next to the config.
//!
//! Two records, and the second one is the one that does the work:
//!
//! * **by floor** - exact, and almost always empty. A dungeon is walked
//!   through: every win moves to the next floor, so the floor about to be
//!   fought has never been fought. This record only ever fills up on the
//!   floors that were *lost* on, which is why the twister's odds read "-"
//!   for ever while everything was going well
//! * **by level gap** - the enemy's level less the character's, in bands
//!   of `BAND`. This is what carries from one floor to the next, and it
//!   is also the thing that actually decides these fights: "enemies about
//!   twenty levels under me go my way four times in five" is a sentence
//!   that survives moving on, and after one afternoon of the twister it
//!   has real numbers behind it

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

const FILE: &str = "probe_history.json";

/// Wins and losses seen against one enemy (one floor of one dungeon)
#[derive(Debug, Default, Serialize, Deserialize, Clone, Copy)]
pub struct Tally {
    #[serde(default)]
    pub wins: u16,
    #[serde(default)]
    pub losses: u16,
}

impl Tally {
    #[must_use]
    pub fn attempts(&self) -> u16 {
        self.wins.saturating_add(self.losses)
    }

    /// Empirical win chance (0.0 - 1.0) with Laplace smoothing, so a single
    /// lucky win reads as a cautious two thirds rather than a confident
    /// 100 %, and the estimate slides toward the truth as samples pile up
    #[must_use]
    pub fn estimate(&self) -> f64 {
        let w = f64::from(self.wins) + 1.0;
        let l = f64::from(self.losses) + 1.0;
        w / (w + l)
    }
}

/// Level gaps are counted in bands this wide. Two levels either way is
/// noise at level two hundred; twenty is a different fight
pub const BAND: i16 = 5;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ProbeHistory {
    /// char key -> dungeon (`{Dungeon:?}`) -> floor -> tally
    #[serde(default)]
    pub chars: BTreeMap<String, BTreeMap<String, BTreeMap<u16, Tally>>>,
    /// char key -> level-gap band -> tally.
    ///
    /// **The floor tally above cannot work for a dungeon you walk
    /// through.** Every win moves you to the next floor, so the floor you
    /// are about to fight has never been fought - the record only ever
    /// fills up on the floors you *lose* on, and the column reads "-"
    /// forever while things are going well. That is exactly what the
    /// twister did.
    ///
    /// The gap between the enemy's level and yours does generalise, and
    /// it is the thing that actually decides these fights. One band is
    /// `BAND` levels wide
    #[serde(default)]
    pub gaps: BTreeMap<String, BTreeMap<i16, Tally>>,
    #[serde(skip)]
    dirty: bool,
}

impl ProbeHistory {
    #[must_use]
    pub fn load() -> Self {
        std::fs::read_to_string(crate::app_path(FILE))
            .or_else(|_| std::fs::read_to_string(FILE))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&mut self) {
        if !self.dirty {
            return;
        }
        // `dirty` only clears on a write that went through: a disk that
        // refused the file used to be forgotten as "saved"
        match serde_json::to_string(self) {
            Ok(s) => {
                if crate::storage::write_atomic(&crate::app_path(FILE), s.as_bytes())
                    .is_ok()
                {
                    self.dirty = false;
                }
            }
            Err(e) => log::error!("probe history: {e}"),
        }
    }

    /// One character on one server. The name is lowercased so a rename of the
    /// display case does not split the record
    #[must_use]
    pub fn key(server_url: &str, char_name: &str) -> String {
        format!(
            "{}#{}",
            crate::get_server_code(server_url),
            char_name.to_lowercase()
        )
    }

    #[must_use]
    pub fn get(&self, key: &str, dungeon: &str, floor: u16) -> Option<Tally> {
        self.chars.get(key)?.get(dungeon)?.get(&floor).copied()
    }

    /// The band a gap falls in
    #[must_use]
    pub fn band(gap: i16) -> i16 {
        gap.div_euclid(BAND)
    }

    /// What past fights say about an enemy this far above (or below) the
    /// character, taken from the band and its two neighbours so that a
    /// fresh band is not left empty next to a well-filled one
    #[must_use]
    pub fn by_gap(&self, key: &str, gap: i16) -> Option<Tally> {
        let bands = self.chars_gaps(key)?;
        let here = Self::band(gap);
        let mut out = Tally::default();
        for b in [here - 1, here, here + 1] {
            if let Some(t) = bands.get(&b) {
                // The band itself counts double - a neighbour is a hint,
                // not the same fight
                let weight = if b == here { 2 } else { 1 };
                out.wins = out.wins.saturating_add(t.wins * weight);
                out.losses = out.losses.saturating_add(t.losses * weight);
            }
        }
        (out.attempts() > 0).then_some(out)
    }

    fn chars_gaps(&self, key: &str) -> Option<&BTreeMap<i16, Tally>> {
        self.gaps.get(key)
    }

    pub fn record(
        &mut self,
        key: &str,
        dungeon: &str,
        floor: u16,
        gap: i16,
        won: bool,
    ) {
        let bump = |t: &mut Tally| {
            if won {
                t.wins = t.wins.saturating_add(1);
            } else {
                t.losses = t.losses.saturating_add(1);
            }
        };
        bump(
            self.chars
                .entry(key.to_string())
                .or_default()
                .entry(dungeon.to_string())
                .or_default()
                .entry(floor)
                .or_default(),
        );
        bump(
            self.gaps
                .entry(key.to_string())
                .or_default()
                .entry(Self::band(gap))
                .or_default(),
        );
        self.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The regression this exists for: a dungeon you walk through never
    /// fights the same floor twice, so the floor tally is empty exactly
    /// when it is needed. The level gap carries across floors
    #[test]
    fn what_was_learned_on_one_floor_carries_to_the_next() {
        let mut h = ProbeHistory::default();
        let key = "EU1#hero";
        // Three floors of the twister, all against enemies about twenty
        // levels under the character, all won
        for floor in [311u16, 312, 313] {
            h.record(key, "Shadow(Twister)", floor, -20, true);
        }
        // The floor we are about to fight has never been fought...
        assert!(h.get(key, "Shadow(Twister)", 314).is_none());
        // ...and the record still has something to say about it
        let t = h.by_gap(key, -21).expect("the gap generalises");
        assert!(t.estimate() > 0.7, "{}", t.estimate());
        // An enemy far above the character is a different question
        assert!(h.by_gap(key, 60).is_none());
    }


    #[test]
    fn an_empty_tally_sits_at_a_coin_flip() {
        let t = Tally::default();
        assert_eq!(t.attempts(), 0);
        assert!((t.estimate() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn one_win_is_hopeful_but_not_certain() {
        // A single win must never read as a confident 100 % - that is exactly
        // the bug that sent the free fight into the twister every hour
        let t = Tally { wins: 1, losses: 0 };
        assert!(t.estimate() > 0.5 && t.estimate() < 0.8);
    }

    #[test]
    fn more_wins_pull_the_estimate_up_and_losses_down() {
        let good = Tally { wins: 9, losses: 1 };
        let bad = Tally { wins: 1, losses: 9 };
        assert!(good.estimate() > 0.75);
        assert!(bad.estimate() < 0.25);
    }

    #[test]
    fn recording_lands_under_the_right_floor_and_survives_a_round_trip() {
        let mut h = ProbeHistory::default();
        let key = "EU1#hero";
        h.record(key, "Shadow(Twister)", 3, -20, true);
        h.record(key, "Shadow(Twister)", 3, -20, false);
        h.record(key, "Shadow(Twister)", 4, -20, true);
        let f3 = h.get(key, "Shadow(Twister)", 3).unwrap();
        assert_eq!((f3.wins, f3.losses), (1, 1));
        let f4 = h.get(key, "Shadow(Twister)", 4).unwrap();
        assert_eq!((f4.wins, f4.losses), (1, 0));
        assert!(h.get(key, "Shadow(Twister)", 9).is_none());

        let json = serde_json::to_string(&h).unwrap();
        let back: ProbeHistory = serde_json::from_str(&json).unwrap();
        let f3b = back.get(key, "Shadow(Twister)", 3).unwrap();
        assert_eq!((f3b.wins, f3b.losses), (1, 1));
    }
}
