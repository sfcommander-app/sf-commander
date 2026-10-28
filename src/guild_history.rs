//! The game only ever shows the guild as it is right now. To judge members by
//! how they *move* - levels gained lately, battles they actually signed up
//! for - the app keeps its own diary of every guild refresh in
//! `guild_history.json` next to the config.

use std::collections::BTreeMap;

use chrono::{DateTime, Local, NaiveDate};
use serde::{Deserialize, Serialize};
use sf_api::gamestate::guild::{BattlesJoined, Guild};

const FILE: &str = "guild_history.json";

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct MemberHistory {
    /// Level at the end of each day we saw the member
    #[serde(default)]
    pub levels: BTreeMap<NaiveDate, u16>,
    /// Every battle we saw planned, keyed "A:<unix>" / "D:<unix>", with the
    /// last known sign-up state. The last observation before the battle
    /// counts, because people sign up late
    #[serde(default)]
    pub battles: BTreeMap<String, bool>,
    /// For each day we watched: did the member take their swing at the
    /// guild portal? The game only keeps the last swing's time, so the
    /// "how often" the leader asks for is our diary's to answer. A day the
    /// portal was already dead (0 % life) is not held against anybody
    #[serde(default)]
    pub portal: BTreeMap<NaiveDate, bool>,
    /// When this member first turned up, for members who turned up while
    /// the diary was already running. The game blocks a fresh member from
    /// the guild's battles for a day, so a battle inside that day is one
    /// they *could not* join - counting it as a miss is the diary
    /// punishing somebody for the game's own rule.
    ///
    /// `None` for the founding cohort: they were here before the diary,
    /// so there is no first day of theirs to protect
    #[serde(default)]
    pub joined_at: Option<DateTime<Local>>,
}

/// How long the game keeps a fresh member out of the guild's battles
pub const JOIN_GRACE_HOURS: i64 = 24;

/// One raid the guild went into, and how it ended
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RaidRecord {
    /// Raid number (finished raids + 1 at the time)
    pub number: u16,
    /// Our fighting mass when it happened (see `guild_fight`)
    pub power: f64,
    pub won: bool,
    pub when: DateTime<Local>,
    /// What the app gave it when it was declared, 0..1, with the whole
    /// guild counted. Kept with the result so the figure is still there
    /// to read once the raid is over - it used to vanish with the raid,
    /// and "what were our odds on the one we just lost" is the question
    /// the leader asks first
    #[serde(default)]
    pub chance: Option<f64>,
    /// The same at the last look before the fight, counting only the
    /// members signed up, and how many that was. The whole-guild figure
    /// is the ceiling; this is what actually went in
    #[serde(default)]
    pub signed_chance: Option<f64>,
    #[serde(default)]
    pub signed_up: Option<usize>,
}

