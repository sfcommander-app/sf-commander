use std::sync::{Mutex, MutexGuard};

/// Locking a `Mutex` with `.unwrap()` panics as soon as *any* thread panicked
/// while holding that lock. Since this app runs with `panic = "abort"` in
/// release, that used to take the whole program down. The data behind these
/// locks is always in a usable state, so we simply recover from poisoning
/// instead of dying.
pub trait LockExt<T> {
    fn lock_safe(&self) -> MutexGuard<'_, T>;
}

impl<T> LockExt<T> for Mutex<T> {
    #[inline]
    fn lock_safe(&self) -> MutexGuard<'_, T> {
        match self.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                log::warn!("Recovered from a poisoned mutex");
                poisoned.into_inner()
            }
        }
    }
}

/// Backoff for retrying a failed request. Grows exponentially, is capped and
/// gets a bit of jitter, so that many characters do not hammer the server in
/// lockstep after a connection hiccup.
pub fn retry_delay(attempt: u64) -> std::time::Duration {
    const BASE_SECS: u64 = 3;
    const MAX_SECS: u64 = 300;

    let exp = BASE_SECS.saturating_mul(1u64 << attempt.min(7));
    let capped = exp.min(MAX_SECS);
    let jitter = fastrand::u64(0..=(capped / 2).max(1));
    std::time::Duration::from_secs(capped)
        + std::time::Duration::from_secs(jitter)
}

/// The server answers with short internal phrases. This turns the known ones
/// into something a person can act on
pub fn friendly_error(raw: &str) -> String {
    let lower = raw.to_lowercase();
    let known = [
        (
            "dungeon battle not free", "the free fight is not available yet",
        ),
        (
            "tower level is wrong", "the tower floor number was not accepted",
        ),
        (
            "cannot do this right now",
            "the game does not allow this right now",
        ),
        (
            "need more units", "the game wants more soldiers for this opponent",
        ),
        ("not enough", "not enough resources"),
        ("no fruit", "no fruit of that kind left"),
        ("already", "already done"),
        (
            "too many requests", "the server is rate limiting - slow down",
        ),
    ];
    for (needle, text) in known {
        if lower.contains(needle) {
            return text.to_string();
        }
    }
    raw.to_string()
}

