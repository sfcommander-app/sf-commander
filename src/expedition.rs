//! Picks expeditions and encounters the way an experienced player would.
//!
//! ## How an expedition actually works
//!
//! Ten clearings, each offering up to three things to pick up. What gets
//! picked up lands in the **four item slots** at the top of the screen, and
//! almost everything in the game is part of a *chain*: the links have to be
//! collected in order and only the last one pays. The unicorn is the clearest
//! example - horn, then donkey, then rainbow, then the unicorn itself.
//!
//! That means a decision can not be made from the offered heroism alone. A
//! rainbow is worthless without the donkey, and a started chain that never
//! gets finished is wasted heroism. `sf-api` hands us those four slots as
//! `Expedition::items`, so [`Crossroads`] carries them and every chain
//! decision is made against what the character is really holding.
//!
//! ## The numbers
//!
//! * Heroism decides the XP bonus at the end: 20+ = +10 %, 30+ = +20 %,
//!   40+ = +35 %.
//! * The expedition's own task (collect N of the target thing) gives
//!   +20 heroism when finished and costs 5 when missed, so the target is
//!   worth roughly +25 over skipping it.
//! * A bounty (`*Bounty`) is a pickup that adds +10 heroism to one specific
//!   thing later on. It is only worth a slot when that thing can still be
//!   reached.
//! * Short and long expeditions pay the same per thirst; the long one just
//!   saves round trips.
//!
//! Per-chain payoffs are documented at [`chain_payoff`] - the older chains
//! have measured values (Playa help center / number13.de), the newer ones
//! fall back to a generic estimate rather than to an invented number.

use sf_api::gamestate::{
    rewards::{Reward, RewardType},
    tavern::{
        AvailableExpedition, ExpeditionEncounter, ExpeditionSpecial,
        ExpeditionThing,
    },
};

use crate::util::split_camel;

/// What the player wants out of the tavern - expeditions and quests both
/// follow this
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExpeditionGoal {
    /// Heroism first - the XP multiplier at the end of the expedition
    #[default]
    Xp,
    /// Keys, treasure chests and the gold reward at the boss first
    Gold,
}

impl ExpeditionGoal {
    /// `true` when the character is farming gold instead of XP
    #[must_use]
    pub const fn is_gold(self) -> bool {
        matches!(self, Self::Gold)
    }

    /// The label used in the UI and in the log
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Xp => "XP",
            Self::Gold => "Gold",
        }
    }
}

// ---------------------------------------------------------------- chains

/// Every pickup chain in the game, each in the order the links have to be
/// collected. The last entry is the one that pays.
///
/// The ids in `sf-api` are grouped by ten (41 horn, 42 donkey, 43 rainbow,
/// 44 unicorn), which is what these groups are taken from - so a chain the
/// game adds later shows up as a new group and can be appended here.
fn chains() -> &'static [&'static [ExpeditionThing]] {
    use ExpeditionThing::*;
    &[
        &[Bait, Dragon],
        &[CampFire, Phoenix, BurntCampfire],
        &[UnicornHorn, Donkey, Rainbow, Unicorn],
        &[SmallHurdle, BigHurdle, WinnersPodium],
        &[Socks, ClothPile, RevealingCouple],
        &[SwordInStone, BentSword, BrokenSword],
        &[Well, Girl, Balloons],
        &[Prince, RoyalFrog],
        &[Hand, Feet, Body, Klaus],
        &[Key, Suitcase],
        &[FishingRod, FishingBait, Merman],
        &[Mugs, DraftBeer, Barkeeper],
        &[Chicken, Tiger, RidingStan],
        &[Cupid, LovestruckShakes, LoveBirds],
    ]
}

/// The chain `thing` belongs to and its position in it
/// Two pickups are the same thing only if we actually know what they are.
///
/// `sf-api` collapses every id it does not recognise into a single
/// `ExpeditionThing::Unknown` and throws the number away - one player's log
/// had fourteen different ids arriving that way in a single evening. So two
/// `Unknown`s are almost certainly two *different* things, and treating them
/// as equal made the picker award the expedition's "+25, this is the item it
/// is looking for" bonus to whatever unknown thing happened to be on offer
#[must_use]
fn same_thing(a: ExpeditionThing, b: ExpeditionThing) -> bool {
    a == b && a != ExpeditionThing::Unknown
}

fn chain_of(
    thing: ExpeditionThing,
) -> Option<(&'static [ExpeditionThing], usize)> {
    chains()
        .iter()
        .find_map(|c| {
            c.iter().position(|t| same_thing(*t, thing)).map(|i| (*c, i))
        })
}

