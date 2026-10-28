//! Keeping the traffic from reading like a script.
//!
//! What a server can see of a client is timing: how many requests a
//! minute, when the logins come, whether five characters from one address
//! move in lockstep. None of that is hidden by a user agent. So the
//! rules here are about time, and all of them are draws, never clocks:
//!
//! * a session is a stretch of play with a random length, and the next
//!   one comes after a random gap around the average the player set, not
//!   every three hours on the dot; some sessions are simply skipped, the
//!   way a person does not come home some evenings
//! * quiet hours: a stretch of the night with nothing at all, shifted by
//!   a fresh three quarters of an hour every day
//! * several characters never start together - each gets its own delay
//!   into a session, and no two log in again at the same minute after
//!   the server's midnight
//! * a ceiling on requests a minute per character, so a full queue never
//!   turns into a burst the game only ever sees from scripts
//!
//! Every draw is a plain function so it can be tested; the callers live
//! in `message.rs`.

use chrono::{NaiveDate, NaiveTime, Timelike};

/// What there is to do in the game right now - the app's own reading of
/// whether it is time to farm or time to put the feet up. Every flag is
/// one thing a player would log in for
#[derive(Debug, Clone, Copy, Default)]
pub struct NeedFacts {
    /// Thirst for adventure left and the character free to use it
    pub thirst: bool,
    /// Beers still to drink today
    pub beer_left: bool,
    /// An expedition is under way and wants finishing
    pub expedition_running: bool,
    /// The arena's free fight is up
    pub arena_free: bool,
    /// A dungeon's free fight is up
    pub dungeon_free: bool,
    /// A guild battle or raid within the next few hours
    pub battle_soon: bool,
    /// Something is waiting in the queue
    pub queued: bool,
}

/// How much there is to do, 0 (nothing at all - feet up) to 100. The
/// thirst weighs most: it is the one resource that ticks away whether it
/// is used or not; a running expedition next, because a half-done one is
/// the plainest sign of a player who walked away mid-click
#[must_use]
pub fn need_score(f: &NeedFacts) -> u8 {
    let mut n = 0u32;
    if f.thirst {
        n += 40;
    }
    if f.expedition_running {
        n += 30;
    }
    if f.beer_left {
        n += 15;
    }
    if f.arena_free {
        n += 15;
    }
    if f.dungeon_free {
        n += 15;
    }
    if f.battle_soon {
        n += 30;
    }
    if f.queued {
        n += 10;
    }
    n.min(100) as u8
}

/// Nothing worth logging in for
pub const NEED_NONE: u8 = 0;
/// Enough to come back soon, or stay a little longer
pub const NEED_BUSY: u8 = 50;

/// A random point in `lo..=hi` seconds, leaning towards the low end by
/// `skew` (1.0 flat, 2.0 clearly short-heavy, 3.0 mostly short). A
/// person's time for a game looks like this: many short moments, a few
/// long ones - never a flat spread, never a fixed split
fn skewed_secs(lo: i64, hi: i64, skew: f64) -> i64 {
    let u = fastrand::f64().powf(skew);
    lo + ((hi - lo) as f64 * u).round() as i64
}

/// A factor that changes the whole draw a little - so two days never
/// have the same rhythm even with the same settings: 0.8 to 1.25
fn mood() -> f64 {
    0.8 + fastrand::f64() * 0.45
}

/// What the one slider decides, for a position 1..=10 (0 is off).
///
/// The scale runs from "hardly plays" to "plays as much as the game's own
/// cooldowns allow" - and the two ends are not a trade between doing well
/// and hiding well, because those turn out to be the same thing. A bot
/// that sits in the game for two hours and then vanishes for eight both
/// *misses* most of what the game hands out on a clock and looks like
/// nothing a person does. Somebody who really plays this game checks in
/// for a few minutes, often, all day - which is how the timers are
/// actually collected and what the traffic of a player actually looks
/// like. So towards 10 the visits get shorter and far more frequent: more
/// collected, less to see.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Profile {
    /// Multiplier on a session's length
    pub length: f64,
    /// Multiplier on the requests-a-minute ceiling
    pub cap: f64,
}

/// The highest the slider goes. 0 is off, 1..=`MAX_LEVEL` is play
pub const MAX_LEVEL: u8 = 10;

