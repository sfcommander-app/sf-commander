//! Licensing: a signed permission slip that runs out after a year.
//!
//! The shape of it is the smallest thing that does the job. A licence is a
//! short line of text the author signs with a private key that never leaves
//! his machine, and that the app checks against a public key compiled into
//! it. Nobody can write themselves a licence, because writing one means
//! producing an Ed25519 signature over their own text without the private
//! key.
//!
//! # What this does and does not stop
//!
//! Signing stops **forgery**. It does not stop **patching**, and it is worth
//! being straight about that: the binary runs on the customer's computer,
//! the customer owns the computer, and a determined one can change the
//! instruction that acts on [`Status`], or swap the public key below for one
//! they hold the private half of. No amount of cryptography in a program
//! that verifies itself can prevent that - the check and the thing checked
//! are both in their hands. What this does buy:
//!
//! * Nobody can mint a licence, extend their own, or change whose it is.
//! * A licence passed to a friend does not work, because it names the
//!   machine it was issued for.
//! * Turning the clock back does not buy time (see `high_water`).
//! * Getting round it takes a patched build that has to be re-made for every
//!   release, and passed around as a cracked binary - which is a different
//!   and much more visible act than sharing a file.
//!
//! That is what commercial licensing is for. It makes paying the easy path,
//! not an impossible one to avoid.

use std::{fmt, time::SystemTime};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD as B64};

/// The public half of the signing key. The private half lives on the
/// author's machine and is never in this repository.
///
/// Replace this with the line `sfc-licence keygen` prints. Until then the
/// app refuses every licence rather than accepting every licence - a zero
/// key would otherwise be a key an attacker can trivially match.
pub const PUBLIC_KEY: [u8; 32] = [0u8; 32];

/// Whether this build enforces licensing at all.
///
/// A build with no key pasted in yet is a build in development: it runs
/// exactly as it always has. Turning the check on is one act - pasting the
/// public key - so there is no way to ship an enforcing build by accident
/// and no way to ship a key-less one that locks every customer out
#[must_use]
pub fn enforced() -> bool {
    PUBLIC_KEY.iter().any(|b| *b != 0)
}

/// Every licence starts with this, so a token that is something else
/// entirely fails with a clear message instead of a parse error
const PREFIX: &str = "SFC1";

/// How long a licence is good for when the tool is asked for the usual one
pub const TERM_DAYS: i64 = 365;

/// Within this many days of the end, the app starts saying so
pub const WARN_DAYS: i64 = 14;

/// The furthest the remembered clock is allowed to jump forward in one
/// sitting. A machine whose clock is briefly wrong - a dead CMOS battery
/// reading 2099, a bad time sync - would otherwise poison the high-water
/// mark for good and expire a paying customer permanently
const MAX_CLOCK_JUMP_SECS: i64 = 45 * 86_400;

// ------------------------------------------------------------------ errors

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Invalid {
    /// The text is not a licence at all
    Malformed,
    /// Right shape, but the signature is not the author's
    BadSignature,
    /// A newer app issued it, or the format moved on
    UnknownVersion(String),
    /// The build has no public key compiled in
    NoKeyInBuild,
}

impl fmt::Display for Invalid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Invalid::Malformed => f.write_str("that is not a licence code"),
            Invalid::BadSignature => {
                f.write_str("this licence was not issued by the author")
            }
            Invalid::UnknownVersion(v) => {
                write!(f, "this licence is version {v}, which this build of \
                           the app does not know - update the app")
            }
            Invalid::NoKeyInBuild => f.write_str(
                "this build has no licence key compiled into it",
            ),
        }
    }
}

// ----------------------------------------------------------------- licence

/// What a licence says. Everything here is covered by the signature
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Licence {
    /// Serial number, so one can be named in a message or a refund
    pub id: String,
    /// The machine it was issued for, as [`fingerprint`] renders it
    pub machine: String,
    /// Who it was sold to - shown in the app, not checked
    pub holder: String,
    /// Unix seconds
    pub issued: i64,
    /// Unix seconds. After this the app stops
    pub expires: i64,
}

impl Licence {
    /// The exact bytes that get signed.
    ///
    /// Canonical by construction: fixed order, one separator, no optional
    /// whitespace, no escaping rules to get wrong on one side and right on
    /// the other. The issuing tool builds the same line from the same
    /// function, so the two cannot drift apart
    #[must_use]
    pub fn payload(&self) -> String {
        format!(
            "{PREFIX}|{}|{}|{}|{}|{}",
            self.id, self.machine, self.issued, self.expires, self.holder
        )
    }

