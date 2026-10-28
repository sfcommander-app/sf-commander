//! What actually happened, against what the simulation said would happen.
//!
//! The arena's odds come from `sf_api::simulate` run on the opponent's
//! visible gear and class. It is a good model and it is not the game: it
//! does not know about a rune the screen does not print, the exact order
//! the game rolls in, or the pet bonus behind the player. So "80 %" is an
//! opinion, and until somebody checks it against results it stays one -
//! which is why some opponents do not fall on the first try.
//!
//! Two records are kept, per character, in `fight_log.json`:
//!
//! * **by bucket** - the predicted chance rounded into tens (0-10 %,
//!   10-20 % ... 90-100 %) against how those fights actually went. After a
//!   couple of hundred fights this says things like "when the simulation
//!   says 80 %, you win 62 % of them", and the estimate is bent to match.
//!   This is Platt scaling for the poor: no library, no training step,
//!   just a tally per bucket with Laplace smoothing so a single result
//!   never reads as certainty
//! * **by opponent** - the same tally per name. Somebody you are 0 of 3
//!   against goes down whatever the simulation thinks of them, which is
//!   the guild leader's own observation: the model is wrong about
//!   *particular* people, not only on average
//!
//! The by-opponent half also absorbs the game's own combat log (the third
//! tab of the post, `gs.mail.combat_log`) - it arrives with an ordinary
//! poll, so the whole history back to whatever the game still keeps costs
//! no request at all. That list does not say who started the fight, so a
//! defense counts the same as an attack there; it is treated as the
//! weaker evidence it is and only ever nudges a number the live record
//! has not already settled.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sf_api::gamestate::social::{CombatLogEntry, CombatMessageType};

use crate::probe_history::Tally;

const FILE: &str = "fight_log.json";

/// Predicted chances are counted in tens
const BUCKETS: u8 = 10;

/// Fights in one bucket before the record outweighs the simulation. Eight
/// is a compromise: enough that a run of luck does not rewrite the model,
/// few enough that a character farming the arena all day is calibrated
/// within a couple of days
const BUCKET_TRUST: f64 = 8.0;