/// Minutes between sessions on average, per position. Eight hours at 1 -
/// a look-in three times a day - down to a bit over half an hour at 10,
/// which is somebody with the game open on their phone
#[must_use]
pub fn gap_minutes(level: u8) -> u32 {
    match level.min(MAX_LEVEL) {
        0 | 1 => 480,
        2 => 360,
        3 => 240,
        4 => 180,
        5 => 120,
        6 => 90,
        7 => 70,
        8 => 55,
        9 => 45,
        _ => 36,
    }
}

/// The share of a duty to actually attend. One - every duty is attended.
///
/// This used to be one minus a "human miss share" (15 % of portals, 10 %
/// of defenses, 5 % of attacks, doubled on STEALTH): the bot skipped
/// guild duties on purpose so the record would not read as a machine.
/// It was the wrong trade. A missed portal looks the same from outside
/// whoever missed it, so it hides nothing - and it costs the reward, the
/// guild's battle and the member's own grade, in an app whose whole job
/// is to not leave anything on the table. The diary is still kept, so
/// the Settings page can show what was actually attended
#[must_use]
pub fn attend_target(
    _duty: crate::attendance::Duty,
    _profile: &Profile,
) -> f64 {
    1.0
}

/// Whether to skip a duty now, given `went` of `seen` past decisions and
/// the `target` share. With no record yet it is a plain draw; with one,
/// a skip comes when going would keep the record above the target -
/// mostly, not always, so the misses never fall on a schedule
#[must_use]
pub fn skip_duty(went: usize, seen: usize, target: f64) -> bool {
    if target >= 1.0 {
        return false;
    }
    if seen < 5 {
        return fastrand::f64() < 1.0 - target;
    }
    let if_go = (went as f64 + 1.0) / (seen as f64 + 1.0);
    if if_go > target {
        fastrand::f64() < 0.8
    } else {
        fastrand::f64() < 0.05
    }
}

impl Profile {
    /// The blend for a slider position 0..=10
    #[must_use]
    pub fn from_safety(level: u8) -> Self {
        let l = f64::from(level.clamp(1, MAX_LEVEL));
        // How long one visit runs. It climbs with the slider, but gently:
        // the gap between visits is what actually decides how much of the
        // day the account is online (eight hours apart at 1, a little
        // over half an hour at 10), and a session that ran for five hours
        // would be the one thing a person never does. So 1 is a short
        // look-in three times a day and 10 is a sitting of normal length
        // that comes round again soon - which is what "all day" looks
        // like from the outside.
        //
        // The ceiling on requests a minute comes down a little as the
        // visits multiply: twenty short visits that each send a burst is
        // a worse pattern than twenty that each send a handful
        let t = (l - 1.0) / 9.0;
        // 6.0: the bot plays continuously while it is on, so the slider
        // only says how hard: the request ceiling runs from seven tenths
        // of the player's number (STEALTH) to half again (NO SHAME)
        // 6.2.3: NO SHAME runs at up to twice the player's number (it was
        // half again - "it should be a bit faster")
        let cap = if l <= 5.0 {
            0.7 + 0.3 * (l - 1.0) / 4.0
        } else {
            1.0 + 1.0 * (l - 5.0) / 5.0
        };
        Profile {
            length: 0.7 + (1.4 - 0.7) * t,
            cap,
        }
    }

    /// How long the pauses between clicks run for a profile, in percent
    /// (see `pace::set_speed`): STEALTH a quarter slower, BALANCED as
    /// written, NO SHAME a little over half (6.2.3)
    #[must_use]
    pub fn speed_pct(level: u8) -> u8 {
        match Self::label(level) {
            "STEALTH" => 125,
            "NO SHAME" => 55,
            _ => 100,
        }
    }

    /// The slider's word for its position
    #[must_use]
    pub fn label(level: u8) -> &'static str {
        match level {
            0 => "OFF",
            1..=4 => "STEALTH",
            5..=7 => "BALANCED",
            _ => "NO SHAME",
        }
    }
}

