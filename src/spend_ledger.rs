//! What each character has already paid for today: mushroom dungeon
//! fights, mushroom arena fights, shop rerolls. The daily ceilings the user
//! sets for these used to be counted in memory only, so every restart -
//! and every update, which is a restart - started the count over and the
//! automation could spend the whole allowance a second time. This ledger
//! lives on disk next to the exe, is written the moment a paid step goes
//! out, and counts by the game server's day, not the PC clock: the
//! allowances renew at the server's midnight, and a laptop that travels
//! must not get a second helping.

use std::{collections::BTreeMap, path::PathBuf};

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

const FILE: &str = "daily_spend.json";

/// A paid step that counts against a daily ceiling
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spend {
    DungeonMushroom,
    ArenaMushroom,
    ShopReroll,
}

/// One character's paid steps for one server day
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize,
)]
pub struct DaySpend {
    /// The server day the counts belong to
    pub day: Option<NaiveDate>,
    #[serde(default)]
    pub dungeon: u8,
    #[serde(default)]
    pub arena: u8,
    #[serde(default)]
    pub rerolls: u8,
    /// Mushrooms gone from the purse today, whoever spent them - every
    /// drop between two looks at the character adds up here. Gains do
    /// not take anything back off: the day's budget is what may go out
    #[serde(default)]
    pub mushrooms: u32,
    /// The purse at the last look, so the next look can tell a drop
    #[serde(default)]
    pub mushrooms_seen: Option<u32>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SpendLedger {
    /// `ProbeHistory::key` -> today's counts
    #[serde(default)]
    chars: BTreeMap<String, DaySpend>,
}

impl SpendLedger {
    fn path() -> PathBuf {
        crate::app_path(FILE)
    }

    /// The ledger on disk, or an empty one when there is none yet. A file
    /// that is there but unreadable is logged and left alone - the app
    /// then counts from zero for the day, which is the pre-ledger
    /// behaviour, not worse
    #[must_use]
    pub fn load() -> Self {
        let path = Self::path();
        if !path.exists() {
            return Self::default();
        }
        match crate::storage::read_with_backup(&path, |s| {
            serde_json::from_str::<Self>(s).map_err(|e| e.to_string())
        }) {
            Ok((ledger, _)) => ledger,
            Err(e) => {
                log::error!(
                    "daily spend ledger unreadable, counting from zero: {e}"
                );
                Self::default()
            }
        }
    }

    fn save(&self) {
        match serde_json::to_string(self) {
            Ok(s) => {
                // A failed save is reported by `storage` - the counts in
                // memory still hold for this run
                let _ =
                    crate::storage::write_atomic(&Self::path(), s.as_bytes());
            }
            Err(e) => log::error!("daily spend ledger: {e}"),
        }
    }

    /// Today's counts for a character. A new server day starts them over
    #[must_use]
    pub fn today(&mut self, key: &str, day: NaiveDate) -> DaySpend {
        let entry = self.chars.entry(key.to_string()).or_default();
        if entry.day != Some(day) {
            *entry = DaySpend {
                day: Some(day),
                ..DaySpend::default()
            };
        }
        *entry
    }

    /// Counts one paid step, without touching the disk (tests, and the
    /// half of `bump` that matters)
    pub fn record(
        &mut self,
        key: &str,
        day: NaiveDate,
        what: Spend,
    ) -> DaySpend {
        let mut now = self.today(key, day);
        match what {
            Spend::DungeonMushroom => {
                now.dungeon = now.dungeon.saturating_add(1)
            }
            Spend::ArenaMushroom => now.arena = now.arena.saturating_add(1),
            Spend::ShopReroll => now.rerolls = now.rerolls.saturating_add(1),
        }
        self.chars.insert(key.to_string(), now);
        now
    }

    /// Counts one paid step and writes the ledger out right away - the
    /// step is on its way to the server, a crash after this must not
    /// forget it
    pub fn bump(&mut self, key: &str, day: NaiveDate, what: Spend) -> DaySpend {
        let now = self.record(key, day, what);
        self.save();
        now
    }

    /// A look at the purse: `purse` mushrooms right now. A drop since the
    /// last look is spent today; a rise is a gain and changes nothing.
    /// Returns what has gone out today. Written to disk only when the
    /// count moved, so the frequent polls cost nothing
    pub fn note_mushrooms(&mut self, key: &str, day: NaiveDate, purse: u32) -> u32 {
        let mut now = self.today(key, day);
        let mut moved = false;
        if let Some(seen) = now.mushrooms_seen
            && purse < seen
        {
            now.mushrooms = now.mushrooms.saturating_add(seen - purse);
            moved = true;
        }
        if now.mushrooms_seen != Some(purse) {
            now.mushrooms_seen = Some(purse);
            moved = true;
        }
        if moved {
            self.chars.insert(key.to_string(), now);
            if now.mushrooms > 0 {
                self.save();
            }
        }
        now.mushrooms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, d).unwrap()
    }

    #[test]
    fn counts_survive_a_reload_within_the_day() {
        let mut ledger = SpendLedger::default();
        ledger.record("s30#glandalf", day(12), Spend::DungeonMushroom);
        ledger.record("s30#glandalf", day(12), Spend::DungeonMushroom);
        ledger.record("s30#glandalf", day(12), Spend::ShopReroll);
        // A restart is a serialise / deserialise round trip
        let text = serde_json::to_string(&ledger).unwrap();
        let mut back: SpendLedger = serde_json::from_str(&text).unwrap();
        let today = back.today("s30#glandalf", day(12));
        assert_eq!((today.dungeon, today.arena, today.rerolls), (2, 0, 1));
        // Two paid fights are the ceiling of two: the third is refused by
        // the caller comparing against this
        assert!(today.dungeon >= 2);
    }

    #[test]
    fn a_new_server_day_resets_once() {
        let mut ledger = SpendLedger::default();
        ledger.record("k", day(12), Spend::ArenaMushroom);
        assert_eq!(ledger.today("k", day(13)).arena, 0);
        ledger.record("k", day(13), Spend::ArenaMushroom);
        assert_eq!(ledger.today("k", day(13)).arena, 1);
        // Yesterday does not come back
        assert_eq!(ledger.today("k", day(12)).arena, 0);
    }

    /// The mushroom count of the day is every drop of the purse added up:
    /// a gain in between (an expedition paid out) does not hide a later
    /// spend, and a new server day starts from the purse as it is then
    #[test]
    fn mushroom_drops_add_up_and_gains_change_nothing() {
        let mut ledger = SpendLedger::default();
        assert_eq!(ledger.note_mushrooms("k", day(12), 50), 0);
        assert_eq!(ledger.note_mushrooms("k", day(12), 45), 5);
        // A gain: nothing spent
        assert_eq!(ledger.note_mushrooms("k", day(12), 60), 5);
        assert_eq!(ledger.note_mushrooms("k", day(12), 40), 25);
        // Next day: the purse is the new floor
        assert_eq!(ledger.note_mushrooms("k", day(13), 40), 0);
        assert_eq!(ledger.note_mushrooms("k", day(13), 39), 1);
    }

    #[test]
    fn characters_do_not_share_a_count() {
        let mut ledger = SpendLedger::default();
        ledger.record("a", day(12), Spend::ShopReroll);
        assert_eq!(ledger.today("b", day(12)).rerolls, 0);
    }
}