    /// Read back what [`Licence::payload`] wrote
    fn from_payload(s: &str) -> Result<Licence, Invalid> {
        // The holder's name is last and may contain anything except the
        // separator, so the split is bounded and the name keeps its spaces
        let mut it = s.splitn(6, '|');
        let version = it.next().ok_or(Invalid::Malformed)?;
        if version != PREFIX {
            return Err(Invalid::UnknownVersion(version.to_string()));
        }
        let mut next = || it.next().ok_or(Invalid::Malformed);
        let id = next()?.to_string();
        let machine = next()?.to_string();
        let issued: i64 = next()?.parse().map_err(|_| Invalid::Malformed)?;
        let expires: i64 = next()?.parse().map_err(|_| Invalid::Malformed)?;
        let holder = next()?.to_string();
        if id.is_empty() || machine.is_empty() {
            return Err(Invalid::Malformed);
        }
        Ok(Licence {
            id,
            machine,
            holder,
            issued,
            expires,
        })
    }

    /// The whole token: the line, the signature, and nothing else
    #[must_use]
    pub fn token(&self, signature: &[u8]) -> String {
        format!(
            "{}.{}",
            B64.encode(self.payload().as_bytes()),
            B64.encode(signature)
        )
    }
}

/// Check a pasted code against the key compiled into this build.
///
/// Returns the licence only if the signature is the author's. Nothing about
/// dates or machines is decided here - that is [`check`], which needs to
/// know what time it is and what machine this is
pub fn verify(token: &str, public_key: &[u8; 32]) -> Result<Licence, Invalid> {
    if public_key.iter().all(|b| *b == 0) {
        return Err(Invalid::NoKeyInBuild);
    }
    // Pasted codes arrive with newlines in them from chat clients, and with
    // the odd stray space. None of that is part of the code
    let cleaned: String =
        token.chars().filter(|c| !c.is_whitespace()).collect();
    let (payload_b64, sig_b64) =
        cleaned.split_once('.').ok_or(Invalid::Malformed)?;
    let payload = B64.decode(payload_b64).map_err(|_| Invalid::Malformed)?;
    let signature = B64.decode(sig_b64).map_err(|_| Invalid::Malformed)?;
    ring::signature::UnparsedPublicKey::new(
        &ring::signature::ED25519,
        public_key.as_slice(),
    )
    .verify(&payload, &signature)
    .map_err(|_| Invalid::BadSignature)?;
    let text = std::str::from_utf8(&payload).map_err(|_| Invalid::Malformed)?;
    Licence::from_payload(text)
}

// ------------------------------------------------------------------ status

/// Where a licence stands right now
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Good, with this many whole days to go
    Valid { days_left: i64 },
    /// Good, but close enough to say so
    EndingSoon { days_left: i64 },
    /// Over. The app stops
    Expired { days_over: i64 },
    /// A real licence, for somebody else's computer
    WrongMachine { issued_for: String },
    /// No licence at all yet
    Missing,
    /// Not a licence, or not one of ours
    Bad(Invalid),
}

impl Status {
    /// Whether the app is allowed to do anything on the player's behalf.
    ///
    /// Reading stays open in every state - locking someone out of looking at
    /// their own settings helps nobody. What stops is the automation, which
    /// is the thing being sold
    #[must_use]
    pub fn may_run(&self) -> bool {
        !enforced() || self.is_current()
    }

    /// The policy on its own, with no regard for whether this build
    /// enforces anything. This is the rule; [`Status::may_run`] is the rule
    /// plus the question of whether the rule is switched on
    #[must_use]
    pub fn is_current(&self) -> bool {
        matches!(self, Status::Valid { .. } | Status::EndingSoon { .. })
    }
}

/// Decide where a licence stands, given the clock and the machine.
///
/// `now` is unix seconds, already passed through [`effective_now`]
#[must_use]
pub fn check(licence: &Licence, now: i64, machine: &str) -> Status {
    if licence.machine != machine {
        return Status::WrongMachine {
            issued_for: licence.machine.clone(),
        };
    }
    let left = licence.expires - now;
    if left <= 0 {
        return Status::Expired {
            days_over: (-left).div_euclid(86_400) + 1,
        };
    }
    // A licence with four hours left has "0 days left" by division, which
    // reads as expired. Round up: it is still a day you can use
    let days_left = left.div_euclid(86_400) + i64::from(left % 86_400 != 0);
    if days_left <= WARN_DAYS {
        Status::EndingSoon { days_left }
    } else {
        Status::Valid { days_left }
    }
}