/// Fights against one opponent before their own record outweighs the
/// bucket. Two: "beaten by them twice" is already worth more than any
/// average
const FOE_TRUST: f64 = 2.0;

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct CharLog {
    /// Predicted-chance bucket (0 = 0-10 %, 9 = 90-100 %) -> how those
    /// fights went
    #[serde(default)]
    pub buckets: BTreeMap<u8, Tally>,
    /// Opponent name, lowercased -> how those fights went
    #[serde(default)]
    pub foes: BTreeMap<String, Tally>,
    /// The newest combat-log id already counted, so a poll that returns
    /// the same hundred entries does not count them again
    #[serde(default)]
    pub seen: i64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct FightLog {
    /// Char key (see `ProbeHistory::key`) -> record
    #[serde(default)]
    pub chars: BTreeMap<String, CharLog>,
    #[serde(skip)]
    dirty: bool,
}

impl FightLog {
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
        if let Ok(s) = serde_json::to_string_pretty(self) {
            _ = std::fs::write(crate::app_path(FILE), s);
        }
        self.dirty = false;
    }

    fn bucket(predicted: f64) -> u8 {
        ((predicted.clamp(0.0, 1.0) * f64::from(BUCKETS)) as u8)
            .min(BUCKETS - 1)
    }

    fn tally(t: &mut Tally, won: bool) {
        if won {
            t.wins = t.wins.saturating_add(1);
        } else {
            t.losses = t.losses.saturating_add(1);
        }
    }

    /// A fight the app itself sent, with the chance it believed at the
    /// time. Counts towards both records
    pub fn note(&mut self, key: &str, foe: &str, predicted: f64, won: bool) {
        let c = self.chars.entry(key.to_string()).or_default();
        Self::tally(c.buckets.entry(Self::bucket(predicted)).or_default(), won);
        Self::tally(c.foes.entry(foe.to_lowercase()).or_default(), won);
        self.dirty = true;
    }

    /// A fight with no prediction attached - from the game's own combat
    /// log, or a target the simulation had nothing to say about. Counts
    /// only towards the opponent's record
    pub fn note_result(&mut self, key: &str, foe: &str, won: bool) {
        let c = self.chars.entry(key.to_string()).or_default();
        Self::tally(c.foes.entry(foe.to_lowercase()).or_default(), won);
        self.dirty = true;
    }

    /// Everything new in the game's own combat log. Only arena fights:
    /// the fortress, the pets and the guild are other models entirely and
    /// a quest is not a fight against a player at all. Returns how many
    /// were new, so the caller can say so once
    pub fn absorb(&mut self, key: &str, log: &[CombatLogEntry]) -> usize {
        let seen = self.chars.get(key).map_or(0, |c| c.seen);
        let mut newest = seen;
        let mut taken = 0;
        for e in log {
            if e.msg_id <= seen {
                continue;
            }
            newest = newest.max(e.msg_id);
            if !matches!(e.battle_type, CombatMessageType::Arena) {
                continue;
            }
            if e.player_name.is_empty() {
                continue;
            }
            self.note_result(key, &e.player_name, e.won);
            taken += 1;
        }
        if newest > seen {
            self.chars.entry(key.to_string()).or_default().seen = newest;
            self.dirty = true;
        }
        taken
    }

    /// The chance to actually win, `raw` being what the simulation says.
    ///
    /// The bucket bends the number towards what fights of that confidence
    /// really did; the opponent's own record, if there is one, bends it
    /// further. With no record at all this returns `raw` unchanged, so a
    /// fresh install behaves exactly as before
    #[must_use]
    pub fn calibrated(&self, key: &str, foe: &str, raw: f64) -> f64 {
        let Some(c) = self.chars.get(key) else {
            return raw;
        };
        let mut out = raw;
        if let Some(t) = c.buckets.get(&Self::bucket(raw)) {
            let n = f64::from(t.attempts());
            let w = n / (n + BUCKET_TRUST);
            out = out * (1.0 - w) + t.estimate() * w;
        }
        if let Some(t) = c.foes.get(&foe.to_lowercase()) {
            let n = f64::from(t.attempts());
            if n >= FOE_TRUST {
                let w = n / (n + FOE_TRUST);
                out = out * (1.0 - w) + t.estimate() * w;
            }
        }
        out.clamp(0.0, 1.0)
    }

    /// How much the record has to say, for the panel: fights counted and
    /// opponents known
    #[must_use]
    pub fn samples(&self, key: &str) -> (u32, usize) {
        let Some(c) = self.chars.get(key) else {
            return (0, 0);
        };
        let fights =
            c.buckets.values().map(|t| u32::from(t.attempts())).sum::<u32>();
        (fights, c.foes.len())
    }

    /// One line for the live feed when the record disagrees with the
    /// simulation by enough to matter. Without a "Thinking:" in front -
    /// the feed puts that there itself (`push_think`), and both of them
    /// doing it came out as "Thinking: Thinking: ..."
    #[must_use]
    pub fn disagreement(
        &self,
        key: &str,
        foe: &str,
        raw: f64,
    ) -> Option<String> {
        let cal = self.calibrated(key, foe, raw);
        if (cal - raw).abs() < 0.08 {
            return None;
        }
        let c = self.chars.get(key)?;
        let against = c.foes.get(&foe.to_lowercase()).filter(|t| t.attempts() >= 2);
        Some(match against {
            Some(t) => format!(
                "the simulation says {:.0} % against {foe}, but you \
                 are {}-{} against them - taking it as {:.0} %",
                raw * 100.0,
                t.wins,
                t.losses,
                cal * 100.0
            ),
            None => format!(
                "the simulation says {:.0} % here, and fights like \
                 it have gone {:.0} % - taking it as {:.0} %",
                raw * 100.0,
                cal * 100.0,
                cal * 100.0
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const K: &str = "s29.eu#gussolean";

    #[test]
    fn an_empty_record_changes_nothing() {
        let log = FightLog::default();
        assert!((log.calibrated(K, "Nobody", 0.8) - 0.8).abs() < 1e-9);
        assert_eq!(log.samples(K), (0, 0));
    }

    /// The simulation says 80 % and the fights come back 50-50: the
    /// estimate slides down towards what really happens, and does not
    /// jump there on the first result
    #[test]
    fn the_bucket_bends_the_estimate_towards_what_really_happened() {
        let mut log = FightLog::default();
        let first = {
            log.note(K, "A", 0.8, false);
            log.calibrated(K, "B", 0.8)
        };
        assert!(first > 0.7, "one loss must not rewrite the model: {first}");
        for i in 0..40 {
            log.note(K, &format!("x{i}"), 0.8, i % 2 == 0);
        }
        let after = log.calibrated(K, "B", 0.8);
        assert!(after < 0.62, "{after}");
        assert!(after > 0.5, "{after}");
        // A bucket nobody has fought in is untouched
        assert!((log.calibrated(K, "B", 0.2) - 0.2).abs() < 1e-9);
        let (fights, foes) = log.samples(K);
        assert_eq!(fights, 41);
        assert_eq!(foes, 41);
    }

    /// Beaten twice by the same player: they go down whatever the
    /// simulation thinks, and the name is matched however it is spelled
    #[test]
    fn an_opponent_who_keeps_winning_is_believed_over_the_simulation() {
        let mut log = FightLog::default();
        log.note(K, "K3roo", 0.9, false);
        log.note(K, "k3roo", 0.9, false);
        let theirs = log.calibrated(K, "K3ROO", 0.9);
        let others = log.calibrated(K, "Somebody", 0.9);
        assert!(theirs < 0.6, "{theirs}");
        assert!(theirs < others, "{theirs} vs {others}");
    }

    /// The game's own combat log is counted once, however often it is
    /// polled, and only the arena part of it
    #[test]
    fn the_combat_log_is_absorbed_once() {
        use chrono::Local;
        let entry = |id: i64, name: &str, won: bool, typ: CombatMessageType| {
            CombatLogEntry {
                msg_id: id,
                player_name: name.to_string(),
                won,
                battle_type: typ,
                time: Local::now(),
            }
        };
        let log_entries = vec![
            entry(10, "Obdyt", true, CombatMessageType::Arena),
            entry(11, "Krysps", false, CombatMessageType::Arena),
            entry(12, "elPh1l", true, CombatMessageType::FortressFight),
        ];
        let mut log = FightLog::default();
        assert_eq!(log.absorb(K, &log_entries), 2);
        assert_eq!(log.absorb(K, &log_entries), 0);
        assert_eq!(log.samples(K), (0, 2));
        // The fortress fight is somebody else's model - not counted
        assert!(log.chars[K].foes.get("elph1l").is_none());
        // A newer entry still comes in
        let more = vec![entry(13, "Obdyt", false, CombatMessageType::Arena)];
        assert_eq!(log.absorb(K, &more), 1);
        assert_eq!(log.chars[K].foes["obdyt"].attempts(), 2);
    }
}