/// A raid that is declared but not fought yet
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PendingRaid {
    pub number: u16,
    pub power: f64,
    pub date: DateTime<Local>,
    /// The estimate as it stood when the raid was declared
    #[serde(default)]
    pub chance: Option<f64>,
    /// The estimate from the members signed up, refreshed on every look
    /// while the raid is pending - the last one is what went in
    #[serde(default)]
    pub signed_chance: Option<f64>,
    #[serde(default)]
    pub signed_up: Option<usize>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct GuildHistory {
    /// guild key (server + guild id) -> member name (lowercase) -> history
    #[serde(default)]
    pub guilds: BTreeMap<String, BTreeMap<String, MemberHistory>>,
    /// guild key -> raids fought, oldest first
    #[serde(default)]
    pub raids: BTreeMap<String, Vec<RaidRecord>>,
    /// guild key -> declared raid we still wait for
    #[serde(default)]
    pub pending_raids: BTreeMap<String, PendingRaid>,
    /// guild key -> member (lowercase) -> when they were greeted in the
    /// chat. Kept so a restart, or a second owner character in the same
    /// guild, never says hello twice
    #[serde(default)]
    pub greeted: BTreeMap<String, BTreeMap<String, DateTime<Local>>>,
    /// The same for the goodbye when somebody leaves
    #[serde(default)]
    pub farewelled: BTreeMap<String, BTreeMap<String, DateTime<Local>>>,
    #[serde(skip)]
    dirty: bool,
}

/// What the diary can say about the next raid
pub struct RaidEstimate {
    /// 0.0 - 1.0, None while no raid was watched yet
    pub chance: Option<f64>,
    /// Where the number comes from, in words
    pub basis: String,
    /// The mass the raid is reckoned to need, in the units of `power` -
    /// so a caller can ask the same question of a different mass, such
    /// as the members actually signed up
    pub needed: Option<f64>,
}

pub struct Progress {
    /// Levels gained in the last 7 days (None = not tracked long enough)
    pub levels_7d: Option<u16>,
    pub levels_30d: Option<u16>,
    /// (signed up, battles observed)
    pub battles: (usize, usize),
    /// How many distinct days we have seen this member
    pub days_seen: usize,
    /// The first day our diary ever saw this member. Days since then is their
    /// time in the guild - exact for anyone who joined after we started
    /// watching, a lower bound for anyone already there when we began
    pub first_seen: Option<NaiveDate>,
    /// (days the portal was fought, days watched) over the last
    /// `PORTAL_DAYS` days - today included
    pub portal: (usize, usize),
    /// `battles`, but the two kinds kept apart: (signed, seen) for
    /// attacks and for defenses. The diary keys them "A:" and "D:", so
    /// this costs nothing to work out and a member who turns up for every
    /// defense but never for an attack is no longer hidden behind one
    /// combined share
    pub attacks: (usize, usize),
    pub defenses: (usize, usize),
}

/// How many days back the portal share looks
pub const PORTAL_DAYS: i64 = 14;

/// The name in a "somebody joined the guild" line of the chat - the game
/// writes one the moment a member is accepted, in the client's language,
/// which is the fastest "new member" signal there is. `None` for every
/// other line
pub fn joined_name(line: &str) -> Option<String> {
    const PHRASES: [&str; 6] = [
        " vstoupil do cechu", " vstoupila do cechu", " has joined the guild",
        " joined the guild", " ist der Gilde beigetreten",
        " dołączył do gildii",
    ];
    let line = line.trim();
    PHRASES.iter().find_map(|p| {
        let (name, _) = line.split_once(p)?;
        Some(bare_name(name)?.to_string())
    })
}

/// The name in a "somebody left the guild" line of the chat; `None` for
/// every other line. Kicks and leaves read the same to the chat
pub fn left_name(line: &str) -> Option<String> {
    const PHRASES: [&str; 10] = [
        " opustil cech", " opustila cech", " byl vyhozen z cechu",
        " byla vyhozena z cechu", " has left the guild", " left the guild",
        " was kicked from the guild", " has been kicked from the guild",
        " hat die Gilde verlassen", " opuścił gildię",
    ];
    let line = line.trim();
    PHRASES.iter().find_map(|p| {
        let (name, _) = line.split_once(p)?;
        Some(bare_name(name)?.to_string())
    })
}

/// The name out of the front of a system line. The Czech client writes
/// "Hráč Xerpo opustil cech." - with the "Player" word in front - so that
/// word is dropped before the "no spaces in a name" check, which used to
/// throw the whole line away and no goodbye ever went out
fn bare_name(raw: &str) -> Option<&str> {
    let mut name = raw.trim().trim_start_matches(['§', ':']).trim();
    for prefix in ["Hráč ", "Hráčka ", "Player ", "Spieler ", "Gracz "] {
        if let Some(rest) = name.strip_prefix(prefix) {
            name = rest.trim();
        }
    }
    (!name.is_empty() && !name.contains(' ')).then_some(name)
}

/// Two battle keys closer than this are the same battle. The date of a
/// planned battle drifts by a second or two between refreshes (it is
/// converted through the server time difference, which is re-measured every
/// time), so the same raid used to land in the diary two or three times -
/// "0/15" for seven real battles. Battles are hours apart, never seconds
const SAME_BATTLE_SECS: i64 = 120;

impl MemberHistory {
    /// The key under which battle `kind` (`'A'` / `'D'`) at `ts` is kept:
    /// an existing key within `SAME_BATTLE_SECS`, or a fresh one
    fn battle_key(&self, kind: char, ts: i64) -> String {
        let prefix = format!("{kind}:");
        self.battles
            .keys()
            .filter_map(|k| {
                let old: i64 = k.strip_prefix(&prefix)?.parse().ok()?;
                ((old - ts).abs() <= SAME_BATTLE_SECS).then(|| k.clone())
            })
            .next()
            .unwrap_or_else(|| format!("{prefix}{ts}"))
    }

    /// Folds keys that drifted apart into one - the copies a diary written
    /// before the fix above already holds. Signed up in any copy counts:
    /// the copies were the same battle, and a sign-up seen once was there
    fn merge_battle_copies(&mut self) -> bool {
        let mut merged: BTreeMap<String, bool> = BTreeMap::new();
        let mut last: Option<(char, i64, String)> = None;
        // BTreeMap order is by string, which for one prefix and unix
        // timestamps of equal length is chronological
        for (k, signed) in &self.battles {
            let parsed = k.split_once(':').and_then(|(p, t)| {
                Some((p.chars().next()?, t.parse::<i64>().ok()?))
            });
            let Some((kind, ts)) = parsed else {
                merged.insert(k.clone(), *signed);
                continue;
            };
            match &last {
                Some((lk, lts, key))
                    if *lk == kind && (ts - lts).abs() <= SAME_BATTLE_SECS =>
                {
                    let e = merged.entry(key.clone()).or_insert(false);
                    *e = *e || *signed;
                }
                _ => {
                    merged.insert(k.clone(), *signed);
                    last = Some((kind, ts, k.clone()));
                }
            }
        }
        let changed = merged.len() != self.battles.len();
        self.battles = merged;
        changed
    }
}

impl GuildHistory {
    pub fn load() -> Self {
        // Beside the exe, with the old working-directory copy as a fallback so
        // the diary survives the update that moves it
        let mut h: Self = std::fs::read_to_string(crate::app_path(FILE))
            .or_else(|_| std::fs::read_to_string(FILE))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        for members in h.guilds.values_mut() {
            for m in members.values_mut() {
                if m.merge_battle_copies() {
                    h.dirty = true;
                }
            }
        }
        h
    }

    pub fn save(&mut self) {
        if !self.dirty {
            return;
        }
        // `dirty` only clears on a write that went through: a disk that
        // refused the file used to be forgotten as "saved"
        match serde_json::to_string(self) {
            Ok(s) => {
                if crate::storage::write_atomic(
                    &crate::app_path(FILE),
                    s.as_bytes(),
                )
                .is_ok()
                {
                    self.dirty = false;
                }
            }
            Err(e) => log::error!("guild history: {e}"),
        }
    }

    pub fn key(server_url: &str, guild: &Guild) -> String {
        format!("{}#{}", crate::get_server_code(server_url), guild.id)
    }

    /// Watches raids: remembers a declared one with our strength, and once
    /// its time is over checks whether the raid counter moved. Returns
    /// `Some((raid number, won))` on the one refresh that settles a pending
    /// raid, so the caller can announce the result; `None` otherwise
    pub fn record_raid(
        &mut self,
        key: &str,
        guild: &Guild,
        now: DateTime<Local>,
    ) -> Option<(u16, bool)> {
        let power = crate::guild_fight::own_power(guild);
        let mut settled = None;
        if let Some(pending) = self.pending_raids.get(key).cloned()
            && now > pending.date + chrono::Duration::minutes(3)
            && guild
                .attacking
                .as_ref()
                .is_none_or(|b| !b.is_raid() || b.date != pending.date)
        {
            let won = guild.finished_raids >= pending.number;
            let list = self.raids.entry(key.to_string()).or_default();
            if !list.iter().any(|r| r.when == pending.date) {
                list.push(RaidRecord {
                    number: pending.number,
                    power: pending.power,
                    won,
                    when: pending.date,
                    chance: pending.chance,
                    signed_chance: pending.signed_chance,
                    signed_up: pending.signed_up,
                });
                if list.len() > 60 {
                    list.remove(0);
                }
                settled = Some((pending.number, won));
            }
            self.pending_raids.remove(key);
            self.dirty = true;
        }
        if let Some(b) = guild.attacking.as_ref()
            && b.is_raid()
        {
            let number = guild.finished_raids + 1;
            let fresh = self
                .pending_raids
                .get(key)
                .is_none_or(|p| p.date != b.date || p.number != number);
            // The whole-guild odds are taken once, when the raid is
            // declared; the as-signed odds follow the sign-ups on every
            // look, so the record ends up holding the last state before
            // the fight
            let est = self.raid_estimate(key, number, power);
            let (signed_power, signed_up) =
                crate::guild_fight::signed_power(guild);
            let signed_chance = est
                .needed
                .map(|n| crate::guild_fight::win_chance(signed_power, n));
            if fresh {
                self.pending_raids.insert(
                    key.to_string(),
                    PendingRaid {
                        number,
                        power,
                        date: b.date,
                        chance: est.chance,
                        signed_chance,
                        signed_up: Some(signed_up),
                    },
                );
                self.dirty = true;
            } else if let Some(p) = self.pending_raids.get_mut(key)
                && (p.signed_up != Some(signed_up)
                    || p.signed_chance != signed_chance)
            {
                p.signed_chance = signed_chance;
                p.signed_up = Some(signed_up);
                self.dirty = true;
            }
        }
        settled
    }

    /// The raid fought most recently, with the odds it was given. A record
    /// from before the odds were written down gets them worked out again
    /// from what the book held at the time
    #[must_use]
    pub fn last_raid(&self, key: &str) -> Option<RaidRecord> {
        let last = self.raids.get(key)?.iter().max_by_key(|r| r.when)?;
        let mut out = last.clone();
        if out.chance.is_none() {
            out.chance = self
                .raid_estimate_before(key, out.number, out.power, Some(out.when))
                .chance;
        }
        Some(out)
    }

    /// Odds for raid `number` with fighting mass `power` now. The game does
    /// not show raid monsters, so the only teacher is the past: a lost raid
    /// tells us what was not enough, a won one what was
    pub fn raid_estimate(
        &self,
        key: &str,
        number: u16,
        power: f64,
    ) -> RaidEstimate {
        self.raid_estimate_before(key, number, power, None)
    }

    /// The same, from the raids fought before `before` only - the estimate
    /// as it stood at that moment. A raid recorded before the odds were
    /// kept with it gets its odds back this way: from what was in the
    /// book when it was declared, not from its own result
    pub fn raid_estimate_before(
        &self,
        key: &str,
        number: u16,
        power: f64,
        before: Option<DateTime<Local>>,
    ) -> RaidEstimate {
        use crate::i18n::t;
        let Some(all) = self.raids.get(key) else {
            return RaidEstimate {
                chance: None,
                basis: t("no raid watched yet").into(),
                needed: None,
            };
        };
        let earlier: Vec<&RaidRecord> = all
            .iter()
            .filter(|r| before.is_none_or(|b| r.when < b))
            .collect();
        let list = &earlier;
        // What was not enough for exactly this raid
        let lost = list
            .iter()
            .filter(|r| r.number == number && !r.won)
            .map(|r| r.power)
            .fold(None, |m: Option<f64>, p| Some(m.map_or(p, |m| m.max(p))));
        // What was enough for the previous raid (the next is a bit harder)
        let won_prev = list
            .iter()
            .filter(|r| r.number + 1 == number && r.won)
            .map(|r| r.power)
            .fold(None, |m: Option<f64>, p| Some(m.map_or(p, |m| m.min(p))));
        let won_same = list.iter().any(|r| r.number == number && r.won);
        let (needed, basis) = match (lost, won_prev) {
            (Some(l), Some(w)) => (
                l.max(w) * 1.08,
                format!(
                    "{} {number} {}, {} {} {}",
                    t("raid"),
                    t("was lost before"),
                    t("raid"),
                    number - 1,
                    t("was won")
                ),
            ),
            (Some(l), None) => (
                l * 1.1,
                format!("{} {number} {}", t("raid"), t("was lost before")),
            ),
            (None, Some(w)) => (
                w * 1.15,
                format!("{} {} {}", t("raid"), number - 1, t("was won")),
            ),
            (None, None) if won_same => (
                power * 0.8,
                format!("{} {number} {}", t("raid"), t("was won before")),
            ),
            (None, None) => {
                return RaidEstimate {
                    chance: None,
                    basis: format!(
                        "{} {}, {} {number}",
                        list.len(),
                        t("raids watched"),
                        t("none near raid")
                    ),
                    needed: None,
                };
            }
        };
        RaidEstimate {
            chance: Some(crate::guild_fight::win_chance(power, needed)),
            basis,
            needed: Some(needed),
        }
    }

    /// Takes one snapshot of the guild. Cheap enough to call on every refresh
    pub fn record(&mut self, key: &str, guild: &Guild, now: DateTime<Local>) {
        let today = now.date_naive();
        let entry = self.guilds.entry(key.to_string()).or_default();
        let attack = guild.attacking.as_ref().map(|b| b.date.timestamp());
        let defense = guild.defending.as_ref().map(|b| b.date.timestamp());

        // A dead portal cannot be fought - a day it is dead is not a day
        // anybody skipped it
        let portal_alive = guild.portal.life_percentage > 0;

        // Was this guild in the diary before this snapshot? On the very
        // first run every member looks new, and stamping the lot of them
        // with a join time would hand the whole guild a day of grace
        // nobody has earned
        let diary_running = !entry.is_empty();

        for m in &guild.members {
            let h = entry.entry(m.name.to_lowercase()).or_default();
            // Never seen before, and the diary was already running: this
            // is their first day and the game will not let them fight yet
            if diary_running && h.joined_at.is_none() && h.levels.is_empty() {
                h.joined_at = Some(now);
                self.dirty = true;
            }
            // Nothing is written down about a member for their first day.
            // The game keeps them out of the guild's battles for
            // twenty-four hours, so everything the diary could record in
            // that window is a duty they were not allowed to do - the
            // leader's own words: Franta joins, a day passes, and only
            // then do the statistics start. `joined_at` keeps the arrival
            // itself, so the table still knows how long they have been
            // here
            let counts = h.joined_at.is_none_or(|j| {
                now - j >= chrono::Duration::hours(JOIN_GRACE_HOURS)
            });
            if !counts {
                continue;
            }
            if h.levels.get(&today) != Some(&m.level) {
                h.levels.insert(today, m.level);
                self.dirty = true;
            }
            // The last swing's day is a fought day, whenever it was; today
            // is a watched day only while the portal stands
            if let Some(day) = m.portal_fought.map(|t| t.date_naive())
                && h.portal.get(&day) != Some(&true)
            {
                h.portal.insert(day, true);
                self.dirty = true;
            }
            if portal_alive && !h.portal.contains_key(&today) {
                h.portal.insert(today, false);
                self.dirty = true;
            }
            let in_attack = matches!(
                m.battles_joined,
                Some(BattlesJoined::Attack | BattlesJoined::Both)
            );
            let in_defense = matches!(
                m.battles_joined,
                Some(BattlesJoined::Defense | BattlesJoined::Both)
            );
            // **A battle is judged by when the battle is, not by when we
            // happen to be looking.** The gate above only asks whether
            // the member is still inside their first day *now* - so a
            // battle that fell inside their protected day was written
            // down as a miss as soon as the day was over, which is how
            // somebody who joined yesterday already had one against them.
            // The game would not let them in; it is not a duty they
            // skipped
            let allowed_from = h.joined_at.map(|j| {
                (j + chrono::Duration::hours(JOIN_GRACE_HOURS)).timestamp()
            });
            let could_fight =
                |ts: i64| allowed_from.is_none_or(|from| ts >= from);
            // Anything already written down inside that window was
            // written by the old rule and is wrong about them. Dropped
            // here rather than in a migration, so a diary that has been
            // running the whole time heals itself on the next poll
            let before = h.battles.len();
            h.battles.retain(|k, _| {
                k.split_once(':')
                    .and_then(|(_, t)| t.parse::<i64>().ok())
                    .is_none_or(&could_fight)
            });
            if h.battles.len() != before {
                self.dirty = true;
            }
            if let Some(ts) = attack.filter(|ts| could_fight(*ts)) {
                let k = h.battle_key('A', ts);
                if h.battles.get(&k) != Some(&in_attack) {
                    h.battles.insert(k, in_attack);
                    self.dirty = true;
                }
            }
            if let Some(ts) = defense.filter(|ts| could_fight(*ts)) {
                let k = h.battle_key('D', ts);
                if h.battles.get(&k) != Some(&in_defense) {
                    h.battles.insert(k, in_defense);
                    self.dirty = true;
                }
            }
        }

        // Members who left take their diary with them after a while - keep
        // the file from growing forever. Battles go the same way: the
        // sign-up share is about the recent past, not a member's whole life
        let cutoff = today - chrono::Duration::days(120);
        let battle_cutoff = (now - chrono::Duration::days(120)).timestamp();
        for h in entry.values_mut() {
            h.levels.retain(|d, _| *d >= cutoff);
            h.portal.retain(|d, _| *d >= cutoff);
            let before = h.battles.len();
            h.battles.retain(|k, _| {
                k.split_once(':')
                    .and_then(|(_, t)| t.parse::<i64>().ok())
                    .is_none_or(|t| t >= battle_cutoff)
            });
            if h.battles.len() != before {
                self.dirty = true;
            }
        }
    }

    /// Members (lowercase) who turned up today and have not been greeted:
    /// first seen today, and after the diary itself began - on the diary's
    /// first day the whole guild is "new", and greeting fifty people who
    /// have been here for years is not a welcome. Never anyone the diary
    /// already saw on an earlier day
    pub fn newcomers(&self, key: &str, now: DateTime<Local>) -> Vec<String> {
        let today = now.date_naive();
        let Some(guild) = self.guilds.get(key) else {
            return Vec::new();
        };
        let track_start = guild
            .values()
            .filter_map(|h| h.levels.keys().next().copied())
            .min();
        let greeted = self.greeted.get(key);
        guild
            .iter()
            .filter(|(name, h)| {
                let first = h.levels.keys().next().copied();
                first == Some(today)
                    && track_start.is_some_and(|t| t < today)
                    && !greeted.is_some_and(|g| g.contains_key(*name))
            })
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// The first day the diary saw `name` in this guild
    pub fn first_seen(&self, key: &str, name: &str) -> Option<NaiveDate> {
        self.guilds
            .get(key)?
            .get(&name.to_lowercase())?
            .levels
            .keys()
            .next()
            .copied()
    }

    /// The last day the diary saw `name` in this guild
    pub fn last_seen(&self, key: &str, name: &str) -> Option<NaiveDate> {
        self.guilds
            .get(key)?
            .get(&name.to_lowercase())?
            .levels
            .keys()
            .next_back()
            .copied()
    }

    pub fn is_greeted(&self, key: &str, name: &str) -> bool {
        self.greeted
            .get(key)
            .is_some_and(|g| g.contains_key(&name.to_lowercase()))
    }

    pub fn mark_greeted(
        &mut self,
        key: &str,
        name: &str,
        now: DateTime<Local>,
    ) {
        self.greeted
            .entry(key.to_string())
            .or_default()
            .insert(name.to_lowercase(), now);
        self.dirty = true;
    }

    pub fn is_farewelled(&self, key: &str, name: &str) -> bool {
        self.farewelled
            .get(key)
            .is_some_and(|g| g.contains_key(&name.to_lowercase()))
    }

    pub fn mark_farewelled(
        &mut self,
        key: &str,
        name: &str,
        now: DateTime<Local>,
    ) {
        self.farewelled
            .entry(key.to_string())
            .or_default()
            .insert(name.to_lowercase(), now);
        self.dirty = true;
    }

    pub fn progress(
        &self,
        key: &str,
        name: &str,
        now: DateTime<Local>,
    ) -> Progress {
        let today = now.date_naive();
        let Some(h) = self
            .guilds
            .get(key)
            .and_then(|g| g.get(&name.to_lowercase()))
        else {
            return Progress {
                levels_7d: None,
                levels_30d: None,
                battles: (0, 0),
                days_seen: 0,
                first_seen: None,
                portal: (0, 0),
                attacks: (0, 0),
                defenses: (0, 0),
            };
        };
        let portal_since = today - chrono::Duration::days(PORTAL_DAYS - 1);
        let portal_days: Vec<bool> =
            h.portal.range(portal_since..).map(|(_, f)| *f).collect();
        let portal = (
            portal_days.iter().filter(|f| **f).count(),
            portal_days.len(),
        );
        let days_seen = h.levels.len();
        let latest = h.levels.values().next_back().copied();
        // During the first day nothing is recorded (see `record`), so the
        // diary would say "never seen" about somebody standing right
        // there. The arrival itself is kept, and that is their first day
        let first_seen = h
            .levels
            .keys()
            .next()
            .copied()
            .or_else(|| h.joined_at.map(|j| j.date_naive()));

        // Level at (or before) `days` ago; None when we do not reach back that
        // far - then the gain can not be known
        let gain = |days: i64| -> Option<u16> {
            let since = today - chrono::Duration::days(days);
            let tracked_long_enough = first_seen.is_some_and(|f| f <= since);
            if !tracked_long_enough {
                return None;
            }
            let base = h.levels.range(..=since).next_back().map(|(_, l)| *l)?;
            Some(latest?.saturating_sub(base))
        };

        let signed = h.battles.values().filter(|s| **s).count();
        // The two kinds apart, by the key's prefix
        let of_kind = |p: char| {
            let seen = h.battles.keys().filter(|k| k.starts_with(p)).count();
            let went = h
                .battles
                .iter()
                .filter(|(k, s)| k.starts_with(p) && **s)
                .count();
            (went, seen)
        };
        Progress {
            levels_7d: gain(7),
            levels_30d: gain(30),
            battles: (signed, h.battles.len()),
            days_seen,
            first_seen,
            portal,
            attacks: of_kind('A'),
            defenses: of_kind('D'),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// "v guild seznamu memberu to neukazuje jaka byla procentualni sance
    /// na najezd ... kterej uz probehl" - the odds stay with the raid
    #[test]
    fn the_odds_a_raid_went_in_on_are_still_there_after_it() {
        let now = Local::now();
        let mut h = GuildHistory::default();
        let key = "s#9";
        // The book: raid 51 was won at mass 1000, raid 52 lost at 1050
        let book = h.raids.entry(key.into()).or_default();
        for (number, power, won, days) in
            [(51u16, 1000.0, true, 5i64), (52, 1050.0, false, 3)]
        {
            book.push(RaidRecord {
                number,
                power,
                won,
                when: now - chrono::Duration::days(days),
                chance: None,
                signed_chance: None,
                signed_up: None,
            });
        }
        // Raid 52 declared again at mass 1100: the estimate is taken now
        let mut guild = Guild::default();
        guild.finished_raids = 51;
        let before = h.raid_estimate(key, 52, 1100.0).chance;
        assert!(before.is_some(), "the book has raid 52 in it");
        h.pending_raids.insert(
            key.into(),
            PendingRaid {
                number: 52,
                power: 1100.0,
                date: now - chrono::Duration::hours(2),
                chance: before,
                signed_chance: None,
                signed_up: None,
            },
        );
        // It is fought and lost: the counter did not move, the battle is gone
        guild.attacking = None;
        let settled = h.record_raid(key, &guild, now);
        assert_eq!(settled, Some((52, false)));
        let last = h.last_raid(key).expect("a raid was fought");
        assert_eq!(last.number, 52);
        assert!(!last.won);
        assert_eq!(last.chance, before, "the odds changed after the raid");
    }

    /// A raid recorded before the odds were written down gets them back
    /// from what the book held when it was declared - not from its own
    /// result, which would make every lost raid look like a bad bet
    #[test]
    fn an_old_record_gets_its_odds_from_the_book_as_it_stood() {
        let now = Local::now();
        let mut h = GuildHistory::default();
        let key = "s#9";
        let book = h.raids.entry(key.into()).or_default();
        book.push(RaidRecord {
            number: 51,
            power: 1000.0,
            won: true,
            when: now - chrono::Duration::days(5),
            chance: None,
            signed_chance: None,
            signed_up: None,
        });
        book.push(RaidRecord {
            number: 52,
            power: 1100.0,
            won: false,
            when: now - chrono::Duration::days(1),
            chance: None,
            signed_chance: None,
            signed_up: None,
        });
        let last = h.last_raid(key).expect("a raid was fought");
        let as_it_stood = h
            .raid_estimate_before(key, 52, 1100.0, Some(last.when))
            .chance;
        let with_hindsight = h.raid_estimate(key, 52, 1100.0).chance;
        assert!(as_it_stood.is_some());
        assert_eq!(last.chance, as_it_stood);
        assert_ne!(
            last.chance, with_hindsight,
            "the lost raid was judged by its own loss"
        );
    }

    /// "kdyz se z 49 ... 10 neprihlasi, tak je ta sance mensi zejo" - yes,
    /// and now the number says so: the same raid, the same book, ten
    /// fewer signed up, a lower figure
    #[test]
    fn ten_members_not_signed_up_is_a_lower_chance() {
        use sf_api::gamestate::guild::{BattlesJoined, GuildMemberData};
        let now = Local::now();
        let mut h = GuildHistory::default();
        let key = "s#9";
        h.raids.entry(key.into()).or_default().push(RaidRecord {
            number: 28,
            power: 49.0 * 200f64.powi(2),
            won: true,
            when: now - chrono::Duration::days(3),
            chance: None,
            signed_chance: None,
            signed_up: None,
        });
        let mut guild = Guild::default();
        guild.finished_raids = 28;
        for i in 0..49 {
            guild.members.push(GuildMemberData {
                name: format!("m{i}"),
                level: 200,
                battles_joined: (i < 39).then_some(BattlesJoined::Attack),
                ..Default::default()
            });
        }
        let all = crate::guild_fight::own_power(&guild);
        let (signed, n) = crate::guild_fight::signed_power(&guild);
        assert_eq!(n, 39);
        let est = h.raid_estimate(key, 29, all);
        let needed = est.needed.expect("raid 28 is on the book");
        let whole = est.chance.expect("an estimate");
        let as_signed = crate::guild_fight::win_chance(signed, needed);
        assert!(as_signed < whole, "{as_signed} is not below {whole}");
        // A tenth of the guild missing is not a rounding error
        assert!(whole - as_signed > 0.05);
    }

    fn with(keys: &[(&str, bool)]) -> MemberHistory {
        MemberHistory {
            battles: keys.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
            ..Default::default()
        }
    }

    /// The portal share counts fought days against watched days in the
    /// window - a dead-portal day is neither, an old last swing is a
    /// fought day even before the diary began
    #[test]
    fn the_portal_share_comes_from_watched_days() {
        use sf_api::gamestate::guild::GuildMemberData;
        let now = Local::now();
        let today = now.date_naive();
        let mut h = GuildHistory::default();
        let mut guild = Guild::default();
        guild.portal.life_percentage = 80;
        guild.members.push(GuildMemberData {
            name: "Ann".to_string(),
            portal_fought: Some(now - chrono::Duration::days(3)),
            ..Default::default()
        });
        guild.members.push(GuildMemberData {
            name: "Bob".to_string(),
            portal_fought: Some(now),
            ..Default::default()
        });
        h.record("k", &guild, now);
        // Ann: fought three days ago, not today -> 1 of 2 watched days
        assert_eq!(h.progress("k", "ann", now).portal, (1, 2));
        assert_eq!(h.progress("k", "bob", now).portal, (1, 1));
        // The portal dies later today: Ann's day stays a miss, a newcomer
        // seen only now gets no watched day at all
        guild.portal.life_percentage = 0;
        guild.members.push(GuildMemberData {
            name: "Cid".to_string(),
            ..Default::default()
        });
        h.record("k", &guild, now);
        assert_eq!(h.progress("k", "ann", now).portal, (1, 2));
        assert_eq!(h.progress("k", "cid", now).portal, (0, 0));
        // A swing later today turns the miss into a hit
        guild.members[0].portal_fought = Some(now);
        h.record("k", &guild, now);
        assert_eq!(h.progress("k", "ann", now).portal, (2, 2));
        let _ = today;
    }

    /// The same battle seen a second later must land on the key it already
    /// has - that drift is what turned seven battles into "0/15"
    #[test]
    fn a_battle_drifting_by_seconds_keeps_its_key() {
        let h = with(&[("A:1789001299", false), ("D:1789001299", true)]);
        assert_eq!(h.battle_key('A', 1789001300), "A:1789001299");
        assert_eq!(h.battle_key('A', 1789001299 - 90), "A:1789001299");
        // Another kind is another battle, and hours apart is another too
        assert_eq!(h.battle_key('D', 1789001300), "D:1789001299");
        assert_eq!(h.battle_key('A', 1789001299 + 3600), "A:1789004899");
    }

    #[test]
    fn the_join_line_of_the_chat_names_the_newcomer() {
        assert_eq!(
            joined_name("PSYCHO vstoupil do cechu."),
            Some("PSYCHO".to_string())
        );
        assert_eq!(
            joined_name("§Ania vstoupila do cechu."),
            Some("Ania".to_string())
        );
        assert_eq!(
            joined_name("Bob has joined the guild."),
            Some("Bob".to_string())
        );
        assert_eq!(joined_name("Tammy opustil cech."), None);
        assert_eq!(joined_name("hello :) and welcome"), None);
        assert_eq!(joined_name(" vstoupil do cechu."), None);
        assert_eq!(left_name("Tammy opustil cech."), Some("Tammy".to_string()));
        assert_eq!(
            left_name("Hráč Xerpo opustil cech."),
            Some("Xerpo".to_string())
        );
        assert_eq!(
            left_name("MUNDI byl vyhozen z cechu."),
            Some("MUNDI".to_string())
        );
        assert_eq!(
            left_name("Bob has left the guild."),
            Some("Bob".to_string())
        );
        assert_eq!(left_name("PSYCHO vstoupil do cechu."), None);
        assert_eq!(
            joined_name("Hráč Xerpo vstoupil do cechu."),
            Some("Xerpo".to_string())
        );
    }

    /// Only somebody who appeared after the diary started, today, and was
    /// not greeted yet is a newcomer - the founding cohort never is
    #[test]
    fn newcomers_are_todays_arrivals_after_the_diary_began() {
        let now = Local::now();
        let today = now.date_naive();
        let yesterday = today - chrono::Duration::days(1);
        let mut h = GuildHistory::default();
        let g = h.guilds.entry("s#1".into()).or_default();
        let member = |days: &[NaiveDate]| MemberHistory {
            levels: days.iter().map(|d| (*d, 100u16)).collect(),
            ..Default::default()
        };
        g.insert("old".into(), member(&[yesterday, today]));
        g.insert("fresh".into(), member(&[today]));
        g.insert("also fresh".into(), member(&[today]));
        assert_eq!(h.newcomers("s#1", now).len(), 2);
        h.mark_greeted("s#1", "Fresh", now);
        assert_eq!(h.newcomers("s#1", now), vec!["also fresh".to_string()]);

        // A diary that started today knows nobody as new
        let mut day_one = GuildHistory::default();
        let g = day_one.guilds.entry("s#2".into()).or_default();
        g.insert("a".into(), member(&[today]));
        g.insert("b".into(), member(&[today]));
        assert!(day_one.newcomers("s#2", now).is_empty());
    }

    /// A diary written before the fix still holds the copies; they fold
    /// into one on load, and a sign-up seen in any copy survives
    #[test]
    fn old_copies_are_merged_on_load() {
        let mut h = with(&[
            ("A:1788859447", false),
            ("A:1788859448", true),
            ("A:1788901308", false),
            ("A:1788901309", false),
            ("A:1789001299", true),
            ("A:1789001300", false),
            ("A:1789001301", false),
            ("D:1789001300", true),
        ]);
        assert!(h.merge_battle_copies());
        assert_eq!(h.battles.len(), 4);
        assert_eq!(h.battles.get("A:1788859447"), Some(&true));
        assert_eq!(h.battles.get("A:1788901308"), Some(&false));
        assert_eq!(h.battles.get("A:1789001299"), Some(&true));
        assert_eq!(h.battles.get("D:1789001300"), Some(&true));
        // Second pass changes nothing
        assert!(!h.merge_battle_copies());
    }
}

#[cfg(test)]
mod grace_tests {
    use super::*;
    use sf_api::gamestate::guild::{Guild, GuildMemberData, PlanedBattle};

    fn guild_with(names: &[&str]) -> Guild {
        let mut g = Guild::default();
        g.members = names
            .iter()
            .map(|n| GuildMemberData {
                name: (*n).to_string(),
                level: 100,
                ..Default::default()
            })
            .collect();
        g.portal.life_percentage = 100;
        g
    }

    /// The game keeps a fresh member out of the guild's battles for a day.
    /// Nothing about them is written down in that window - a battle they
    /// were not allowed to join must never read as a battle they skipped
    #[test]
    fn a_members_first_day_is_not_counted_against_them() {
        let start = Local::now() - chrono::Duration::hours(48);
        let mut h = GuildHistory::default();
        let mut guild = guild_with(&["oldtimer"]);
        // Day one of the diary: the founding cohort is not "new"
        h.record("k", &guild, start);
        assert!(h.guilds["k"]["oldtimer"].joined_at.is_none());

        // Franta joins, and a battle is planned right away
        guild.members.push(GuildMemberData {
            name: "franta".to_string(),
            level: 100,
            ..Default::default()
        });
        let battle = start + chrono::Duration::hours(1);
        guild.attacking = Some(PlanedBattle {
            date: battle,
            ..Default::default()
        });
        h.record("k", &guild, battle);
        let franta = &h.guilds["k"]["franta"];
        assert!(franta.joined_at.is_some(), "the arrival itself is kept");
        assert!(franta.battles.is_empty(), "no battle in the first day");
        assert!(franta.portal.is_empty(), "no portal day either");
        assert!(franta.levels.is_empty());
        // ...while everybody else is counted as usual
        assert_eq!(h.guilds["k"]["oldtimer"].battles.len(), 1);

        // The app still knows he is here, and for how long
        let p = h.progress("k", "franta", battle);
        assert_eq!(p.first_seen, Some(battle.date_naive()));
        assert_eq!(p.battles, (0, 0));

        // A day later the statistics start
        let after = start + chrono::Duration::hours(26);
        guild.attacking = Some(PlanedBattle {
            date: after,
            ..Default::default()
        });
        h.record("k", &guild, after);
        let franta = &h.guilds["k"]["franta"];
        assert_eq!(franta.battles.len(), 1, "counted once the day is up");
        assert!(!franta.levels.is_empty());
    }

    /// The bug behind "he came yesterday and already has a missed battle":
    /// the window was checked against *now*, so a battle that fell inside
    /// the protected day was written down the moment the day was over.
    /// A battle is judged by when the battle is
    #[test]
    fn a_battle_inside_the_protected_day_never_counts_later() {
        let start = Local::now() - chrono::Duration::hours(72);
        let mut h = GuildHistory::default();
        let mut guild = guild_with(&["oldtimer"]);
        h.record("k", &guild, start);

        // Leapay joins, and a battle is fought six hours later - inside
        // the day the game keeps them out of it
        guild.members.push(GuildMemberData {
            name: "leapay".to_string(),
            level: 100,
            ..Default::default()
        });
        let joined = start + chrono::Duration::hours(1);
        h.record("k", &guild, joined);
        let during = joined + chrono::Duration::hours(6);
        guild.attacking = Some(PlanedBattle {
            date: during,
            ..Default::default()
        });
        h.record("k", &guild, during);
        assert!(h.guilds["k"]["leapay"].battles.is_empty());

        // Two days on, the app looks again while that same battle is
        // still the one the game reports. It must still not count
        let later = joined + chrono::Duration::hours(48);
        h.record("k", &guild, later);
        assert!(
            h.guilds["k"]["leapay"].battles.is_empty(),
            "a battle they were locked out of is not a battle they missed"
        );

        // A battle after the protected day counts as normal
        guild.attacking = Some(PlanedBattle {
            date: later,
            ..Default::default()
        });
        h.record("k", &guild, later);
        assert_eq!(h.guilds["k"]["leapay"].battles.len(), 1);
    }

    /// A diary that already wrote one down under the old rule heals
    /// itself on the next poll rather than needing a migration
    #[test]
    fn a_miss_recorded_by_the_old_rule_is_dropped() {
        let start = Local::now() - chrono::Duration::hours(72);
        let joined = start + chrono::Duration::hours(1);
        let mut h = GuildHistory::default();
        let mut guild = guild_with(&["oldtimer"]);
        h.record("k", &guild, start);
        guild.members.push(GuildMemberData {
            name: "leapay".to_string(),
            level: 100,
            ..Default::default()
        });
        h.record("k", &guild, joined);
        // Forge the entry the old rule would have left behind
        let inside = (joined + chrono::Duration::hours(3)).timestamp();
        h.guilds
            .get_mut("k")
            .unwrap()
            .get_mut("leapay")
            .unwrap()
            .battles
            .insert(format!("A:{inside}"), false);
        h.record("k", &guild, joined + chrono::Duration::hours(48));
        assert!(h.guilds["k"]["leapay"].battles.is_empty());
    }

    /// The very first snapshot must not hand the whole guild a day of
    /// grace - nobody there is new, the diary is
    #[test]
    fn the_diarys_own_first_day_gives_nobody_grace() {
        let now = Local::now();
        let mut h = GuildHistory::default();
        let mut guild = guild_with(&["a", "b"]);
        guild.attacking = Some(PlanedBattle {
            date: now,
            ..Default::default()
        });
        h.record("k", &guild, now);
        for name in ["a", "b"] {
            assert!(h.guilds["k"][name].joined_at.is_none(), "{name}");
            assert_eq!(h.guilds["k"][name].battles.len(), 1, "{name}");
        }
    }
}

#[cfg(test)]
mod chat_tests {
    use sf_api::gamestate::guild::ChatMessage;

    /// The guild chat carries two kinds of line and both belong in it:
    /// what somebody typed, and what the game says the guild did. The
    /// second kind has no time and no author, and the parser used to
    /// require both and drop everything else - so the app showed a
    /// handful of player messages and none of the level ups, portal hits,
    /// kicks or promotions, and the join/leave watcher never saw a thing
    #[test]
    fn the_games_own_chat_lines_are_kept() {
        let raw = "00:14 MokraAnia: no need for 10\
                   /Hrac KAMSON87 dosahl urovne 206.\
                   /Witek byl vyhozen z cechu.\
                   /00:15 Petr Macinka: raidu";
        let msgs = ChatMessage::parse_messages(raw);
        assert_eq!(msgs.len(), 4, "{msgs:?}");
        assert_eq!(msgs[0].user, "MokraAnia");
        assert!(msgs[0].time.is_some());
        assert_eq!(msgs[1].user, "");
        assert_eq!(msgs[1].time, None);
        assert_eq!(msgs[1].message, "Hrac KAMSON87 dosahl urovne 206.");
        assert_eq!(msgs[2].message, "Witek byl vyhozen z cechu.");
        assert_eq!(msgs[3].user, "Petr Macinka");
        // Empty pieces are not lines
        assert!(ChatMessage::parse_messages("//  //").is_empty());
    }

    /// ...and now that they arrive, the join and leave watcher can read
    /// them - it never had anything to read before
    #[test]
    fn a_leave_line_is_readable_once_it_arrives() {
        let msgs = ChatMessage::parse_messages("Witek byl vyhozen z cechu.");
        let line = format!("{} {}", msgs[0].user, msgs[0].message);
        assert_eq!(super::left_name(&line), Some("Witek".to_string()));
    }
}