/// Seconds to the next session, `hour` the local hour of day.
///
/// **The work waiting is what decides.** A stretch away is not a timer
/// the app sits out regardless: a player with a full thirst, free fights
/// up and a raid in an hour comes back soon, and one with nothing left to
/// click stays away for the evening. `need` is that reading, 0 to 100
/// (see `need_score`), and it cuts the stretch by up to nine tenths.
///
/// The hour bends it a little: leaving late at night means longer (the
/// night is the quiet hours' anyway), leaving mid-day shorter. Within
/// those bounds the draw is short-heavy and never the same twice.
///
/// There used to be a one-in-twelve "the whole day gone" branch of two
/// and a half to five times the average. On STEALTH, where the average
/// is eight hours and the profile stretches it again, that came out as
/// "next session in 33 h" - a day and a half in which nothing the game
/// hands out on a clock was ever collected. The stretch is capped at two
/// and a half times the average now, and nothing is drawn beyond it
/// The longest the app ever stays away, whatever the slider says: half a
/// day. Anything past that is not a stretch away, it is a day of the
/// game's clocks running for nobody
pub const GAP_CAP_SECS: i64 = 12 * 3600;

#[must_use]
pub fn session_gap_secs(avg_minutes: u32, need: u8, hour: u32) -> i64 {
    // The average is the whole of it: `avg_hours` already comes from the
    // slider as a ladder of one to eight (see `Config::hours_for`). It
    // used to be multiplied by `profile.gap` on top, so three fixed
    // averages were stretched again by the same knob that picked them -
    // which is why moving the slider twenty points often did nothing and
    // the next point tripled the wait
    let base = f64::from(avg_minutes.max(5)) * 60.0 * mood();
    // Nothing to do keeps the whole stretch; plenty to do cuts it to a
    // tenth. Never quite to nothing - back within the minute is the one
    // pattern no person has
    let pressure = (1.0 - 0.9 * f64::from(need.min(100)) / 100.0).max(0.1);
    let lo = (base * 0.5 * pressure) as i64;
    let hi = (base * 1.8 * pressure) as i64;
    let secs = skewed_secs(lo.max(5 * 60), hi.max(lo.max(5 * 60) + 60), 1.6);
    let by_hour = match hour {
        22..=23 | 0..=5 => 1.4,
        11..=15 => 0.8,
        _ => 1.0,
    };
    // Two ceilings, and the lower one wins. The first is relative - never
    // much more than the average the player set. The second is absolute:
    // half a day. Past that the quiet hours have already covered the
    // night, every timer the game runs has come round unused, and the
    // account has not been "away like a person" - it has been switched
    // off for a day
    let ceiling = ((base * 1.8) as i64).min(GAP_CAP_SECS);
    ((secs as f64 * by_hour) as i64).clamp(5 * 60, ceiling.max(10 * 60))
        + fastrand::i64(0..60)
}

/// A session that ran out while there was still plenty to do stays open
/// this much longer - "just one more thing"
#[must_use]
pub fn extend_minutes() -> i64 {
    fastrand::i64(5..=15)
}

/// The longest a session may grow to through extensions
pub const SESSION_CAP_MIN: i64 = 150;

/// How long a session sits with nothing to do before it is closed early -
/// a person with nothing left to click logs off
pub const IDLE_CLOSE_MIN: i64 = 8;

/// How long a session lasts, in seconds, `hour` the local hour of day.
/// Three kinds of visit, the shares of them drawn anew every time and
/// bent by the hour:
///
/// * a glance, 3 to 12 minutes - most of the day's visits
/// * a proper sit-down, 10 to 50 minutes
/// * a long evening, 40 minutes to a ceiling drawn between one and two
///   hours - mostly in the evening, rare at work
///
/// Inside each kind the draw is short-heavy, never flat. The result is
/// never within a sixth of the session before, so no two in a row look
/// alike, and never a whole minute
#[must_use]
pub fn session_length_secs(
    last: Option<i64>,
    hour: u32,
    profile: &Profile,
) -> i64 {
    // (glance, sit-down) percent; the rest is the long evening
    let (glance, sit) = match hour {
        18..=23 => (fastrand::u8(20..=35), fastrand::u8(30..=45)),
        8..=16 => (fastrand::u8(50..=65), fastrand::u8(25..=40)),
        _ => (fastrand::u8(35..=50), fastrand::u8(30..=45)),
    };
    loop {
        let roll = fastrand::u8(0..100);
        let drawn = if roll < glance {
            skewed_secs(3 * 60, 12 * 60, 1.4)
        } else if roll < glance + sit {
            skewed_secs(10 * 60, 50 * 60, 1.6)
        } else {
            let ceiling = fastrand::i64(60 * 60..=120 * 60);
            skewed_secs(40 * 60, ceiling, 1.5)
        };
        // The safety slider stretches or shortens every visit; never
        // under three minutes
        let secs = ((drawn as f64 * profile.length) as i64).max(3 * 60)
            + fastrand::i64(0..60);
        let alike = last.is_some_and(|l| (secs - l).abs() * 6 < l.max(secs));
        if !alike {
            return secs;
        }
    }
}

