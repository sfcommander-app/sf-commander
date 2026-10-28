//! Picks one of the three tavern quests.
//!
//! A quest costs thirst for adventure equal to its length, so the only thing
//! that matters is what it pays *per minute of thirst* - a 10 minute quest
//! paying 1000 XP is worth exactly as much as a 20 minute one paying 2000.
//! Which of the two numbers is compared depends on what the character is
//! after, which is the same XP / gold switch the expeditions use.
//!
//! Quests only come up when there is no expedition event running (or when
//! expeditions are switched off). During an event the expedition is always
//! the better use of the thirst.

use sf_api::gamestate::tavern::Quest;

use crate::expedition::ExpeditionGoal;

/// The quest that was picked, with the numbers behind the decision
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestPick {
    /// Position in `tavern.quests`, 0-2
    pub pos: usize,
    /// Ready for the log
    pub why: String,
}

/// What one quest pays per minute of thirst for the chosen goal
fn per_minute(quest: &Quest, goal: ExpeditionGoal) -> f64 {
    let minutes = f64::from(quest.base_length.max(1)) / 60.0;
    let pay = match goal {
        ExpeditionGoal::Xp => f64::from(quest.base_experience),
        ExpeditionGoal::Gold => f64::from(quest.base_silver),
    };
    pay / minutes
}

/// Picks the quest with the best return for the goal that still fits into the
/// thirst that is left. Quests that pay an item win ties, because an item is
/// a scrapbook entry the other two do not have.
#[must_use]
pub fn pick_quest(
    quests: &[Quest],
    thirst_left_sec: u32,
    goal: ExpeditionGoal,
) -> Option<QuestPick> {
    let (pos, best) = quests
        .iter()
        .enumerate()
        .filter(|(_, q)| q.base_length > 0 && q.base_length <= thirst_left_sec)
        .max_by(|(_, a), (_, b)| {
            per_minute(a, goal)
                .total_cmp(&per_minute(b, goal))
                .then_with(|| a.item.is_some().cmp(&b.item.is_some()))
        })?;

    let rate = per_minute(best, goal);
    let minutes = best.base_length / 60;
    let why = format!(
        "{:.0} {} per minute over {} min{}",
        rate,
        goal.label(),
        minutes.max(1),
        if best.item.is_some() {
            ", pays an item"
        } else {
            ""
        }
    );
    Some(QuestPick { pos, why })
}

#[cfg(test)]
mod tests {
    use sf_api::gamestate::tavern::{Location, Quest};

    use super::*;

    fn quest(length_min: u32, silver: u32, xp: u32) -> Quest {
        Quest {
            base_length: length_min * 60,
            base_silver: silver,
            base_experience: xp,
            item: None,
            location_id: Location::SprawlingJungle,
            monster_id: 0,
        }
    }

    #[test]
    fn the_goal_decides_which_quest_wins() {
        // Same length: one pays XP, the other gold
        let quests = [
            quest(10, 5000, 100),
            quest(10, 100, 5000),
            quest(10, 200, 200),
        ];
        assert_eq!(
            pick_quest(&quests, 3600, ExpeditionGoal::Xp).unwrap().pos,
            1
        );
        assert_eq!(
            pick_quest(&quests, 3600, ExpeditionGoal::Gold).unwrap().pos,
            0
        );
    }

    #[test]
    fn the_rate_matters_not_the_total() {
        // The long quest pays more in total, the short one pays more per
        // minute of thirst - and thirst is the thing that runs out
        let quests = [quest(20, 0, 2000), quest(5, 0, 1000)];
        assert_eq!(
            pick_quest(&quests, 3600, ExpeditionGoal::Xp).unwrap().pos,
            1
        );
    }

    #[test]
    fn a_quest_longer_than_the_thirst_is_skipped() {
        let quests = [quest(20, 0, 9999), quest(5, 0, 100)];
        // Only 6 minutes of thirst left - the fat one is out of reach
        let pick = pick_quest(&quests, 6 * 60, ExpeditionGoal::Xp).unwrap();
        assert_eq!(pick.pos, 1);
        // Nothing fits at all
        assert!(pick_quest(&quests, 60, ExpeditionGoal::Xp).is_none());
    }
}