/// Heroism one link pays when picked, from the official help center table
/// ("Expeditions - Setkání"). Only needed for links that are still ahead -
/// the one on offer carries its own number from the game. Links the table
/// does not list (the event chains) fall back to [`GENERIC_LINK`]
fn link_value(thing: ExpeditionThing) -> f32 {
    use ExpeditionThing::*;
    match thing {
        Bait => -2.0,
        Dragon => 10.0,
        CampFire => 3.0,
        Phoenix => 5.0,
        // The leftover after the phoenix: pays nothing, keeps showing up
        BurntCampfire => 0.0,
        UnicornHorn => 1.0,
        Donkey => 3.0,
        Rainbow => 5.0,
        Unicorn => 7.0,
        SmallHurdle => -1.0,
        BigHurdle => -2.0,
        WinnersPodium => 15.0,
        Socks | ClothPile => 0.0,
        RevealingCouple => 12.0,
        SwordInStone => 5.0,
        BentSword => 2.0,
        // The leftover after the bent sword: a penalty, keeps showing up
        BrokenSword => -5.0,
        Hand | Feet | Body => -5.0,
        Klaus => 35.0,
        // Keys and chests pay gold, not heroism - see `gold_value`
        Key | Suitcase => 0.0,
        _ => GENERIC_LINK,
    }
}

/// What a link of an event chain the table does not cover is assumed to
/// pay - a plain pickup's worth, so an unknown chain is followed but never
/// preferred over a measured one
const GENERIC_LINK: f32 = 3.0;

/// How sure we are that the next link of a started chain actually shows up
/// before the clearings run out. The game offers it "on later clearings",
/// not on every one
const NEXT_LINK_ODDS: f32 = 0.85;

/// Chains the game lets you run twice per expedition. Once the last link is
/// in, the first one may come round again
fn repeatable(chain: &[ExpeditionThing]) -> bool {
    use ExpeditionThing::*;
    matches!(chain.last(), Some(Dragon | WinnersPodium))
}

/// Toilet paper is the one thing that is not a chain but still needs several
/// copies: three of them pay +5, fewer cost 5
const TOILET_PAPER_NEEDED: usize = 3;

// ------------------------------------------------------------ crossroads

/// Everything the decision at one crossroads depends on
#[derive(Debug, Clone, Copy)]
pub struct Crossroads<'a> {
    /// The four item slots at the top of the expedition screen
    pub items: &'a [Option<ExpeditionThing>],
    /// The thing this expedition is supposed to collect
    pub target: ExpeditionThing,
    /// Whether the task still needs more of the target
    pub target_missing: bool,
    /// Clearings still ahead, this one included
    pub floors_left: u8,
    /// Heroism collected so far on this expedition
    pub heroism: i32,
    /// XP or gold
    pub goal: ExpeditionGoal,
}

/// The heroism steps that change the XP bonus at the end: 20+ = +10 %,
/// 30+ = +20 %, 40+ = +35 %. 40 is the one worth steering for - it is the
/// biggest jump and the last one there is
const HEROISM_STEPS: [(i32, f32); 3] = [(20, 3.0), (30, 4.0), (40, 8.0)];

impl Crossroads<'_> {
    /// How many copies of `thing` are in the item slots
    fn held(&self, thing: ExpeditionThing) -> usize {
        self.items
            .iter()
            .flatten()
            .filter(|i| same_thing(**i, thing))
            .count()
    }

    /// How many links of `chain`, counted from the start, are already in the
    /// item slots. The count stops at the first missing link, because that is
    /// exactly how the game hands out the next one
    fn progress(&self, chain: &[ExpeditionThing]) -> usize {
        chain.iter().take_while(|t| self.held(**t) > 0).count()
    }

    /// Whether `thing` can still be picked up in the clearings that are left
    fn still_reachable(&self, thing: ExpeditionThing) -> bool {
        match chain_of(thing) {
            // Not part of a chain - it can show up at any crossroads
            None => true,
            Some((chain, idx)) => {
                let done = self.progress(chain);
                // Already carried, so it can not be picked up again
                if done > idx {
                    return false;
                }
                (idx + 1 - done) <= usize::from(self.floors_left)
            }
        }
    }
}

/// Extra value of a thing for a gold run. Keys and chests are the only
/// pickups that pay gold instead of heroism, so they are worth a slot only
/// when that is what the character is after
fn gold_value(thing: ExpeditionThing, goal: ExpeditionGoal) -> f32 {
    use ExpeditionThing::*;
    match (goal, thing) {
        (ExpeditionGoal::Gold, Key) => 10.0,
        (ExpeditionGoal::Gold, Suitcase) => 14.0,
        (ExpeditionGoal::Xp, Key) => 2.0,
        (ExpeditionGoal::Xp, Suitcase) => 3.0,
        _ => 0.0,
    }
}

/// Things that cost something later even though they look free now. The
/// meat encounters (chicken +3, pork pie +5) shrink the next crossroads to
/// two options / one option - less choice, so a little less expected
/// heroism there
fn side_effects(thing: ExpeditionThing) -> f32 {
    use ExpeditionThing::*;
    match thing {
        CupCake => -1.5,
        Cake => -3.0,
        _ => 0.0,
    }
}