/// The pause before the next fortress attack, `avg_secs` the "about every
/// x minutes" the player set. "Every three minutes" on the dot was the
/// plainest clock in the whole log - a person attacks when they happen to
/// look, so this is a draw: 0.4 to 2.4 times the average, short-heavy
/// (the mean lands near the average), stretched by the safety slider's
/// gap and bent by the day's mood; one time in ten a long look-away of
/// 2.5 to 4 times. Never the same twice, never a whole minute
#[must_use]
#[allow(dead_code)]
pub fn fortress_gap_secs(avg_secs: u64) -> i64 {
    let avg = (avg_secs.max(30) as f64) * mood();
    let secs = if fastrand::u8(0..10) == 0 {
        skewed_secs((avg * 2.5) as i64, (avg * 4.0) as i64, 1.3)
    } else {
        skewed_secs((avg * 0.4) as i64, (avg * 2.4) as i64, 2.0)
    };
    secs.max(30) + fastrand::i64(0..45)
}

/// How long the raid waits after the last soldier is trained.
///
/// Drawn between three minutes and `max_min`, short-heavy - usually
/// within the first third, now and then the whole way - stretched by the
/// day's mood. A person notices the barracks are full when they happen
/// to look, and the moment they look is never the moment it happened
#[must_use]
pub fn raid_notice_secs(max_min: u32) -> i64 {
    let lo = crate::mechanics::NOTICE_MIN_SECS;
    let hi = (i64::from(max_min) * 60).max(lo + 60);
    let hi = ((hi as f64) * mood()) as i64;
    skewed_secs(lo, hi.max(lo + 60), 2.2) + fastrand::i64(0..45)
}

/// A character's own delay into a session, so several of them never
/// start in the same minute - drawn in seconds, up to a quarter of the
/// session and never over twenty minutes. It used to be a flat 0-20 min:
/// with an 11-minute session most of sixteen characters never got a
/// turn, and the feed sat empty for the whole visit
#[must_use]
pub fn stagger_secs(session_secs: i64) -> i64 {
    let cap = (session_secs / 4).clamp(0, 20 * 60);
    fastrand::i64(0..=cap)
}

/// The day's shift of the quiet hours: three quarters of an hour either
/// way, drawn once a day
#[must_use]
pub fn draw_shift() -> i64 {
    fastrand::i64(-45..=45)
}

/// Is `now` inside the quiet hours `from_h`..`to_h` (wall clock hours,
/// the window may run past midnight), moved by `shift_min` and grown by
/// `extend_min` at each end (the safety slider's doing)? A window of no
/// length is never quiet
#[must_use]
pub fn in_quiet(
    now: NaiveTime,
    from_h: u8,
    to_h: u8,
    shift_min: i64,
    extend_min: i64,
) -> bool {
    let start = i64::from(from_h.min(23)) * 60 + shift_min - extend_min;
    let end = i64::from(to_h.min(24)) * 60 + shift_min + extend_min;
    // The window's length: a window past midnight (23 to 6) has its end
    // before its start on the clock, which is what the modulo is for. A
    // window shrunk to nothing or below is no window
    let raw = end - start;
    let wraps = to_h < from_h;
    let len = if wraps { raw.rem_euclid(24 * 60) } else { raw };
    if len <= 0 {
        return false;
    }
    let now_min = i64::from(now.hour()) * 60 + i64::from(now.minute());
    (now_min - start).rem_euclid(24 * 60) < len
}

/// Requests a minute one character may send. The player's number at
/// human pace; full power is three times that, still a ceiling; the
/// safety slider moves it too
#[must_use]
pub fn request_cap(
    per_min: u8,
    pace: crate::pace::Pace,
    profile: &Profile,
) -> usize {
    let base = f64::from(per_min.max(1)) * profile.cap;
    let base = (base.round() as usize).max(1);
    match pace {
        crate::pace::Pace::FullPower => base * 3,
        _ => base,
    }
}