// ------------------------------------------------------------------- clock

/// The time to judge a licence by, and the new high-water mark to store.
///
/// The obvious way to get a year's use for free is to put the clock back, so
/// the app remembers the latest time it has ever seen and never judges a
/// licence by anything earlier. Winding the clock back then buys nothing;
/// it only makes the app notice.
///
/// The cap is there for honesty in the other direction. A machine whose
/// clock is briefly absurd - a flat CMOS battery, a bad sync - must not be
/// able to burn the customer's licence permanently, so the remembered mark
/// creeps forward by at most [`MAX_CLOCK_JUMP_SECS`] at a time even when the
/// clock claims a decade
#[must_use]
pub fn effective_now(system_now: i64, high_water: i64) -> (i64, i64) {
    let judge_by = system_now.max(high_water);
    let new_mark = high_water.max(
        system_now.min(high_water.saturating_add(MAX_CLOCK_JUMP_SECS)),
    );
    (judge_by, new_mark)
}

/// Whether the machine's clock is behind what the app has already seen, by
/// enough that it is worth telling the user rather than silently ignoring
#[must_use]
pub fn clock_looks_wrong(system_now: i64, high_water: i64) -> bool {
    high_water - system_now > 2 * 86_400
}

/// Unix seconds, or 0 if the machine's clock is set before 1970
#[must_use]
pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0))
}

// -------------------------------------------------------------- the machine

/// A short, stable name for this computer, in groups of four so it can be
/// read out over chat without mistakes.
///
/// Built from what the operating system already knows about itself, hashed,
/// so it carries nothing about the machine that anyone could read back out
/// of it. It survives reinstalling the app and wiping its settings; it
/// changes when the operating system is reinstalled, which is the moment a
/// customer has to ask for a re-issue.
#[must_use]
pub fn fingerprint() -> String {
    render_fingerprint(&machine_secrets())
}

/// The raw material, kept apart from the hashing so the hashing can be
/// tested without a machine to hash
fn machine_secrets() -> Vec<String> {
    let mut parts = Vec::new();
    #[cfg(windows)]
    {
        if let Some(guid) = windows_machine_guid() {
            parts.push(guid);
        }
        if let Ok(name) = std::env::var("COMPUTERNAME") {
            parts.push(name);
        }
    }
    #[cfg(not(windows))]
    {
        // The app ships for Windows; this is what keeps the tests and the
        // author's own builds honest
        if let Ok(id) = std::fs::read_to_string("/etc/machine-id") {
            parts.push(id.trim().to_string());
        }
        if let Ok(name) = std::fs::read_to_string("/etc/hostname") {
            parts.push(name.trim().to_string());
        }
    }
    parts.retain(|p| !p.is_empty());
    parts
}