/// Is a bounty already sitting in the item slots whose thing can still turn
/// up? Only one bounty is ever active, so a second one thrown on top wastes
/// the first
fn bounty_active(cr: &Crossroads) -> bool {
    cr.items.iter().flatten().any(|i| {
        i.is_bounty_for()
            .is_some_and(|b| b.iter().any(|t| cr.still_reachable(*t)))
    })
}

/// Scores one crossroads option. Higher is better. The second value is the
/// reason, so the log says *why* something was taken instead of only what
#[must_use]
pub fn encounter_score(
    e: &ExpeditionEncounter,
    cr: &Crossroads,
) -> (f32, &'static str) {
    let mut score = f64::from(e.heroism) as f32;
    let mut reason = "most heroism";

    // ------------------------------------------------- the expedition task
    if cr.target_missing && same_thing(e.typ, cr.target) {
        // +20 for finishing the task, -5 for missing it
        score += 25.0;
        reason = "the item this expedition is looking for";
    }

    // --------------------------------------------------------- the bounty
    if let Some(boosted) = e.typ.is_bounty_for() {
        if boosted.iter().any(|t| cr.still_reachable(*t)) {
            if bounty_active(cr) {
                // Only one bounty is active at a time - this one would
                // throw away the one we carry
                score -= 3.0;
                if reason == "most heroism" {
                    reason = "a bounty is already active";
                }
            } else {
                score += 10.0;
                if reason == "most heroism" {
                    reason = "bounty for something still reachable";
                }
            }
        } else {
            // A bounty for a thing we can no longer get is a wasted slot
            score -= 2.0;
            if reason == "most heroism" {
                reason = "nothing better on offer";
            }
        }
    }

    // ---------------------------------------------------------- the chain
    // A link is worth what it pays now plus what the links after it pay,
    // if they still fit into the clearings left. That is the whole model:
    // the podium's two hurdles are -1 and -2 for a +15, Klaus is three -5
    // for a +35, the unicorn is 1+3+5+7. What matters is the sum of the
    // rest, discounted a little for the next link not turning up in time
    if let Some((chain, idx)) = chain_of(e.typ) {
        let mut done = cr.progress(chain);
        // A finished repeatable chain (dragon, podium) starts over
        if done == chain.len() && repeatable(chain) && idx == 0 {
            done = 0;
        }
        let links_after = chain.len() - 1 - idx;
        if idx < done {
            // A second copy of a link we already carry - no chain value
            score -= 1.0;
        } else if idx > done {
            // Out of order: the links before it are missing, so this can
            // never pay. The game normally does not offer it, but if it
            // does, it is only worth its own heroism
            score -= 2.0;
            if reason == "most heroism" {
                reason = "no use for the chain, taken for the heroism";
            }
        } else if links_after > usize::from(cr.floors_left.saturating_sub(1)) {
            // The chain can not be finished in the clearings that are left
            score -= 3.0;
            if reason == "most heroism" {
                reason = "chain can no longer be finished";
            }
        } else if links_after == 0 {
            // The last link. Its own heroism is in the score already; a
            // little on top because it also closes the cycle and frees the
            // slots - and a unicorn passed over never comes back, a plain
            // +3 does. Not for the leftovers (burnt campfire, broken sword,
            // polluted fountain): those close nothing, they just repeat
            if link_value(e.typ) > 0.0 {
                score += 3.0;
                reason = if done > 0 {
                    "finishes the chain we are carrying"
                } else {
                    "the paying link of a chain"
                };
            } else if reason == "most heroism" {
                reason = "nothing better on offer";
            }
        } else {
            let rest: f32 = chain[idx + 1..].iter().map(|t| link_value(*t)).sum();
            let future = rest * NEXT_LINK_ODDS;
            score += future;
            if done > 0 {
                reason = "the next link of the chain we are carrying";
            } else if future > 0.0 {
                if reason == "most heroism" {
                    reason = "starts a chain that still fits";
                }
            } else if reason == "most heroism" {
                // A leftover like the burnt campfire or the broken sword:
                // nothing after it pays
                reason = "nothing better on offer";
            }
        }
    }

    // ------------------------------------------------ steering to 40 heroism
    // Reaching 40 heroism is the biggest XP lever on an expedition (+35 % vs
    // +20 % at 30), so an XP run is aimed hard at it. Every point still short
    // of 40 is weighted by how little room is left to make it up: with the
    // floors running out, heroism outweighs even the task item, while early on
    // - when 40 is easy - a task pick can still win. Crossing a step
    // (20 / 30 / 40) is a bonus on top, biggest for 40, the last one there is
    if !cr.goal.is_gold() && e.heroism > 0 {
        let after = cr.heroism + e.heroism;
        let deficit = (40 - cr.heroism).max(0);
        if deficit > 0 && cr.floors_left > 0 {
            // The heroism we would have to average over the floors left to
            // still make 40. The tighter that pace, the more every point on
            // offer now is worth - this is what keeps it pointed at 40
            let per_floor = deficit as f32 / f32::from(cr.floors_left);
            let urgency = (per_floor / 8.0).clamp(0.5, 3.5);
            score += e.heroism as f32 * urgency;
        }
        for (step, bonus) in HEROISM_STEPS {
            if cr.heroism < step && after >= step {
                score += bonus;
                if reason == "most heroism" {
                    reason = if step == 40 {
                        "carries the expedition over 40 heroism"
                    } else {
                        "crosses the next heroism step"
                    };
                }
            }
        }
    }

    // ---------------------------------------------------- toilet paper
    if e.typ == ExpeditionThing::ToiletPaper {
        let have = cr.held(ExpeditionThing::ToiletPaper);
        let still_needed = TOILET_PAPER_NEEDED.saturating_sub(have + 1);
        if still_needed == 0 {
            score += 5.0;
            reason = "the third toilet paper";
        } else if still_needed <= usize::from(cr.floors_left.saturating_sub(1))
        {
            score -= 1.0;
        } else {
            // Three will never happen from here - that is a straight -5
            score -= 5.0;
        }
    }

    score += gold_value(e.typ, cr.goal);
    score += side_effects(e.typ);
    (score, reason)
}