/// The minute past the server's midnight at which a character reads the
/// new day: 5 to 50, fixed for that character and that day - so a restart
/// does not move it, and no two characters share it
#[must_use]
pub fn relogin_minute(name: &str, day: NaiveDate) -> u32 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    name.to_lowercase().hash(&mut h);
    day.hash(&mut h);
    5 + (h.finish() % 46) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    /// The middle of the 0-10 slider
    fn balanced() -> Profile {
        Profile::from_safety(5)
    }

    #[test]
    fn quiet_hours_wrap_past_midnight_and_take_the_shift() {
        // 1:00 - 7:00
        assert!(in_quiet(at(3, 0), 1, 7, 0, 0));
        assert!(!in_quiet(at(0, 30), 1, 7, 0, 0));
        assert!(!in_quiet(at(7, 0), 1, 7, 0, 0));
        // Shifted 45 min later: 1:45 - 7:45
        assert!(!in_quiet(at(1, 30), 1, 7, 45, 0));
        assert!(in_quiet(at(7, 30), 1, 7, 45, 0));
        // Past midnight: 23:00 - 6:00
        assert!(in_quiet(at(23, 30), 23, 6, 0, 0));
        assert!(in_quiet(at(2, 0), 23, 6, 0, 0));
        assert!(!in_quiet(at(12, 0), 23, 6, 0, 0));
        // Shifted 45 min earlier: 22:15 - 5:15
        assert!(in_quiet(at(22, 30), 23, 6, -45, 0));
        assert!(!in_quiet(at(5, 30), 23, 6, -45, 0));
        // No window, never quiet
        assert!(!in_quiet(at(3, 0), 4, 4, 0, 0));
        // Grown an hour each end: 0:00 - 8:00; shrunk an hour: 2:00 - 6:00
        assert!(in_quiet(at(0, 30), 1, 7, 0, 60));
        assert!(in_quiet(at(7, 30), 1, 7, 0, 60));
        assert!(!in_quiet(at(1, 30), 1, 7, 0, -60));
        assert!(in_quiet(at(3, 0), 1, 7, 0, -60));
    }

    /// Everybody is in within the first quarter of the session, however
    /// short it is - and never later than twenty minutes into a long one
    #[test]
    fn the_stagger_fits_the_session() {
        assert!((0..200).all(|_| stagger_secs(11 * 60) <= 165));
        assert!((0..200).all(|_| stagger_secs(3 * 60) <= 45));
        assert!((0..200).all(|_| stagger_secs(2 * 3600) <= 20 * 60));
        assert!((0..200).any(|_| stagger_secs(2 * 3600) > 10 * 60));
    }

    /// "About every 3 minutes" is a spread around three minutes, never
    /// three minutes on the dot
    #[test]
    fn fortress_attacks_are_drawn_around_the_average() {
        let draws: Vec<i64> =
            (0..400).map(|_| fortress_gap_secs(180)).collect();
        let mean = draws.iter().sum::<i64>() / draws.len() as i64;
        assert!((100..=330).contains(&mean), "mean {mean}");
        assert!(
            draws.iter().any(|&d| d < 120) && draws.iter().any(|&d| d > 300)
        );
        assert!(draws.iter().all(|&d| d >= 30));
        let distinct: std::collections::HashSet<_> = draws.iter().collect();
        assert!(distinct.len() > 200);
    }

    /// The slider's eleven stops: 0 does nothing, and from 1 to 10 the
    /// visits get shorter while they come round more often. The far end
    /// is not the quiet end - it is the one that both collects most and
    /// looks most like a person
    #[test]
    fn the_slider_runs_from_off_to_all_day() {
        assert_eq!(Profile::label(0), "OFF");
        assert_eq!(Profile::label(1), "STEALTH");
        assert_eq!(Profile::label(5), "BALANCED");
        assert_eq!(Profile::label(10), "NO SHAME");
        // The wait between sessions shrinks the whole way up
        let gaps: Vec<u32> = (1..=MAX_LEVEL).map(gap_minutes).collect();
        assert!(gaps.windows(2).all(|w| w[0] > w[1]), "{gaps:?}");
        assert_eq!(gap_minutes(1), 480);
        assert_eq!(gap_minutes(MAX_LEVEL), 36);
        // ...while the visits themselves get a little longer, so that the
        // two pull the same way: more of the day online, the whole way up
        let low = Profile::from_safety(1);
        let high = Profile::from_safety(MAX_LEVEL);
        assert!(
            high.length > low.length,
            "{} vs {}",
            high.length,
            low.length
        );
        assert!(high.length < low.length * 3.0, "no marathon sittings");
        // 6.0: no sessions - the slider is how hard, so the ceiling
        // climbs from STEALTH to NO SHAME
        assert!(high.cap > low.cap);
        assert_eq!(request_cap(12, crate::pace::Pace::Human, &high), 24);
        assert_eq!(Profile::speed_pct(9), 55);
        assert_eq!(Profile::speed_pct(6), 100);
        assert_eq!(Profile::speed_pct(3), 125);
        assert_eq!(request_cap(12, crate::pace::Pace::Human, &low), 8);
        // A short look-in at 1, a normal sitting at 10
        let brief: i64 =
            (0..300).map(|_| session_length_secs(None, 12, &low)).sum();
        let settled: i64 =
            (0..300).map(|_| session_length_secs(None, 12, &high)).sum();
        assert!(settled > brief, "{settled} vs {brief}");
        // The share of the day online is what really moves: three quarters
        // of an hour every eight hours at 1, most of the day at 10
        let duty = |p: &Profile, level: u8| {
            let mean: f64 = (0..400)
                .map(|_| session_length_secs(None, 12, p) as f64)
                .sum::<f64>()
                / 400.0;
            mean / (mean + f64::from(gap_minutes(level)) * 60.0)
        };
        let idle = duty(&low, 1);
        let busy = duty(&high, MAX_LEVEL);
        assert!(busy > idle * 8.0, "{busy} vs {idle}");
        // Every duty is attended at every position
        use crate::attendance::Duty;
        for level in 0..=MAX_LEVEL {
            let p = Profile::from_safety(level);
            for duty in [Duty::Portal, Duty::Defense, Duty::Attack] {
                assert_eq!(attend_target(duty, &p), 1.0);
            }
        }
        assert!(!(0..200).any(|_| skip_duty(0, 0, 1.0)));
    }

    /// Coming back often is what collects what the game hands out on a
    /// clock - a cooldown that comes round while nobody is logged in is
    /// simply lost. That is the whole argument for the right-hand end of
    /// the slider, so it is worth a test: how many ten-minute arena
    /// cooldowns come round while somebody is actually in the game
    #[test]
    fn many_short_visits_beat_one_long_sitting() {
        const COOLDOWN_MIN: i64 = 10;
        let collected = |level: u8| -> i64 {
            let p = Profile::from_safety(level);
            let gap = gap_minutes(level);
            let mut minute = 0i64;
            let mut taken = 0;
            let mut next_free = 0i64;
            // A day of it, at noon so the hour bends nothing much
            while minute < 24 * 60 {
                let len = session_length_secs(None, 12, &p) / 60;
                let end = minute + len;
                while next_free <= end {
                    if next_free >= minute {
                        taken += 1;
                        next_free += COOLDOWN_MIN;
                    } else {
                        next_free = minute;
                    }
                }
                minute = end + i64::from(gap);
            }
            taken
        };
        let rare: i64 = (0..20).map(|_| collected(1)).sum();
        let often: i64 = (0..20).map(|_| collected(MAX_LEVEL)).sum();
        assert!(often > rare, "{often} vs {rare}");
    }

    /// The work waiting decides the stretch away: nothing to do keeps the
    /// whole of it, plenty to do cuts it to a fraction - and nothing is
    /// ever drawn past two and a half times the average, whatever the
    /// mood and the hour (that branch is what produced "next session in
    /// 33 h" on STEALTH)
    #[test]
    fn the_work_waiting_decides_the_stretch_away() {
        let mut seen = std::collections::HashSet::new();
        for _ in 0..600 {
            let s = session_gap_secs(180, NEED_NONE, 12);
            // Never under five minutes, never past 2.5 x 3 h x mood 1.25
            assert!((5 * 60..=GAP_CAP_SECS + 60).contains(&s), "{s}");
            seen.insert(s);
        }
        assert!(seen.len() > 550, "{}", seen.len());
        // Even at the far end of the slider nothing runs into the next day
        for _ in 0..600 {
            let s = session_gap_secs(480, NEED_NONE, 23);
            assert!(s <= GAP_CAP_SECS + 60, "{s}");
        }
        // The ladder is what decides the wait, one step at a time: eight
        // hours is clearly longer than four, four than one
        let mean = |m: u32| {
            (0..400)
                .map(|_| session_gap_secs(m, NEED_NONE, 12))
                .sum::<i64>()
                / 400
        };
        let (one, four, eight) = (mean(60), mean(240), mean(480));
        assert!(four > one * 3, "{four} vs {one}");
        assert!(eight > four, "{eight} vs {four}");
        // Idle days are long, busy ones are not
        let idle: i64 =
            (0..400).map(|_| session_gap_secs(180, NEED_NONE, 12)).sum();
        let busy: i64 = (0..400).map(|_| session_gap_secs(180, 100, 12)).sum();
        assert!(idle > busy * 5, "{idle} vs {busy}");
        // ...and a busy one still never comes back within the minute
        assert!((0..400).all(|_| session_gap_secs(180, 100, 12) >= 5 * 60));
        // Leaving at night stretches the gap
        let night: i64 =
            (0..600).map(|_| session_gap_secs(180, NEED_NONE, 23)).sum();
        let noon: i64 =
            (0..600).map(|_| session_gap_secs(180, NEED_NONE, 13)).sum();
        assert!(night > noon, "{night} vs {noon}");
    }

    #[test]
    fn the_need_adds_up() {
        assert_eq!(need_score(&NeedFacts::default()), 0);
        let thirsty = NeedFacts {
            thirst: true,
            ..Default::default()
        };
        assert_eq!(need_score(&thirsty), 40);
        let all = NeedFacts {
            thirst: true,
            beer_left: true,
            expedition_running: true,
            arena_free: true,
            dungeon_free: true,
            battle_soon: true,
            queued: true,
        };
        assert_eq!(need_score(&all), 100);
    }

    #[test]
    fn a_session_is_minutes_to_hours_and_never_repeats() {
        let mut last = None;
        let mut short = 0;
        let mut seen = std::collections::HashSet::new();
        for _ in 0..600 {
            let len = session_length_secs(last, 12, &balanced());
            // Never under three minutes, never a sitting nobody would
            // believe (the slider stretches the draw, so the top of the
            // band moves with it - at the middle position that is a
            // little over two hours)
            assert!((3 * 60..=150 * 60).contains(&len), "{len}");
            if let Some(l) = last {
                // At least a sixth apart from the one before
                assert!((len - l).abs() * 6 >= l.max(len), "{len} after {l}");
            }
            if len < 40 * 60 {
                short += 1;
            }
            seen.insert(len);
            last = Some(len);
        }
        // Short visits are the common case, and the draws do not pile up
        // on a few values (the glances share a narrow band, so a handful
        // of repeats among six hundred is what a real spread looks like)
        assert!(short > 250, "{short}");
        assert!(seen.len() > 420, "{}", seen.len());
        // Evenings run longer than office hours
        let evening: i64 = (0..600)
            .map(|_| session_length_secs(None, 20, &balanced()))
            .sum();
        let office: i64 = (0..600)
            .map(|_| session_length_secs(None, 10, &balanced()))
            .sum();
        assert!(evening > office, "{evening} vs {office}");
    }

    #[test]
    fn the_relogin_minute_is_fixed_per_character_and_day() {
        let day = NaiveDate::from_ymd_opt(2026, 9, 13).unwrap();
        let a = relogin_minute("Gussolean", day);
        assert_eq!(a, relogin_minute("gussolean", day));
        assert!((5..=50).contains(&a));
        let next = NaiveDate::from_ymd_opt(2026, 9, 14).unwrap();
        // Different days and names spread out (a collision is possible
        // but two in a row would be a broken hash)
        let others = [
            relogin_minute("Gussolean", next),
            relogin_minute("Petr Macinka", day),
        ];
        assert!(others.iter().any(|&m| m != a));
    }

    #[test]
    fn the_request_cap_follows_the_pace() {
        assert_eq!(request_cap(12, crate::pace::Pace::Human, &balanced()), 12);
        assert_eq!(
            request_cap(12, crate::pace::Pace::FullPower, &balanced()),
            36
        );
        assert_eq!(request_cap(0, crate::pace::Pace::Human, &balanced()), 1);
    }
}
