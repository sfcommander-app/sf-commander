//! The game's own lines in the guild chat, turned into sentences.
//!
//! What a person types arrives as text. What the *game* announces does
//! not: the server sends a code and the bare parts, and the game client
//! builds the sentence out of its own language file. So a promotion
//! reaches us as
//!
//! ```text
//! #ra#Petr Macinka#2#Mocny Mentol
//! ```
//!
//! and a cleared dungeon floor as `#du#Darkblitzz#11#5`. Until the parser
//! stopped throwing the game's lines away these never got as far as the
//! app at all; now they do, and showing them in that shape is worse than not
//! showing them. We are not the game client, so the sentence is ours to
//! write.
//!
//! A code nobody here knows yet is still shown - the parts, readably,
//! with the code in brackets at the end - rather than swallowed or left
//! as protocol. Better an honest "I do not know this one" on screen than
//! a wrong sentence in the guild's history.

use crate::i18n::t;

/// The guild ranks as the game numbers them (the same numbers
/// `sf_api::gamestate::guild::GuildRank` is parsed from)
const RANK_LEADER: &str = "1";
const RANK_OFFICER: &str = "2";
const RANK_MEMBER: &str = "3";
const RANK_INVITED: &str = "4";

/// Fill `{a}` and `{b}` in a translated template
fn fill(template: &'static str, a: &str, b: &str) -> String {
    t(template).replace("{a}", a).replace("{b}", b)
}

/// Split `#code#part#part...` into the code and its parts. `None` for
/// anything that is not one of the game's coded lines - somebody typing a
/// sentence that happens to start with a hash is not a system message
fn split(line: &str) -> Option<(&str, Vec<&str>)> {
    let rest = line.strip_prefix('#')?;
    let mut parts = rest.split('#');
    let code = parts.next()?;
    // A code is short and wordless; "#raid tonight#please come" is
    // somebody talking. It is *not* always pure letters, though -
    // `#r+#42#0` is a real one, and demanding letters threw it out and
    // dumped the raw protocol on screen, which is the one thing this
    // module exists to stop. So: short, no spaces, and at least one
    // letter in it
    if code.is_empty()
        || code.len() > 5
        || code.chars().any(char::is_whitespace)
        || !code.chars().any(char::is_alphabetic)
    {
        return None;
    }
    let parts: Vec<&str> = parts.collect();
    if parts.iter().all(|p| p.trim().is_empty()) {
        return None;
    }
    Some((code, parts))
}

/// The sentence for one of the game's coded lines, or `None` if this is
/// not one of them and should be shown as it came
#[must_use]
pub fn decode(line: &str) -> Option<String> {
    let (code, p) = split(line.trim())?;
    let get = |i: usize| p.get(i).copied().unwrap_or_default().trim();
    Some(match (code, p.len()) {
        // Confirmed against the in-game catapult announcement. The first
        // field can include a timestamp; zero is protocol data, not ammo.
        ("bd", 2) if !get(0).is_empty() && get(1) == "0" => {
            let (stamp, who) = get(0).split_once(' ')
                .filter(|(stamp, _)| chrono::NaiveTime::parse_from_str(stamp, "%H:%M").is_ok())
                .map_or(("", get(0)), |(stamp, who)| (stamp, who.trim()));
            let sentence = fill("Player {a} loaded the catapult with mushrooms.", who, "");
            if stamp.is_empty() { sentence } else { format!("{stamp} {sentence}") }
        }
        // Somebody's rank changed: who did it, the new rank, who it
        // happened to
        ("ra", 3) => {
            let (who, rank, whom) = (get(0), get(1), get(2));
            match rank {
                RANK_LEADER => fill("{a} handed the guild over to {b}", who, whom),
                RANK_OFFICER => fill("{a} promoted {b} to officer", who, whom),
                RANK_MEMBER => fill("{a} demoted {b} to member", who, whom),
                RANK_INVITED => fill("{a} invited {b} to the guild", who, whom),
                _ => fill("{a} changed {b}'s rank", who, whom),
            }
        }
        // A hit on the guild portal: who, how hard, and what the portal
        // has left. The middle field the game sends between the name and
        // the damage is not something we need - the last two numbers are
        // the damage and the life left - so the arm takes both the three
        // and the four part shapes rather than guessing what it means
        ("po", 3 | 4) => {
            let who = get(0);
            let damage = get(p.len() - 2);
            let left = get(p.len() - 1);
            // Word for word as the game writes it, not a paraphrase of
            // it: this line ends up in the guild's own history next to
            // the ones people typed, and a sentence nobody recognises is
            // worse than the raw code it replaced
            t("{a} did damage to the guild portal amounting to {b}. \
               Remaining lives: {c}%")
                .replace("{a}", who)
                .replace("{b}", &thousands(damage))
                .replace("{c}", left)
        }
        // Somebody levelled up - far and away the most common line in
        // any guild's chat, and it was still showing as "Name - 190 (lu)"
        ("lu", 2) => t("{a} reached level {b}")
            .replace("{a}", get(0))
            .replace("{b}", get(1)),
        // A cleared dungeon floor: who, which dungeon, which floor. The
        // dungeon is numbered from one here while the library numbers
        // from zero - "11" is `CircusOfHorror = 10`, which is what
        // confirmed the shape of this line in the first place
        // The guild announcement uses 90 for the Tower, independently of
        // LightDungeon::Tower's API index (14). Confirmed by the matching
        // in-game line for KaličůvPalec, floor 96.
        ("du", 3) if get(1) == "90" => {
            t("Player {a} cleared floor {b} of the Tower.")
                .replace("{a}", get(0))
                .replace("{b}", get(2))
        }
        ("du", 3) => {
            let who = get(0);
            let floor = get(2);
            match dungeon_name(get(1)) {
                Some(name) => t("{a} cleared floor {c} of {b}")
                    .replace("{a}", who)
                    .replace("{b}", &name)
                    .replace("{c}", floor),
                // An index the library does not know is not worth a
                // made-up name; the rest of the line still reads
                None => unknown(code, &p),
            }
        }
        _ => unknown(code, &p),
    })
}