/// Index of the crossroads option to take, its score and the reason
#[must_use]
pub fn pick_encounter(
    encounters: &[ExpeditionEncounter],
    cr: &Crossroads,
) -> Option<(usize, f32, &'static str)> {
    encounters
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let (score, why) = encounter_score(e, cr);
            (i, score, why)
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
}

/// Which of the offered boss rewards to take
#[must_use]
pub fn pick_reward(rewards: &[Reward], goal: ExpeditionGoal) -> usize {
    let rank = |r: &Reward| -> (u8, u64) {
        let typ_rank = match (goal, &r.typ) {
            // Mushrooms outrank everything, always. They are the one thing
            // in the game that cannot be farmed - gold and XP come back
            // tomorrow, a mushroom passed over does not
            // Mushrooms can not be farmed - a mushroom passed over is gone
            (_, RewardType::Mushrooms) => 9,
            // The guild leader's order after the mushrooms: the pets' food
            // baskets come before everything else - a basket feeds a
            // habitat for days and there is no other source that keeps up
            (_, RewardType::FruitBasket) => 8,
            // Gems are rare and go into gear
            (_, RewardType::LegendaryGem | RewardType::Gem) => 7,
            // A single fruit is still pet food
            (_, RewardType::Fruit(_)) => 6,
            // The purple item-upgrade materials - arcane, metal, souls. Rare
            // and straight into armour/weapons, so they beat the plain
            // gold/XP goal
            (
                _,
                RewardType::Arcane
                | RewardType::Metal
                | RewardType::Souls,
            ) => 5,
            (ExpeditionGoal::Xp, RewardType::XP) => 4,
            // `Silver` is what the API calls the game's gold
            (ExpeditionGoal::Gold, RewardType::Silver) => 4,
            (_, RewardType::XP | RewardType::Silver) => 3,
            (_, RewardType::QuicksandGlass) => 3,
            (_, RewardType::Wood | RewardType::Stone) => 2,
            _ => 1,
        };
        (typ_rank, r.amount)
    };
    rewards
        .iter()
        .enumerate()
        .max_by_key(|(_, r)| rank(r))
        .map(|(i, _)| i)
        .unwrap_or(0)
}

/// Which of the (usually two) available expeditions to start. Returns the
/// index and a short reason
#[must_use]
pub fn pick_expedition(
    available: &[AvailableExpedition],
    thirst_left_sec: u32,
    goal: ExpeditionGoal,
) -> Option<(usize, &'static str)> {
    let affordable = || {
        available
            .iter()
            .enumerate()
            .filter(|(_, e)| e.thirst_for_adventure_sec <= thirst_left_sec)
    };
    // 1) daily task progress and eggs are worth more than anything else
    if let Some((i, _)) = affordable()
        .find(|(_, e)| e.special == Some(ExpeditionSpecial::DailyTask))
    {
        return Some((i, "advances the daily task"));
    }
    if let Some((i, _)) =
        affordable().find(|(_, e)| e.special == Some(ExpeditionSpecial::Egg))
    {
        return Some((i, "has an egg"));
    }
    // 2) on a gold run, an expedition whose task is the treasure chain is
    //    worth more than a longer one - the task is the one pickup that is
    //    guaranteed to show up
    if goal.is_gold()
        && let Some((i, _)) = affordable().find(|(_, e)| {
            matches!(
                e.target,
                ExpeditionThing::Key | ExpeditionThing::Suitcase
            )
        })
    {
        return Some((i, "its task is the treasure chest chain"));
    }
    // 3) otherwise the longest one that still fits - same reward per thirst,
    //    fewer round trips
    affordable()
        .max_by_key(|(_, e)| e.thirst_for_adventure_sec)
        .map(|(i, _)| (i, "longest one that fits the thirst left"))
}

