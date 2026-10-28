//! The game's guild finder ("Guild search" for a character without a guild).
//!
//! Neither request is in `sf-api`; both were read off the live game client
//! (the request class names sit in the WebGL build, the wire format was
//! checked against a running session):
//!
//! * `GroupJoinList` (no parameter) answers with `joinablegrouplist.r:` -
//!   fourteen `/`-separated fields per guild, up to 50 guilds, the same
//!   order the game shows them in
//! * `GroupJoin:<guild id>` is the ENTER button. From a character that
//!   already has a guild the id answers "cannot do this right now" and the
//!   name "server not available"; a character without a guild got the
//!   same "server not available" for the name (1.78, live), so the id is
//!   what the server understands. Still to be confirmed: the answer to
//!   the id from a guildless character
//!
//! The fields, in order: rank, id, name, members, three strength numbers
//! the game does not label (they track the honor), an unknown small number,
//! the emblem, lowest / highest / average member level, the description
//! (`$b` = line break, `$C` = comma) and the chat language (`xx` = any).

use std::collections::HashMap;

/// One guild the finder offers
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinderGuild {
    pub rank: u32,
    pub id: u32,
    pub name: String,
    pub members: u8,
    pub min_level: u16,
    pub max_level: u16,
    pub avg_level: u16,
    pub description: String,
    pub language: String,
}

/// How many fields one guild takes in the list
const FIELDS: usize = 14;

/// Parses the value of `joinablegrouplist.r`. A record that does not parse
/// is skipped, not fatal - the list is advisory
pub fn parse_list(raw: &str) -> Vec<FinderGuild> {
    let parts: Vec<&str> = raw.split('/').collect();
    parts
        .as_chunks::<FIELDS>()
        .0
        .iter()
        .filter_map(|f| {
            let num = |i: usize| f[i].trim().parse::<u32>().ok();
            Some(FinderGuild {
                rank: num(0)?,
                id: num(1)?,
                name: f[2].trim().to_string(),
                members: num(3)? as u8,
                min_level: num(9)? as u16,
                max_level: num(10)? as u16,
                avg_level: num(11)? as u16,
                description: f[12]
                    .replace("$b", "\n")
                    .replace("$C", ",")
                    .trim()
                    .to_string(),
                language: f[13].trim().to_string(),
            })
        })
        .filter(|g| !g.name.is_empty())
        .collect()
}

/// The two guild-wide bonuses, as the sum of what the members bought:
/// treasure (gold from quests) and instructor (XP from quests)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Bonus {
    pub treasure: u16,
    pub instructor: u16,
}

impl Bonus {
    /// What a member gets out of the guild, in one number
    #[must_use]
    pub fn total(self) -> u32 {
        u32::from(self.treasure) + u32::from(self.instructor)
    }
}

/// Picks the guild worth joining: the best bonuses among those with a free
/// seat that have not refused us yet. Guilds whose bonuses are not known
/// yet never win - a guess would send the character into a random guild,
/// which is worse than waiting for one more lookup
pub fn best<'a>(
    list: &'a [FinderGuild],
    bonuses: &HashMap<String, Bonus>,
    tried: &[String],
) -> Option<&'a FinderGuild> {
    list.iter()
        .filter(|g| g.members < 50)
        .filter(|g| !tried.iter().any(|t| t.eq_ignore_ascii_case(&g.name)))
        .filter_map(|g| bonuses.get(&g.name.to_lowercase()).map(|b| (g, b)))
        // Best bonus first; the Hall of Fame rank breaks a tie
        .max_by(|(ga, a), (gb, b)| {
            a.total()
                .cmp(&b.total())
                .then_with(|| gb.rank.cmp(&ga.rank))
        })
        .map(|(g, _)| g)
}

/// Which guilds to look up for their bonuses: the finder's order is the
/// game's own, so the first dozen not yet known are the ones worth a
/// request each. A second read used to look at the same twelve again
/// instead of moving down the list
pub fn worth_a_look(
    list: &[FinderGuild],
    known: &HashMap<String, Bonus>,
) -> Vec<String> {
    list.iter()
        .filter(|g| g.members < 50)
        .filter(|g| !known.contains_key(&g.name.to_lowercase()))
        .take(12)
        .map(|g| g.name.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "140/289/Rising Phoenix/47/324/402/407/12/0303130615013509090309/20/147/81/LvL 39+$b$bMaximum 2 day inactivity/en/237/1863/Cech/46/191/324/331/11/04030F0810042908080306/12/148/58//cs/371/1949/Peklo/50/61/101/101/0/00/1/100/26/Full$C sorry/xx";

    #[test]
    fn parses_the_live_format() {
        let list = parse_list(SAMPLE);
        assert_eq!(list.len(), 3);
        let g = &list[0];
        assert_eq!(g.rank, 140);
        assert_eq!(g.id, 289);
        assert_eq!(g.name, "Rising Phoenix");
        assert_eq!(g.members, 47);
        assert_eq!((g.min_level, g.max_level, g.avg_level), (20, 147, 81));
        assert_eq!(g.description, "LvL 39+\n\nMaximum 2 day inactivity");
        assert_eq!(g.language, "en");
        assert_eq!(list[1].description, "");
        assert_eq!(list[2].description, "Full, sorry");
    }

    /// The library files `joinablegrouplist.r` under the key before the
    /// dot - the full name found nothing and the list came up empty
    #[test]
    fn the_response_key_is_the_part_before_the_dot() {
        let resp = sf_api::response::Response::parse(
            format!("joinablegrouplist.r:{SAMPLE}"),
            chrono::Local::now().naive_local(),
        )
        .unwrap();
        assert!(resp.values().get("joinablegrouplist.r").is_none());
        let raw = resp.values().get("joinablegrouplist").unwrap().as_str();
        assert_eq!(parse_list(raw).len(), 3);
    }

    #[test]
    fn a_broken_tail_is_dropped() {
        let list = parse_list(&format!("{SAMPLE}/9/garbage"));
        assert_eq!(list.len(), 3);
        assert!(parse_list("").is_empty());
    }

    #[test]
    fn best_needs_known_bonuses_and_a_seat() {
        let list = parse_list(SAMPLE);
        let mut bonuses = HashMap::new();
        // Nothing looked up yet: no pick, never a guess
        assert!(best(&list, &bonuses, &[]).is_none());
        bonuses.insert(
            "cech".to_string(),
            Bonus {
                treasure: 40,
                instructor: 30,
            },
        );
        bonuses.insert(
            "rising phoenix".to_string(),
            Bonus {
                treasure: 60,
                instructor: 50,
            },
        );
        // Peklo would be best, but it is full
        bonuses.insert(
            "peklo".to_string(),
            Bonus {
                treasure: 200,
                instructor: 200,
            },
        );
        assert_eq!(best(&list, &bonuses, &[]).unwrap().name, "Rising Phoenix");
        // Refused once: the next best
        let tried = vec!["rising phoenix".to_string()];
        assert_eq!(best(&list, &bonuses, &tried).unwrap().name, "Cech");
        // Both already known: a second read has nothing to look at
        assert!(worth_a_look(&list, &bonuses).is_empty());
        let mut only_one = HashMap::new();
        only_one.insert("cech".to_string(), Bonus::default());
        assert_eq!(worth_a_look(&list, &only_one), vec!["Rising Phoenix"]);
    }
}