/// Turns an API enum name into something a player would read:
/// `MinesOfGloria` -> `Mines of Gloria`, `TimeHonoredSchoolofMagic` ->
/// `Time Honored School of Magic`, `The13thFloor` -> `The 13th Floor`.
///
/// The game's own names glue the small words on in lower case
/// (`SchoolofMagic`, `TavernoftheDarkDoppelgangers`), so a plain split on
/// capitals is not enough - those have to be peeled off the end of a word
#[must_use]
pub fn split_camel(raw: &str) -> String {
    /// Written small when they are not the first word
    const SMALL: [&str; 7] = ["of", "the", "in", "and", "at", "for", "on"];
    /// The only ones the game actually glues onto the word before them.
    /// Keeping this list short matters: peeling "at" would turn
    /// "Cutthroat" into "Cutthro at", and "in" would ruin "AshMountain"
    const GLUED: [&str; 2] = ["of", "the"];

    let chars: Vec<char> = raw.chars().collect();
    let mut words: Vec<String> = Vec::new();
    let mut cur = String::new();
    for (i, &c) in chars.iter().enumerate() {
        let prev = i.checked_sub(1).and_then(|p| chars.get(p)).copied();
        let next = chars.get(i + 1).copied();
        let boundary = match (prev, c) {
            (None, _) => false,
            // ...aB - the ordinary camel case boundary
            (Some(p), c)
                if c.is_uppercase() && (p.is_lowercase() || p.is_numeric()) =>
            {
                true
            }
            // ...TVLegends - the last capital of a run starts a new word
            (Some(p), c)
                if c.is_uppercase()
                    && p.is_uppercase()
                    && next.is_some_and(char::is_lowercase) =>
            {
                true
            }
            // The13th - a number after a letter
            (Some(p), c) if c.is_numeric() && p.is_alphabetic() => true,
            _ => false,
        };
        if boundary && !cur.is_empty() {
            words.push(std::mem::take(&mut cur));
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        words.push(cur);
    }

    // "Schoolof" -> "School" + "of", "Tavernofthe" -> "Tavern" + "of" + "the"
    let mut peeled: Vec<String> = Vec::new();
    for word in words {
        let mut rest = word.as_str();
        let mut tail: Vec<&str> = Vec::new();
        loop {
            let found = GLUED.iter().find(|small| {
                rest.len() > small.len() + 3 && rest.ends_with(**small)
            });
            match found {
                Some(small) => {
                    let cut = rest.len() - small.len();
                    tail.push(&rest[cut..]);
                    rest = &rest[..cut];
                }
                None => break,
            }
        }
        peeled.push(rest.to_string());
        peeled.extend(tail.into_iter().rev().map(str::to_string));
    }

    // Small words stay lower case unless they open the name
    for (i, w) in peeled.iter_mut().enumerate() {
        let lower = w.to_lowercase();
        if i > 0 && SMALL.contains(&lower.as_str()) {
            *w = lower;
        }
    }
    peeled.join(" ")
}

#[cfg(test)]
mod util_tests {
    use super::split_camel;

    #[test]
    fn camel_case_is_readable() {
        assert_eq!(split_camel("MinesOfGloria"), "Mines of Gloria");
        assert_eq!(split_camel("Tower"), "Tower");
        assert_eq!(split_camel("CutthroatGrotto"), "Cutthroat Grotto");
        // "at" and "in" sit inside ordinary words - they must never be cut
        assert_eq!(split_camel("AshMountain"), "Ash Mountain");
        assert_eq!(split_camel("Sandstorm"), "Sandstorm");
    }

    #[test]
    fn small_words_glued_on_in_the_game_get_peeled_off() {
        // These are the names as the game library spells them
        assert_eq!(
            split_camel("TimeHonoredSchoolofMagic"),
            "Time Honored School of Magic"
        );
        assert_eq!(
            split_camel("TavernoftheDarkDoppelgangers"),
            "Tavern of the Dark Doppelgangers"
        );
        assert_eq!(
            split_camel("ContinuousLoopofIdols"),
            "Continuous Loop of Idols"
        );
        assert_eq!(split_camel("PyramidsofMadness"), "Pyramids of Madness");
    }

    #[test]
    fn numbers_and_capital_runs() {
        assert_eq!(split_camel("The13thFloor"), "The 13th Floor");
        assert_eq!(split_camel("RetroTVLegends"), "Retro TV Legends");
        assert_eq!(split_camel("PlayaGamesHQ"), "Playa Games HQ");
    }

    #[test]
    fn a_short_word_is_never_cut_in_half() {
        // "The" ends in "he" but there is nothing left to keep in front of
        // it, and "Hell" must not lose anything either
        assert_eq!(split_camel("Hell"), "Hell");
        assert_eq!(split_camel("TheServerRoom"), "The Server Room");
        assert_eq!(split_camel("Easteros"), "Easteros");
        assert_eq!(split_camel("Hemorridor"), "Hemorridor");
    }
}

/// A stretch of time in words a person uses for it: "2 d 4 h", "6 h
/// 20 min", "18 min". Not a countdown - this is for "about that long
/// from now", where seconds are noise and the unit is the point
#[must_use]
pub fn long_wait(secs: i64) -> String {
    let s = secs.max(0);
    if s >= 24 * 3600 {
        format!("{} d {} h", s / (24 * 3600), (s % (24 * 3600)) / 3600)
    } else if s >= 3600 {
        format!("{} h {} min", s / 3600, (s % 3600) / 60)
    } else {
        format!("{} min", (s / 60).max(1))
    }
}

#[cfg(test)]
mod wait_tests {
    use super::long_wait;

    #[test]
    fn a_wait_is_said_in_the_units_that_matter() {
        assert_eq!(long_wait(18 * 60), "18 min");
        assert_eq!(long_wait(6 * 3600 + 20 * 60), "6 h 20 min");
        assert_eq!(long_wait(2 * 24 * 3600 + 4 * 3600), "2 d 4 h");
        // Under a minute is still "1 min" - "0 min" reads as "now", and
        // it is not now
        assert_eq!(long_wait(5), "1 min");
        assert_eq!(long_wait(0), "1 min");
    }
}