/// The dungeon behind the number the chat sends, which counts from one
/// while `LightDungeon` counts from zero. `None` for anything the library
/// has no name for, so a new dungeon never comes out as the wrong one
fn dungeon_name(index: &str) -> Option<String> {
    use num_traits::FromPrimitive;
    let n: u32 = index.parse().ok()?;
    let dungeon =
        sf_api::gamestate::dungeons::LightDungeon::from_u32(n.checked_sub(1)?)?;
    Some(crate::util::split_camel(&format!("{dungeon:?}")))
}

/// A run of digits with thin spaces every three, the way the game prints
/// them. Anything that is not a plain number is handed back untouched
fn thousands(n: &str) -> String {
    if n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) {
        return n.to_string();
    }
    let mut out = String::with_capacity(n.len() + n.len() / 3);
    for (i, c) in n.chars().enumerate() {
        if i > 0 && (n.len() - i).is_multiple_of(3) {
            out.push('\u{00A0}');
        }
        out.push(c);
    }
    out
}

/// A code with no sentence written for it yet. The parts, in the order
/// they came, and the code at the end so it can be reported and added
fn unknown(code: &str, parts: &[&str]) -> String {
    let body = parts
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" - ");
    format!("{body}  ({code})")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rank_change_reads_as_a_sentence() {
        assert_eq!(
            decode("#ra#Petr Macinka#2#Mocny Mentol").unwrap(),
            "Petr Macinka promoted Mocny Mentol to officer"
        );
        assert_eq!(
            decode("#ra#Petr Macinka#3#Flath").unwrap(),
            "Petr Macinka demoted Flath to member"
        );
        assert_eq!(
            decode("#ra#Petr Macinka#1#Vanley").unwrap(),
            "Petr Macinka handed the guild over to Vanley"
        );
        assert_eq!(
            decode("#ra#Petr Macinka#4#Stabington").unwrap(),
            "Petr Macinka invited Stabington to the guild"
        );
        // A rank the game grows later still says something true
        assert_eq!(
            decode("#ra#Petr Macinka#9#Nobody").unwrap(),
            "Petr Macinka changed Nobody's rank"
        );
    }

    /// A hit on the guild portal, with the damage grouped the way the
    /// game groups it. The field between the name and the damage is not
    /// ours to interpret, so both shapes have to read the same
    #[test]
    fn a_portal_hit_reads_as_a_sentence() {
        assert_eq!(
            decode("#po#Reverus#2#4738023#78").unwrap(),
            "Reverus did damage to the guild portal amounting to \
             4\u{00A0}738\u{00A0}023. Remaining lives: 78%"
        );
        assert_eq!(
            decode("#po#Darkblitzz#9121415#78").unwrap(),
            "Darkblitzz did damage to the guild portal amounting to \
             9\u{00A0}121\u{00A0}415. Remaining lives: 78%"
        );
    }

    #[test]
    fn digits_are_grouped_and_anything_else_is_left_alone() {
        assert_eq!(thousands("9121415"), "9\u{00A0}121\u{00A0}415");
        assert_eq!(thousands("78"), "78");
        assert_eq!(thousands("1000"), "1\u{00A0}000");
        assert_eq!(thousands(""), "");
        assert_eq!(thousands("1.5M"), "1.5M");
    }

    /// The line that gave the numbering away: the chat counts dungeons
    /// from one, `LightDungeon` from zero, so 11 is the Circus of Horror
    #[test]
    fn a_cleared_dungeon_floor_names_the_dungeon() {
        assert_eq!(
            decode("#du#Darkblitzz#11#5").unwrap(),
            "Darkblitzz cleared floor 5 of Circus of Horror"
        );
        assert_eq!(
            decode("#du#Somebody#1#10").unwrap(),
            "Somebody cleared floor 10 of Desecrated Catacombs"
        );
        // Zero and an index nobody knows fall back rather than naming the
        // wrong dungeon
        assert_eq!(decode("#du#Somebody#0#3").unwrap(), "Somebody - 0 - 3  (du)");
        assert_eq!(
            decode("#du#Somebody#999#3").unwrap(),
            "Somebody - 999 - 3  (du)"
        );
    }

    #[test]
    fn tower_announcement_uses_its_chat_id_and_keeps_the_floor() {
        assert_eq!(
            decode("#du#KaličůvPalec#90#96").unwrap(),
            "Player KaličůvPalec cleared floor 96 of the Tower."
        );
        assert_eq!(
            decode("#du#Dr Miro#90#100").unwrap(),
            "Player Dr Miro cleared floor 100 of the Tower."
        );
        assert_eq!(decode("#du#Somebody#90").unwrap(), "Somebody - 90  (du)");
        assert_eq!(decode("#du#Somebody#91#96").unwrap(), "Somebody - 91 - 96  (du)");
    }

    #[test]
    fn a_level_up_reads_as_a_sentence() {
        assert_eq!(
            decode("#lu#Tilleporter#190").unwrap(),
            "Tilleporter reached level 190"
        );
        assert_eq!(
            decode("#lu#Schn\u{00F6}ddel#240").unwrap(),
            "Schn\u{00F6}ddel reached level 240"
        );
    }

    /// The point of the fallback: no sentence is invented, and nothing is
    /// swallowed either
    #[test]
    fn an_unknown_code_is_shown_readably_with_its_code() {
        assert_eq!(decode("#xyz#Someone").unwrap(), "Someone  (xyz)");
        assert_eq!(decode("#zz#A#1#2#3#4").unwrap(), "A - 1 - 2 - 3 - 4  (zz)");
        // A code with punctuation in it is still a code. Insisting on
        // letters left "#r+#42#0" on screen exactly as the server sent it
        assert_eq!(decode("#r+#42#0").unwrap(), "42 - 0  (r+)");
        assert_eq!(
            decode("#bd#21:45 Dr Miro#9").unwrap(),
            "21:45 Dr Miro - 9  (bd)"
        );
    }

    #[test]
    fn catapult_announcement_preserves_names_and_optional_time() {
        assert_eq!(decode("#bd#02:04 PoseidonVltavy#0").unwrap(),
            "02:04 Player PoseidonVltavy loaded the catapult with mushrooms.");
        assert_eq!(decode("#bd#Dr Miro#0").unwrap(),
            "Player Dr Miro loaded the catapult with mushrooms.");
        assert_eq!(decode("#bd#21:45 Dr Miro#0").unwrap(),
            "21:45 Player Dr Miro loaded the catapult with mushrooms.");
        assert_eq!(decode("#bd#PoseidonVltavy"), Some("PoseidonVltavy  (bd)".into()));
    }

    /// Somebody talking is left exactly alone, hash or no hash
    #[test]
    fn what_a_person_typed_is_never_touched() {
        assert_eq!(decode("hello everyone"), None);
        assert_eq!(decode("#1 in the guild!"), None);
        assert_eq!(decode("#"), None);
        assert_eq!(decode("#ra#"), None);
        assert_eq!(decode("#raid tonight#please come"), None);
    }
}
