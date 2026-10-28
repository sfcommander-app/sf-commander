//! Owner-only features. Two locks: a password (SHA-256 of what is typed is
//! compared with the constant below) and a hard-coded list of character
//! names the features are allowed to act for. Even with the password, the
//! announcement never goes out from anybody else's character.

use sha2::{Digest, Sha256};

/// SHA-256 of the admin password. To change it, hash the new password and
/// paste it here (e.g. `echo -n newpass | sha256sum`)
pub const PASSWORD_SHA256: &str =
    "928a78db3538790530979acdec5956c8d0ccd29f7939c58c8ad4b5df194ddd2c";

/// Characters that may post the announcement (lowercase)
pub const OWNER_CHARACTERS: &[&str] = &["petr macinka", "gussolean"];

/// The name on the copyright line. The character he plays under, spelled
/// the way the guild reads it - which is the name this program is known
/// by to everybody who runs it, and not the one on his post
pub const AUTHOR: &str = "Petr Macinka";

/// The one character the daily Discord invite goes out from (lowercase).
/// The guild leader's own: it is his guild's invite, the other owner
/// character sits in a different guild on another server and must not
/// post it there
pub const DISCORD_POSTER: &str = "petr macinka";

/// The Discord invite, as one line ready to paste into the guild chat. It
/// has its own button so it can go out without overwriting whatever the
/// periodic announcement is set to
pub const DISCORD_INVITE: &str =
    "Join our discord! :) https://discord.gg/47EZeahgn";

pub const DEFAULT_ANNOUNCEMENT: &str = "Vytvořil jsem nástroj pro cech - automatizace, statistiky, procenta. \
Zájemci na náš Discord: https://discord.gg/47EZeahgn (napište nick postavy, dostanete roli a download)";

pub fn password_ok(typed: &str) -> bool {
    let mut h = Sha256::new();
    h.update(typed.trim().as_bytes());
    let hex = format!("{:x}", h.finalize());
    hex == PASSWORD_SHA256
}

pub fn is_owner(character_name: &str) -> bool {
    let n = character_name.trim().to_lowercase();
    OWNER_CHARACTERS.iter().any(|o| *o == n)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The password itself is deliberately not in here - the whole point of
    /// storing a hash is that the source does not carry it. This only checks
    /// the constant is a real SHA-256 and that the gate says no by default
    #[test]
    fn the_constant_is_a_sha256_and_rejects_the_wrong_word() {
        assert_eq!(PASSWORD_SHA256.len(), 64);
        assert!(
            PASSWORD_SHA256
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
        assert!(!password_ok(""));
        assert!(!password_ok("password"));
    }

    #[test]
    fn the_discord_poster_is_an_owner() {
        assert!(is_owner(DISCORD_POSTER));
        assert_eq!(DISCORD_POSTER, DISCORD_POSTER.to_lowercase());
    }

    #[test]
    fn only_the_listed_characters_count_as_owner() {
        assert!(is_owner("Petr Macinka"));
        assert!(is_owner("  gussolean  "));
        assert!(!is_owner("Panoramix"));
    }
}