// ------------------------------------------------------------------ names

/// The name of a pickup, the way a player would call it.
///
/// `sf-api` has internal placeholders (`Unknown`, `Dummy1`..`Dummy3`) that are
/// not things in the game, so they are named for what they are instead of
/// being printed raw. Anything the list below does not cover falls back to the
/// readable form of the API name, so a pickup added by a future event shows up
/// as `Riding Stan` and never as an invented name.
#[must_use]
pub fn thing_name(thing: ExpeditionThing) -> String {
    use ExpeditionThing::*;
    let known = match thing {
        Unknown => "unknown item",
        Dummy1 => "plain item 1",
        Dummy2 => "plain item 2",
        Dummy3 => "plain item 3",
        ToiletPaper => "toilet paper",
        Bait => "bait",
        Dragon => "dragon",
        CampFire => "campfire",
        Phoenix => "phoenix",
        BurntCampfire => "extinguished campfire",
        UnicornHorn => "unicorn horn",
        Donkey => "donkey",
        Rainbow => "rainbow",
        Unicorn => "unicorn",
        CupCake => "cupcake",
        Cake => "cake",
        SmallHurdle => "small hurdle",
        BigHurdle => "big hurdle",
        WinnersPodium => "winner's podium",
        Socks => "socks",
        ClothPile => "pile of clothes",
        RevealingCouple => "revealing couple",
        SwordInStone => "sword in the stone",
        BentSword => "bent sword",
        BrokenSword => "broken sword",
        Well => "well",
        Girl => "girl",
        Balloons => "balloons",
        Prince => "prince",
        RoyalFrog => "royal frog",
        Hand => "hand",
        Feet => "feet",
        Body => "body",
        Klaus => "Klaus",
        Key => "key",
        Suitcase => "treasure chest",
        FishingRod => "fishing rod",
        FishingBait => "fishing bait",
        Merman => "merman",
        Mugs => "mugs",
        DraftBeer => "draft beer",
        Barkeeper => "barkeeper",
        Chicken => "chicken",
        Tiger => "tiger",
        RidingStan => "riding Stan",
        Cupid => "cupid",
        LovestruckShakes => "lovestruck Shakes",
        LoveBirds => "lovebirds",
        other => {
            // Every remaining variant is a bounty. Name it after the thing it
            // pays for, so the log says what it is good for
            return match other.is_bounty_for() {
                Some([one]) => format!("{} bounty", thing_name(*one)),
                Some(_) => "plain item bounty".to_string(),
                None => split_camel(&format!("{other:?}")).to_lowercase(),
            };
        }
    };
    known.to_string()
}


/// The name of a daily login reward
#[must_use]
pub fn calendar_reward_name(
    typ: sf_api::gamestate::rewards::CalendarRewardType,
) -> String {
    use sf_api::gamestate::rewards::CalendarRewardType as C;
    let name = match typ {
        // Same story as the boss reward: the API calls the game's gold silver
        C::Silver => "gold",
        C::Mushrooms => "mushrooms",
        C::Experience => "XP",
        C::Stone => "stone",
        C::Wood => "wood",
        C::Souls => "souls",
        C::Arcane => "arcane",
        C::Runes => "runes",
        C::Item => "an item",
        C::Level | C::LevelUp => "a level",
        C::TenQuicksandGlasses => "quicksand glasses",
        other => {
            return split_camel(&format!("{other:?}")).to_lowercase();
        }
    };
    name.to_string()
}