/// Windows keeps a per-installation GUID in the registry. Read with the
/// tool every Windows has rather than a crate, and with no console window -
/// this runs inside a windowed application
#[cfg(windows)]
fn windows_machine_guid() -> Option<String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new("reg")
        .args([
            "query",
            r"HKLM\SOFTWARE\Microsoft\Cryptography",
            "/v",
            "MachineGuid",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    // "    MachineGuid    REG_SZ    0c3a...".  The value is the last word
    text.lines()
        .find(|l| l.contains("MachineGuid"))
        .and_then(|l| l.split_whitespace().last())
        .map(str::to_string)
}

/// Hash the parts and render them as five groups of four.
///
/// A machine that tells us nothing about itself would otherwise hash to the
/// same name as every other such machine, and one licence would fit them
/// all - so that case is called out rather than papered over
fn render_fingerprint(parts: &[String]) -> String {
    if parts.is_empty() {
        return "UNKNOWN-MACHINE".to_string();
    }
    let joined = parts.join("\u{1f}");
    let digest = ring::digest::digest(&ring::digest::SHA256, joined.as_bytes());
    // Crockford base32 without the letters that get misread aloud
    const ALPHABET: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let mut out = String::with_capacity(24);
    for (i, byte) in digest.as_ref().iter().take(13).enumerate() {
        if i > 0 && i % 4 == 0 {
            out.push('-');
        }
        out.push(char::from(ALPHABET[usize::from(byte % 32)]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A key pair made on the spot, so the tests exercise the real signing
    /// path rather than a stub
    fn keypair() -> (ring::signature::Ed25519KeyPair, [u8; 32]) {
        let rng = ring::rand::SystemRandom::new();
        let pkcs8 =
            ring::signature::Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
        let pair =
            ring::signature::Ed25519KeyPair::from_pkcs8(pkcs8.as_ref())
                .unwrap();
        let mut public = [0u8; 32];
        use ring::signature::KeyPair as _;
        public.copy_from_slice(pair.public_key().as_ref());
        (pair, public)
    }

    fn issue(
        pair: &ring::signature::Ed25519KeyPair,
        machine: &str,
        expires: i64,
    ) -> String {
        let licence = Licence {
            id: "L-0001".into(),
            machine: machine.into(),
            holder: "Petr Macinka".into(),
            issued: 1_700_000_000,
            expires,
        };
        let sig = pair.sign(licence.payload().as_bytes());
        licence.token(sig.as_ref())
    }

    #[test]
    fn a_licence_the_author_signed_is_accepted() {
        let (pair, public) = keypair();
        let token = issue(&pair, "ABCD-EFGH", 1_800_000_000);
        let licence = verify(&token, &public).expect("should verify");
        assert_eq!(licence.machine, "ABCD-EFGH");
        assert_eq!(licence.holder, "Petr Macinka");
        assert_eq!(licence.expires, 1_800_000_000);
    }

    #[test]
    fn a_licence_signed_by_anybody_else_is_not() {
        // The whole point: another key is not the author's key
        let (theirs, _) = keypair();
        let (_, ours) = keypair();
        let token = issue(&theirs, "ABCD-EFGH", 1_800_000_000);
        assert_eq!(verify(&token, &ours), Err(Invalid::BadSignature));
    }

    #[test]
    fn changing_one_character_of_a_real_licence_breaks_it() {
        // Somebody editing the expiry date out of their own valid licence
        // is the first thing anyone tries
        let (pair, public) = keypair();
        let token = issue(&pair, "ABCD-EFGH", 1_800_000_000);
        let real = verify(&token, &public).unwrap();
        let forged = Licence {
            expires: real.expires + 10 * 365 * 86_400,
            ..real
        };
        let tampered = format!(
            "{}.{}",
            B64.encode(forged.payload().as_bytes()),
            token.split_once('.').unwrap().1
        );
        assert_eq!(verify(&tampered, &public), Err(Invalid::BadSignature));
    }

    #[test]
    fn a_build_with_no_key_does_not_lock_anybody_out() {
        // Development builds, and any build shipped before the key is
        // pasted in, must behave exactly as the app always has. The check
        // turns on by pasting a key and in no other way
        assert!(!enforced());
        assert!(Status::Missing.may_run());
        assert!(Status::Expired { days_over: 500 }.may_run());
        // ...but the rule itself still says what it says
        assert!(!Status::Expired { days_over: 500 }.is_current());
    }

    #[test]
    fn a_build_with_no_key_trusts_nothing() {
        // Shipping before the key is pasted in must fail closed. An
        // all-zero key is one an attacker can sign for
        let (pair, _) = keypair();
        let token = issue(&pair, "ABCD-EFGH", 1_800_000_000);
        assert_eq!(verify(&token, &[0u8; 32]), Err(Invalid::NoKeyInBuild));
    }

    #[test]
    fn rubbish_in_the_box_says_so_rather_than_crashing() {
        let (_, public) = keypair();
        for junk in ["", "hello", "....", "a.b", "SFC1|nope"] {
            assert!(matches!(
                verify(junk, &public),
                Err(Invalid::Malformed | Invalid::BadSignature)
            ));
        }
    }

    #[test]
    fn whitespace_from_a_chat_client_is_not_part_of_the_code() {
        // People paste these out of Discord, which wraps long lines
        let (pair, public) = keypair();
        let token = issue(&pair, "ABCD-EFGH", 1_800_000_000);
        let mangled = format!(
            "  {}\n{}  ",
            &token[..20],
            &token[20..]
        );
        assert!(verify(&mangled, &public).is_ok());
    }

    #[test]
    fn a_name_with_spaces_survives_the_round_trip() {
        let l = Licence {
            id: "L-7".into(),
            machine: "M".into(),
            holder: "Petr  Macinka ml.".into(),
            issued: 1,
            expires: 2,
        };
        assert_eq!(Licence::from_payload(&l.payload()).unwrap(), l);
    }

    #[test]
    fn a_licence_for_another_computer_does_not_work_here() {
        let l = Licence {
            id: "L-1".into(),
            machine: "THEIR-MACHINE".into(),
            holder: "x".into(),
            issued: 0,
            expires: i64::MAX,
        };
        assert!(matches!(
            check(&l, 1_000, "MY-MACHINE"),
            Status::WrongMachine { .. }
        ));
    }

    #[test]
    fn the_year_runs_out_on_the_day_it_says() {
        let start = 1_700_000_000;
        let l = Licence {
            id: "L-1".into(),
            machine: "M".into(),
            holder: "x".into(),
            issued: start,
            expires: start + TERM_DAYS * 86_400,
        };
        assert_eq!(check(&l, start, "M"), Status::Valid { days_left: 365 });
        // The day before the end is still a day you paid for
        let last_day = l.expires - 3_600;
        assert_eq!(
            check(&l, last_day, "M"),
            Status::EndingSoon { days_left: 1 }
        );
        assert!(check(&l, last_day, "M").is_current());
        // And the moment it is over, it is over
        assert_eq!(
            check(&l, l.expires, "M"),
            Status::Expired { days_over: 1 }
        );
        assert!(!check(&l, l.expires, "M").is_current());
    }

    #[test]
    fn the_warning_starts_a_fortnight_out() {
        let start = 1_700_000_000;
        let l = Licence {
            id: "L-1".into(),
            machine: "M".into(),
            holder: "x".into(),
            issued: start,
            expires: start + TERM_DAYS * 86_400,
        };
        let at = |days: i64| check(&l, l.expires - days * 86_400, "M");
        assert_eq!(at(WARN_DAYS + 1), Status::Valid { days_left: 15 });
        assert_eq!(at(WARN_DAYS), Status::EndingSoon { days_left: 14 });
    }

    #[test]
    fn putting_the_clock_back_buys_nothing() {
        let seen = 1_800_000_000;
        // A year back
        let (judge_by, mark) = effective_now(seen - 365 * 86_400, seen);
        assert_eq!(judge_by, seen, "the app judged by the moved clock");
        assert_eq!(mark, seen, "the mark must not move backwards");
        assert!(clock_looks_wrong(seen - 365 * 86_400, seen));
    }

    #[test]
    fn a_clock_that_is_briefly_absurd_does_not_burn_the_licence() {
        // A flat CMOS battery reading 2099 must not permanently expire
        // somebody who has paid
        let seen = 1_800_000_000;
        let absurd = seen + 70 * 365 * 86_400;
        let (_, mark) = effective_now(absurd, seen);
        assert_eq!(
            mark,
            seen + MAX_CLOCK_JUMP_SECS,
            "the remembered time ran away with a broken clock"
        );
        // Once the clock is right again, the mark is only weeks ahead and
        // ordinary time catches up
        assert!(mark < seen + 90 * 86_400);
    }

    #[test]
    fn ordinary_time_passing_moves_the_mark_along() {
        let seen = 1_800_000_000;
        let later = seen + 3 * 86_400;
        let (judge_by, mark) = effective_now(later, seen);
        assert_eq!(judge_by, later);
        assert_eq!(mark, later);
        assert!(!clock_looks_wrong(later, seen));
    }

    #[test]
    fn a_machine_name_is_stable_readable_and_says_nothing() {
        let parts = vec!["some-guid".to_string(), "PC-NAME".to_string()];
        let a = render_fingerprint(&parts);
        assert_eq!(a, render_fingerprint(&parts), "not stable");
        assert_ne!(
            a,
            render_fingerprint(&["other-guid".to_string()]),
            "two machines got the same name"
        );
        assert_eq!(a.len(), 13 + 3, "not four groups of four and a bit");
        assert!(
            a.chars().all(|c| c.is_ascii_uppercase()
                || c.is_ascii_digit()
                || c == '-')
        );
        // Nothing of the input survives into the name
        assert!(!a.contains("PC-NAME"));
    }

    #[test]
    fn a_machine_that_will_not_say_who_it_is_is_not_everyone() {
        // Falling back to a constant would make one licence fit every such
        // machine, which is the opposite of the point
        assert_eq!(render_fingerprint(&[]), "UNKNOWN-MACHINE");
    }
}
