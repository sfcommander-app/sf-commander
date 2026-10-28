//! How fast the bot is allowed to go. "Human" spreads everything out with
//! random pauses like a person clicking through the game; "Full power" runs
//! the queue as fast as the game answers.

use std::{
    sync::atomic::{AtomicU8, Ordering},
    time::Duration,
};

use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize,
)]
pub enum Pace {
    #[default]
    Human,
    FullPower,
    /// The old third pace: sessions at human pace. Only still here so a
    /// config written by an older version deserializes; it is migrated to
    /// `Human` + `sessions_enabled` on load and never stored again
    Burst,
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

/// How long the human pauses run, in percent (6.2.3): set by the play
/// profile - STEALTH slower, BALANCED as written, NO SHAME about half.
/// Only the waits between clicks and the checkers' rounds move; the
/// game's own cooldowns and the per-minute ceiling still hold
static SPEED: AtomicU8 = AtomicU8::new(100);

pub fn set_speed(pct: u8) {
    SPEED.store(pct.clamp(30, 200), Ordering::Relaxed);
}

fn scaled(v: u64) -> u64 {
    v * u64::from(SPEED.load(Ordering::Relaxed)) / 100
}

pub fn set(p: Pace) {
    CURRENT.store(p as u8, Ordering::Relaxed);
}

pub fn get() -> Pace {
    match CURRENT.load(Ordering::Relaxed) {
        1 => Pace::FullPower,
        _ => Pace::Human,
    }
}

/// Seconds between two queued jobs of the same character. Human used to be
/// 15-45 s: with five characters taking turns that came to two jobs a
/// minute in total and a quarter of an hour in the queue for everything -
/// "lazy" was the word. A person clicking through their screens pauses
/// seconds, not most of a minute
pub fn account_gap() -> i64 {
    match get() {
        Pace::Human | Pace::Burst => {
            (scaled(fastrand::u64(5..=15)) as i64).max(2)
        }
        Pace::FullPower => 1,
    }
}

/// Seconds between queued jobs of different characters
pub fn global_gap() -> i64 {
    match get() {
        Pace::Human | Pace::Burst => {
            (scaled(fastrand::u64(2..=5)) as i64).max(1)
        }
        Pace::FullPower => 0,
    }
}

/// How often a checker looks again. Human keeps the base interval, full
/// power checks about three times as often (never below 15 s)
pub fn checker_secs(min_s: u64, max_s: u64) -> u64 {
    let base = fastrand::u64(min_s..=max_s.max(min_s));
    match get() {
        Pace::Human | Pace::Burst => scaled(base).max(10),
        Pace::FullPower => (base / 3).max(15),
    }
}

/// Sleep for a checker interval, pace-scaled
pub async fn sleep_checker(min_s: u64, max_s: u64) {
    tokio::time::sleep(Duration::from_secs(checker_secs(min_s, max_s))).await;
}

/// A pause between two steps of the same job (next expedition step, next
/// pet, next upgrade). The human variant is 1.5-2.5x longer than the base
pub async fn sleep_between(min_ms: u64, max_ms: u64) {
    let base = fastrand::u64(min_ms..=max_ms.max(min_ms));
    let ms = match get() {
        Pace::Human | Pace::Burst => {
            scaled(base * fastrand::u64(3..=5) / 2).max(300)
        }
        Pace::FullPower => base.min(400),
    };
    tokio::time::sleep(Duration::from_millis(ms)).await;
}

/// What decides whether the app may work right now, and in what order.
///
/// The order is the whole point, and it was wrong: the slider's left stop
/// was tested *before* "do it now", so pressing the button with the slider
/// at OFF started a countdown, put a green band across the screen saying
/// GOING THROUGH EVERYTHING, wrote "going through everything that is due"
/// into the feed - and sent nothing. The app said it was working and was
/// not.
///
/// "Do it now" is asked for by hand, with the player sitting there
/// watching. That is exactly the reason it already overrode the quiet
/// hours, and it is the same reason it has to override the slider: the
/// slider sets how often the app works *on its own*, and this is not on its
/// own. The switch for "touch nothing at all" is STOP, and STOP is checked
/// before any of this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    /// "Do it now" is running: it outranks the slider and the quiet hours
    GoingThroughIt,
    /// The slider is at its left stop, so nothing starts by itself
    SliderOff,
    /// Inside the quiet hours
    Quiet,
    /// Nothing in the way - it is up to the session schedule
    Schedule,
}

#[must_use]
pub fn gate(going_through_it: bool, slider_off: bool, quiet: bool) -> Gate {
    if going_through_it {
        Gate::GoingThroughIt
    } else if slider_off {
        Gate::SliderOff
    } else if quiet {
        Gate::Quiet
    } else {
        Gate::Schedule
    }
}

/// Seconds between two jobs while "do it now" is running.
///
/// The player pressed a button and is watching the feed, so the answer is
/// "often enough to look like it is working, slowly enough not to look like
/// a script": a handful of seconds, never the two-to-five that the ordinary
/// pace uses and never a stretch of quiet long enough to wonder whether it
/// has stopped
pub fn boost_gap() -> i64 {
    fastrand::i64(BOOST_GAP_MIN..=BOOST_GAP_MAX)
}

pub const BOOST_GAP_MIN: i64 = 5;
pub const BOOST_GAP_MAX: i64 = 10;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn do_it_now_outranks_the_slider_and_the_quiet_hours() {
        // The bug as the guild leader found it: slider at OFF, "do it now"
        // pressed, a countdown on screen and nothing going out
        assert_eq!(gate(true, true, false), Gate::GoingThroughIt);
        assert_eq!(gate(true, false, true), Gate::GoingThroughIt);
        assert_eq!(gate(true, true, true), Gate::GoingThroughIt);
    }

    #[test]
    fn without_it_the_slider_comes_first_and_then_the_hour() {
        assert_eq!(gate(false, true, true), Gate::SliderOff);
        assert_eq!(gate(false, true, false), Gate::SliderOff);
        assert_eq!(gate(false, false, true), Gate::Quiet);
        assert_eq!(gate(false, false, false), Gate::Schedule);
    }

    #[test]
    fn the_boost_pace_is_seconds_apart_not_minutes() {
        for _ in 0..200 {
            let g = boost_gap();
            assert!(
                (BOOST_GAP_MIN..=BOOST_GAP_MAX).contains(&g),
                "a job came {g} s after the last one"
            );
        }
        assert!(BOOST_GAP_MAX < 60, "a gap of a minute reads as stopped");
        assert!(BOOST_GAP_MIN >= 5, "faster than a person can click");
    }
}

#[cfg(test)]
mod do_it_now_tests {
    use super::*;

    /// The button wins over everything: at OFF, in the quiet hours, on
    /// any schedule. And the slider being moved is not one of the inputs
    /// - it has no say in whether a run goes on
    #[test]
    fn a_run_is_gated_by_nothing_but_its_own_button() {
        for slider_off in [false, true] {
            for quiet in [false, true] {
                assert_eq!(
                    gate(true, slider_off, quiet),
                    Gate::GoingThroughIt,
                    "slider_off={slider_off} quiet={quiet}"
                );
            }
        }
        assert_eq!(gate(false, true, true), Gate::SliderOff);
        assert_eq!(gate(false, false, true), Gate::Quiet);
        assert_eq!(gate(false, false, false), Gate::Schedule);
    }

    #[test]
    fn the_run_clicks_every_few_seconds() {
        for _ in 0..200 {
            let g = boost_gap();
            assert!((BOOST_GAP_MIN..=BOOST_GAP_MAX).contains(&g));
        }
    }
}