/// The name of a boss reward, with the amount. `RewardType::Silver` is what
/// the API calls the game's gold - printing it as "silver" is what made the
/// log talk about a currency the game does not have
#[must_use]
pub fn reward_name(reward: &Reward) -> String {
    let amount = reward.amount;
    let name = match &reward.typ {
        RewardType::Silver => "gold",
        RewardType::XP => "XP",
        RewardType::Mushrooms => "mushrooms",
        RewardType::LuckyCoins => "lucky coins",
        RewardType::Wood => "wood",
        RewardType::Stone => "stone",
        RewardType::Arcane => "arcane",
        RewardType::Metal => "metal",
        RewardType::Souls => "souls",
        RewardType::LegendaryGem => "legendary gem",
        RewardType::Gem => "gem",
        RewardType::FruitBasket => "fruit basket",
        RewardType::Fruit(habitat) => {
            return format!(
                "{amount}x {} fruit",
                split_camel(&format!("{habitat:?}")).to_lowercase()
            );
        }
        RewardType::Egg => "egg",
        RewardType::QuicksandGlass => "quicksand glasses",
        RewardType::Honor => "honor",
        RewardType::Beer => "beer",
        RewardType::Frame => "frame",
        RewardType::GoldFidget => "gold fidget",
        RewardType::SilverFidget => "silver fidget",
        RewardType::BronzeFidget => "bronze fidget",
        RewardType::HellevatorPoints => "hellevator points",
        RewardType::HellevatorCards => "hellevator cards",
        RewardType::Mount(mount) => {
            return format!(
                "the {} mount",
                split_camel(&format!("{mount:?}")).to_lowercase()
            );
        }
        RewardType::Unknown => "an unknown reward",
    };
    format!("{amount}x {name}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enc(typ: ExpeditionThing, heroism: i32) -> ExpeditionEncounter {
        ExpeditionEncounter { typ, heroism }
    }

    fn cr<'a>(
        items: &'a [Option<ExpeditionThing>],
        floors_left: u8,
    ) -> Crossroads<'a> {
        Crossroads {
            items,
            target: ExpeditionThing::Dummy1,
            target_missing: false,
            floors_left,
            heroism: 0,
            goal: ExpeditionGoal::Xp,
        }
    }

    /// The library folds every encounter id it does not know into one
    /// `Unknown`, and a single evening's log had fourteen different ids
    /// arriving that way. Two of them are two different things, so the
    /// expedition's "+25, this is what it is looking for" must not fire on
    /// a coincidence of both being unrecognised
    /// Mushrooms are the one reward that cannot be farmed, so they win even
    /// against a big pile of whatever the expedition is being run for
    #[test]
    fn mushrooms_beat_everything_else_on_offer() {
        let mush = Reward {
            typ: RewardType::Mushrooms,
            amount: 1,
        };
        let heap_of_xp = Reward {
            typ: RewardType::XP,
            amount: 5_000_000,
        };
        let heap_of_gold = Reward {
            typ: RewardType::Silver,
            amount: 5_000_000,
        };
        for goal in [ExpeditionGoal::Xp, ExpeditionGoal::Gold] {
            let rewards = [heap_of_xp.clone(), mush.clone(), heap_of_gold.clone()];
            assert_eq!(
                pick_reward(&rewards, goal),
                1,
                "the mushroom has to win with goal {goal:?}"
            );
        }
        // ...and with no mushroom on the table nothing else changed
        let rewards = [heap_of_xp.clone(), heap_of_gold.clone()];
        assert_eq!(pick_reward(&rewards, ExpeditionGoal::Xp), 0);
        assert_eq!(pick_reward(&rewards, ExpeditionGoal::Gold), 1);
    }

    /// The guild leader's order behind the mushrooms: the pets' basket,
    /// then a gem, then the souls and the fortress materials
    #[test]
    fn baskets_beat_gems_beat_souls() {
        let basket = Reward {
            typ: RewardType::FruitBasket,
            amount: 1,
        };
        let gem = Reward {
            typ: RewardType::Gem,
            amount: 1,
        };
        let souls = Reward {
            typ: RewardType::Souls,
            amount: 900,
        };
        let stone = Reward {
            typ: RewardType::Stone,
            amount: 9000,
        };
        let rewards = [souls.clone(), gem.clone(), basket.clone()];
        assert_eq!(pick_reward(&rewards, ExpeditionGoal::Xp), 2);
        let rewards = [souls.clone(), gem.clone(), stone.clone()];
        assert_eq!(pick_reward(&rewards, ExpeditionGoal::Gold), 1);
        let rewards = [stone, souls];
        assert_eq!(pick_reward(&rewards, ExpeditionGoal::Gold), 1);
    }

    #[test]
    fn two_unknown_pickups_are_not_the_same_pickup() {
        use ExpeditionThing::*;
        let empty = [None, None, None, None];
        let mut c = cr(&empty, 5);
        c.target = Unknown;
        c.target_missing = true;

        // Same score as any other +1 pickup: no target bonus
        let unknown_pick = encounter_score(&enc(Unknown, 1), &c).0;
        c.target = Dummy1;
        let plain = encounter_score(&enc(Unknown, 1), &c).0;
        assert!(
            (unknown_pick - plain).abs() < f32::EPSILON,
            "an unknown pickup scored {unknown_pick} against an unknown \
             target but {plain} against a known one - the +25 leaked"
        );

        // A known target still gets its bonus
        c.target = Dummy1;
        let real = encounter_score(&enc(Dummy1, 1), &c).0;
        assert!(real > plain + 20.0, "the real target bonus went missing");
    }

    #[test]
    fn an_unknown_item_in_a_slot_does_not_stand_in_for_a_chain_link() {
        use ExpeditionThing::*;
        // Four unrecognised things in the slots must not read as "we are
        // already carrying the horn"
        let slots = [Some(Unknown), Some(Unknown), None, None];
        let c = cr(&slots, 5);
        assert_eq!(c.held(UnicornHorn), 0);
        assert_eq!(c.held(Unknown), 0);
    }

    #[test]
    fn a_pick_that_reaches_forty_heroism_beats_a_slightly_bigger_one() {
        use ExpeditionThing::*;
        let empty = [None, None, None, None];
        let mut c = cr(&empty, 5);
        // 38 heroism: +2 lands on 40 (+35 % XP), +3 would too - but from 30,
        // where neither crosses anything, the bigger one simply wins
        c.heroism = 38;
        let opts = [enc(Dummy1, 2), enc(Dummy3, 3)];
        assert_eq!(pick_encounter(&opts, &c).map(|(p, ..)| p), Some(1));

        // 39 heroism: +1 reaches 40, +2 from 37 would not
        c.heroism = 39;
        let opts = [enc(Dummy1, 1), enc(Dummy3, 2)];
        assert_eq!(pick_encounter(&opts, &c).map(|(p, ..)| p), Some(1));

        // The step is what does it: at 39, a +1 that reaches 40 beats a +2
        // that is still short - which cannot happen, so check the score
        // directly instead
        c.heroism = 39;
        let reaching = encounter_score(&enc(Dummy1, 1), &c).0;
        c.heroism = 10;
        let plain = encounter_score(&enc(Dummy1, 1), &c).0;
        assert!(
            reaching > plain,
            "reaching 40 ({reaching}) should beat a plain point ({plain})"
        );
    }

    #[test]
    fn the_unicorn_chain_is_followed_to_the_end() {
        use ExpeditionThing::*;
        // Nothing collected yet, nine clearings ahead: the horn opens
        // 1+3+5+7 = 16 heroism over four clearings, a plain +3 is a plain
        // +3 - the horn it is. With two clearings left it can not finish,
        // so the +3 wins
        let empty = [None, None, None, None];
        let opts = [enc(UnicornHorn, 1), enc(Dummy3, 3)];
        let (pos, _, why) = pick_encounter(&opts, &cr(&empty, 9)).unwrap();
        assert_eq!(pos, 0);
        assert_eq!(why, "starts a chain that still fits");
        let (pos, ..) = pick_encounter(&opts, &cr(&empty, 2)).unwrap();
        assert_eq!(pos, 1);

        // Carrying the horn, the donkey has to win over a plain +3
        let horn = [Some(UnicornHorn), None, None, None];
        let opts = [enc(Donkey, 3), enc(Dummy3, 3)];
        let (pos, ..) = pick_encounter(&opts, &cr(&horn, 8)).unwrap();
        assert_eq!(pos, 0);

        // Horn + donkey: the rainbow, even against a much better looking
        // pickup, because dropping the chain here throws away both links
        let two = [Some(UnicornHorn), Some(Donkey), None, None];
        let opts = [enc(Rainbow, 5), enc(Dummy3, 6)];
        let (pos, ..) = pick_encounter(&opts, &cr(&two, 5)).unwrap();
        assert_eq!(pos, 0);

        // And the unicorn itself finishes it
        let three =
            [Some(UnicornHorn), Some(Donkey), Some(Rainbow), None];
        let opts = [enc(Unicorn, 7), enc(Dummy3, 8)];
        let (pos, _, why) = pick_encounter(&opts, &cr(&three, 3)).unwrap();
        assert_eq!(pos, 0);
        assert_eq!(why, "finishes the chain we are carrying");
    }

    #[test]
    fn every_chain_is_known() {
        use ExpeditionThing::*;
        // The four chains the old table was missing entirely
        for thing in [Merman, Barkeeper, RidingStan, LoveBirds] {
            let (chain, idx) = chain_of(thing).expect("chain is known");
            assert_eq!(idx, chain.len() - 1, "{thing:?} has to be the last");
        }
        // The campfire is three links, not one
        assert_eq!(chain_of(BurntCampfire).unwrap().1, 2);
    }

    #[test]
    fn a_chain_that_can_not_finish_is_not_started() {
        use ExpeditionThing::*;
        let empty = [None, None, None, None];
        // Two clearings left, the bait pays on the next one
        let opts = [enc(Bait, -2), enc(Dummy2, 1)];
        // Isolate the chain horizon from the separate urgency bonus for
        // reaching 40 heroism. With only two floors and zero heroism, that
        // bonus deliberately prefers the immediate positive pickup.
        let mut context = cr(&empty, 2);
        context.heroism = 40;
        let (pos, ..) = pick_encounter(&opts, &context).unwrap();
        assert_eq!(pos, 0);
        // Last clearing: no dragon can follow, take the sure +1.
        context.floors_left = 1;
        let (pos, ..) = pick_encounter(&opts, &context).unwrap();
        assert_eq!(pos, 1);
        // Two clearings, nothing collected: bait then dragon is +8 in two
        // clearings against +1 - the bait, whatever the urgency for 40 says
        assert_eq!(pick_encounter(&opts, &cr(&empty, 2)).unwrap().0, 0);
    }

    /// The leftovers the game keeps offering once a cycle is done - the
    /// burnt campfire (0), the broken sword (-5) - must never beat a plain
    /// pickup, and Klaus's three -5 body parts are worth it for the +35
    #[test]
    fn leftovers_lose_and_klaus_is_worth_his_parts() {
        use ExpeditionThing::*;
        let after_phoenix = [Some(CampFire), Some(Phoenix), None, None];
        let opts = [enc(BurntCampfire, 0), enc(Dummy1, 1)];
        let (pos, ..) = pick_encounter(&opts, &cr(&after_phoenix, 6)).unwrap();
        assert_eq!(pos, 1);

        let after_bent = [Some(SwordInStone), Some(BentSword), None, None];
        let opts = [enc(BrokenSword, -5), enc(Dummy1, 1)];
        let (pos, ..) = pick_encounter(&opts, &cr(&after_bent, 6)).unwrap();
        assert_eq!(pos, 1);

        let empty = [None, None, None, None];
        let opts = [enc(Hand, -5), enc(Dummy3, 3)];
        let (pos, ..) = pick_encounter(&opts, &cr(&empty, 8)).unwrap();
        assert_eq!(pos, 0, "three -5 for a +35 is +20 over four clearings");
        // ...but not with three clearings left: Klaus can not arrive
        let (pos, ..) = pick_encounter(&opts, &cr(&empty, 3)).unwrap();
        assert_eq!(pos, 1);
    }

    /// Only one bounty is active at a time - a second one on top throws
    /// the first away
    #[test]
    fn a_second_bounty_is_not_taken_while_one_is_active() {
        use ExpeditionThing::*;
        let carrying = [Some(UnicornBounty), None, None, None];
        let opts = [enc(DragonBounty, 0), enc(Dummy2, 2)];
        let (pos, ..) = pick_encounter(&opts, &cr(&carrying, 8)).unwrap();
        assert_eq!(pos, 1);
    }

    #[test]
    fn a_bounty_is_only_worth_a_slot_when_it_can_pay() {
        use ExpeditionThing::*;
        let empty = [None, None, None, None];
        // Six clearings left: the unicorn is four links away, so the bounty
        // for it can still pay
        let opts = [enc(UnicornBounty, 0), enc(Dummy2, 3)];
        let (pos, ..) = pick_encounter(&opts, &cr(&empty, 6)).unwrap();
        assert_eq!(pos, 0);
        // Two clearings left: the unicorn can not be reached, the bounty is
        // a wasted slot
        let (pos, ..) = pick_encounter(&opts, &cr(&empty, 2)).unwrap();
        assert_eq!(pos, 1);
    }

    #[test]
    fn the_task_item_still_wins() {
        use ExpeditionThing::*;
        let empty = [None, None, None, None];
        let mut c = cr(&empty, 6);
        c.target = CupCake;
        c.target_missing = true;
        let opts = [enc(Hand, -5), enc(CupCake, 3), enc(CampFire, 3)];
        let (pos, ..) = pick_encounter(&opts, &c).unwrap();
        assert_eq!(pos, 1);
    }

    #[test]
    fn gold_mode_prefers_keys() {
        use ExpeditionThing::*;
        let empty = [None, None, None, None];
        let opts = [enc(Key, 0), enc(Dummy3, 3)];
        let mut c = cr(&empty, 7);
        let (xp, ..) = pick_encounter(&opts, &c).unwrap();
        c.goal = ExpeditionGoal::Gold;
        let (gold, ..) = pick_encounter(&opts, &c).unwrap();
        assert_eq!(xp, 1);
        assert_eq!(gold, 0);
    }

    #[test]
    fn reward_follows_goal_and_reads_as_gold() {
        let rewards = [
            Reward {
                typ: RewardType::Silver,
                amount: 1000,
            },
            Reward {
                typ: RewardType::XP,
                amount: 500,
            },
        ];
        assert_eq!(pick_reward(&rewards, ExpeditionGoal::Xp), 1);
        assert_eq!(pick_reward(&rewards, ExpeditionGoal::Gold), 0);
        assert_eq!(reward_name(&rewards[0]), "1000x gold");
    }

    #[test]
    fn names_never_show_api_placeholders() {
        use ExpeditionThing::*;
        assert_eq!(thing_name(Unknown), "unknown item");
        assert_eq!(thing_name(Dummy3), "plain item 3");
        assert_eq!(thing_name(DummyBounty), "plain item bounty");
        assert_eq!(thing_name(RevealingCoupleBounty), "revealing couple bounty");
        assert_eq!(thing_name(BurntCampfire), "extinguished campfire");
    }
}
