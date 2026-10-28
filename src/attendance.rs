//! How often the character actually turns up - and the misses that keep
//! it from being every single time.
//!
//! A bot signs every attack, every defense, swings at the portal every
//! day: a 100 % record over weeks, which no person has. This diary keeps
//! the record per character on disk (`attendance.json` next to the exe)
//! and hands out deliberate misses so the share settles just under the
//! target the safety slider sets (see `stealth::attend_target`). The
//! decision for one battle or one portal day is made once and remembered,
//! so a second look does not sign up what the first look skipped.

use std::{collections::BTreeMap, path::PathBuf};

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

const FILE: &str = "attendance.json";

/// How many past decisions of a kind the share is read from
pub const WINDOW: usize = 30;

/// A duty the diary keeps a record of
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Duty {
    Portal,
    Defense,
    Attack,
}

impl Duty {
    pub fn label(self) -> &'static str {
        match self {
            Duty::Portal => "portal",
            Duty::Defense => "defense",
            Duty::Attack => "attack",
        }
    }
}

/// One character's record: per duty, the decisions in order (true = went)
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct CharRecord {
    /// Portal days: date -> went
    #[serde(default)]
    pub portal: BTreeMap<NaiveDate, bool>,
    /// Battles keyed by their unix time -> went
    #[serde(default)]
    pub defense: BTreeMap<i64, bool>,
    #[serde(default)]
    pub attack: BTreeMap<i64, bool>,
}

impl CharRecord {
    /// (went, decided) over the last `WINDOW` decisions of `duty`
    #[must_use]
    pub fn tally(&self, duty: Duty) -> (usize, usize) {
        let values: Vec<bool> = match duty {
            Duty::Portal => self.portal.values().rev().take(WINDOW).copied().collect(),
            Duty::Defense => self.defense.values().rev().take(WINDOW).copied().collect(),
            Duty::Attack => self.attack.values().rev().take(WINDOW).copied().collect(),
        };
        (values.iter().filter(|v| **v).count(), values.len())
    }

    fn trim(&mut self) {
        while self.portal.len() > 2 * WINDOW {
            self.portal.pop_first();
        }
        while self.defense.len() > 2 * WINDOW {
            self.defense.pop_first();
        }
        while self.attack.len() > 2 * WINDOW {
            self.attack.pop_first();
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Attendance {
    /// `ProbeHistory::key` -> record
    #[serde(default)]
    chars: BTreeMap<String, CharRecord>,
}

impl Attendance {
    fn path() -> PathBuf {
        crate::app_path(FILE)
    }

    #[must_use]
    pub fn load() -> Self {
        let path = Self::path();
        if !path.exists() {
            return Self::default();
        }
        match crate::storage::read_with_backup(&path, |s| {
            serde_json::from_str::<Self>(s).map_err(|e| e.to_string())
        }) {
            Ok((a, _)) => a,
            Err(e) => {
                log::error!("attendance diary unreadable, starting over: {e}");
                Self::default()
            }
        }
    }

    fn save(&self) {
        match serde_json::to_string(self) {
            Ok(s) => {
                let _ = crate::storage::write_atomic(&Self::path(), s.as_bytes());
            }
            Err(e) => log::error!("attendance diary: {e}"),
        }
    }

    /// The record of one character, empty when unseen - used by the guild
    /// report later to show individual member attendance
    #[allow(dead_code)]
    #[must_use]
    pub fn record(&self, key: &str) -> CharRecord {
        self.chars.get(key).cloned().unwrap_or_default()
    }

    /// Every character's tallies for `duty`, summed
    #[must_use]
    pub fn total(&self, duty: Duty) -> (usize, usize) {
        self.chars.values().map(|r| r.tally(duty)).fold((0, 0), |a, b| {
            (a.0 + b.0, a.1 + b.1)
        })
    }

    /// Whether the character goes to this portal day / battle. Decided
    /// once per `when` and remembered: `target` is the share of times to
    /// go (1.0 never skips). Returns true to go
    pub fn decide(
        &mut self,
        key: &str,
        duty: Duty,
        when: i64,
        target: f64,
    ) -> bool {
        let rec = self.chars.entry(key.to_string()).or_default();
        let existing = match duty {
            Duty::Portal => rec.portal.get(&day_of(when)).copied(),
            Duty::Defense => rec.defense.get(&when).copied(),
            Duty::Attack => rec.attack.get(&when).copied(),
        };
        if let Some(went) = existing {
            return went;
        }
        let (went_n, seen) = rec.tally(duty);
        let go = !crate::stealth::skip_duty(went_n, seen, target);
        match duty {
            Duty::Portal => {
                rec.portal.insert(day_of(when), go);
            }
            Duty::Defense => {
                rec.defense.insert(when, go);
            }
            Duty::Attack => {
                rec.attack.insert(when, go);
            }
        }
        rec.trim();
        self.save();
        go
    }
}

/// The local day a unix time falls on
fn day_of(ts: i64) -> NaiveDate {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|t| t.with_timezone(&chrono::Local).date_naive())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A decision is made once per battle and kept; with a target of one
    /// nobody is ever skipped; with a target well under the record the
    /// misses come
    #[test]
    fn decisions_are_remembered_and_follow_the_target() {
        let mut a = Attendance::default();
        a.chars.clear();
        for i in 0..20 {
            assert!(a.decide_in_memory("k", Duty::Attack, 1_800_000_000 + i * 3600, 1.0));
        }
        assert_eq!(a.record("k").tally(Duty::Attack), (20, 20));
        // Asked again about the same battle: the same answer, no new entry
        assert!(a.decide_in_memory("k", Duty::Attack, 1_800_000_000, 0.0));
        assert_eq!(a.record("k").tally(Duty::Attack), (20, 20));
        // A record at 100 % and a target of 80 %: the next few go missing
        let mut skipped = 0;
        for i in 20..40 {
            if !a.decide_in_memory("k", Duty::Attack, 1_800_000_000 + i * 3600, 0.8) {
                skipped += 1;
            }
        }
        assert!(skipped >= 2, "skipped {skipped}");
        let (went, seen) = a.record("k").tally(Duty::Attack);
        assert_eq!(seen, WINDOW);
        assert!(went < seen);
    }

    impl Attendance {
        /// `decide` without the disk
        fn decide_in_memory(&mut self, key: &str, duty: Duty, when: i64, target: f64) -> bool {
            let rec = self.chars.entry(key.to_string()).or_default();
            let existing = match duty {
                Duty::Portal => rec.portal.get(&day_of(when)).copied(),
                Duty::Defense => rec.defense.get(&when).copied(),
                Duty::Attack => rec.attack.get(&when).copied(),
            };
            if let Some(went) = existing {
                return went;
            }
            let (went_n, seen) = rec.tally(duty);
            let go = !crate::stealth::skip_duty(went_n, seen, target);
            match duty {
                Duty::Portal => {
                    rec.portal.insert(day_of(when), go);
                }
                Duty::Defense => {
                    rec.defense.insert(when, go);
                }
                Duty::Attack => {
                    rec.attack.insert(when, go);
                }
            }
            rec.trim();
            go
        }
    }
}
