//! Shared game rules used by both automation and the UI.
//! Keep server counters, configuration ceilings and actual game limits distinct.

use sf_api::gamestate::{
    GameState,
    character::Mount,
    rewards::Event,
    underworld::{Underworld, UnderworldBuildingType},
};

/// The thirst a day starts with, before any beer is drunk. One minute of
/// a quest costs one point
pub const THIRST_BASE_MIN: u32 = 100;
/// What one beer is worth
pub const THIRST_PER_BEER_MIN: u32 = 20;

/// How high a character's thirst can go in one day, in minutes.
///
/// This was a flat 300, which is right for ten beers and wrong for anybody
/// who has it: the witch's belt enchantment at level 66 grants an eleventh
/// beer, and eleven beers make the day's ceiling 320. A character with
/// "0/11" beers on the plate beside "0/300" is the app disagreeing with
/// itself in two boxes that are an inch apart.
///
/// `beer_max` is what the game itself reports for this character, so the
/// ceiling follows the belt, an event's extra beer, or anything else the
/// game decides to hand out, instead of being written down here
#[must_use]
pub fn thirst_ceiling_min(beer_max: u8) -> u32 {
    THIRST_BASE_MIN + u32::from(beer_max) * THIRST_PER_BEER_MIN
}

/// The thirst as the plate says it: what is there now, out of what the day
/// can hold.
///
/// Both screens that show this go through here, so they cannot drift. The
/// ceiling is never allowed below the figure standing in front of it - the
/// Adventuromat carries unspent thirst over midnight, so a day can open
/// with more than it would have earned, and a bar reading past its own end
/// is the sort of thing that gets reported as nonsense
#[must_use]
pub fn thirst_line(current_min: u32, beer_max: u8) -> String {
    format!("{current_min}/{}", thirst_ceiling_min(beer_max).max(current_min))
}

pub fn event_active(gs: &GameState, event: Event) -> bool {
    gs.specials.events.active.contains(&event)
        && gs
            .specials
            .events
            .ends
            .is_none_or(|end| end > chrono::Local::now())
}

pub fn pet_feed_limit(gs: &GameState) -> u16 {
    if event_active(gs, Event::AssemblyOfAwesomeAnimals) {
        9
    } else {
        3
    }
}

pub fn wheel_spin_limit(gs: &GameState) -> u8 {
    if event_active(gs, Event::LuckyDay) {
        40
    } else {
        20
    }
}

pub const WHEEL_COIN_PRICE: u32 = 10;

/// `lure_level` is the opponent level needed for full souls, NOT a quota.
pub fn daily_lures(underworld: &Underworld) -> u16 {
    u16::from(
        underworld.buildings[UnderworldBuildingType::Gate]
            .level
            .min(5),
    )
}

/// Which of the two build orders the underworld follows - the guild
/// leader's two phases. `Auto` switches on its own once the Heart of
/// Darkness reaches `AUTO_SWITCH_HEART`
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum UnderworldPhase {
    #[default]
    Auto,
    /// Gathering souls: everything that makes the lures pay
    Souls,
    /// The character's own bonuses: crit damage, gold, stored thirst
    Character,
}

impl UnderworldPhase {
    pub const ALL: [UnderworldPhase; 3] =
        [Self::Auto, Self::Souls, Self::Character];
}

impl std::fmt::Display for UnderworldPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(crate::i18n::t(match self {
            UnderworldPhase::Auto => "Auto (souls until the Heart is at 10)",
            UnderworldPhase::Souls => "Gathering souls",
            UnderworldPhase::Character => "Character bonuses",
        }))
    }
}

/// `Auto` builds for souls until the Heart of Darkness is this high: by
/// then the extractor, gate and chamber have had their turn and the lures
/// pay, and what is left to raise is what the character feels in a fight
pub const AUTO_SWITCH_HEART: u8 = 10;

/// The phase actually in force, `Auto` resolved by the heart's level
#[must_use]
pub fn underworld_phase(
    setting: UnderworldPhase,
    heart: u8,
) -> UnderworldPhase {
    match setting {
        UnderworldPhase::Auto if heart >= AUTO_SWITCH_HEART => {
            UnderworldPhase::Character
        }
        UnderworldPhase::Auto => UnderworldPhase::Souls,
        other => other,
    }
}

/// The order the buildings go up in.
///
/// **This is the guild leader's own order, in his words: the Heart, then
/// the Soul Extractor, then the Gladiator Trainer - and a level on the
/// Keeper *unit* now and then.** Four things, not ten.
///
/// The Heart is first because it caps the level of every other building,
/// so it is the only thing that ever unblocks anything (the code reads
/// that cap off the levels, it is not trusted from a wiki). The Extractor
/// is the engine that produces the souls everything else is paid for
/// with. The Trainer's crit damage is the one bonus down here that the
/// character carries into every fight above ground.
///
/// Everything after those three is a tail, not a plan: they are eligible
/// only once the souls the three need are covered, which in practice
/// means rarely. They are kept in the list rather than removed so that a
/// maxed-out underworld still has somewhere to put its surplus.
///
/// The Keeper *unit* is not in here at all - units are picked separately,
/// out of the souls the next building does not need, which is exactly
/// what "now and then" means.
#[must_use]
pub fn underworld_order(
    phase: UnderworldPhase,
) -> [UnderworldBuildingType; 10] {
    use UnderworldBuildingType as B;
    // The phase used to swap the order about. It no longer does: the
    // three that matter are the same either way, and the tail behind them
    // is surplus in both. The setting is kept because it still decides
    // which unit goes first (see the keeper, below)
    let _ = underworld_phase(phase, 0);
    [
        B::HeartOfDarkness,
        B::SoulExtractor,
        B::GladiatorTrainer,
        // ...and the surplus, in the order it is worth anything
        B::GoldPit,
        B::Adventuromatic,
        B::Gate,
        B::TortureChamber,
        B::Keeper,
        B::TrollBlock,
        B::GoblinPit,
    ]
}

pub fn remaining_lures(underworld: &Underworld) -> u16 {
    daily_lures(underworld).saturating_sub(underworld.lured_today)
}

/// The mount ladder, best first. The game's fourth mount is the dragon
/// (the griffin in some clients), for 25 mushrooms; the rest cost silver
pub const MOUNT_LADDER: [Mount; 4] =
    [Mount::Dragon, Mount::Tiger, Mount::Horse, Mount::Cow];

fn can_afford(mount: Mount, silver: u64, mushrooms: u32) -> bool {
    let cost = mount.cost();
    silver >= cost.silver && mushrooms >= u32::from(cost.mushrooms)
}

/// Which mount to rent now, in the guild leader's words: the dragon, then
/// the tiger, then the horse, and the cow only when nothing else can be
/// paid. The best one affordable is bought when there is no mount, when
/// the current one runs out within a day, and as an upgrade the moment it
/// is affordable; a mount that is running and as good or better is left
/// alone (the game refuses a downgrade onto a running one anyway)
pub fn mount_to_buy(
    current: Option<Mount>,
    ending_soon: bool,
    silver: u64,
    mushrooms: u32,
) -> Option<Mount> {
    let best = MOUNT_LADDER
        .into_iter()
        .find(|m| can_afford(*m, silver, mushrooms))?;
    match current {
        None => Some(best),
        Some(_) if ending_soon => Some(best),
        Some(cur) if (best as u8) > (cur as u8) => Some(best),
        Some(_) => None,
    }
}

/// How the day's mushroom budget is meant to be spent - the guild
/// leader's split: beer first (up to half, never over ten), then three
/// tenths of what is left on rerolls of the weapon shop, three tenths on
/// the magic shop, the rest on paid dungeon fights (XP). These are the
/// ceilings the bot works towards - the budget is a mandate to spend, not
/// only a ceiling not to cross
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MushroomPlan {
    /// Beers a day - one mushroom each
    pub beer: u8,
    /// Rerolls a day for each of the two shops
    pub per_shop: u8,
    /// Paid dungeon fights a day
    pub dungeon: u8,
}

/// The split for `budget` mushrooms a day; 0 spends nothing anywhere.
///
/// Beer comes first and is named here rather than left implicit: one
/// mushroom buys twenty minutes of thirst, which is a whole expedition's
/// worth of XP and gold - the best thing a mushroom turns into, and the
/// only one that expires at midnight whether it is used or not. It never
/// takes more than half the budget, so the shops and the dungeon still
/// get their share
#[must_use]
pub fn mushroom_plan(budget: u32) -> MushroomPlan {
    let beer = (budget / 2).min(10) as u8;
    let rest = budget.saturating_sub(u32::from(beer));
    let per_shop = (rest * 3 / 10).min(10) as u8;
    let dungeon = rest
        .saturating_sub(2 * u32::from(per_shop))
        .min(u32::from(u8::MAX)) as u8;
    MushroomPlan {
        beer,
        per_shop,
        dungeon,
    }
}

/// What the souls are being held for: the first building of the order
/// that can still go up, with what its next level costs.
///
/// One source of truth for two readers - the housekeeping loop, which
/// keeps everything behind this target from spending the souls it needs,
/// and the screen, which has to be able to say "saving for X" without
/// waiting for the loop to have run. It used to live only inside the
/// loop, so a page that had not been visited said nothing at all.
///
/// `None` when everything eligible sits at the Heart's cap - then there is
/// nothing to save for and the Heart itself is what goes up next.
#[must_use]
pub fn underworld_saving_for(
    uw: &sf_api::gamestate::underworld::Underworld,
    phase: UnderworldPhase,
) -> Option<(UnderworldBuildingType, u64)> {
    use UnderworldBuildingType as B;
    underworld_order(phase)
        .into_iter()
        .filter(|&b| b != B::HeartOfDarkness && underworld_eligible(uw, b))
        .map(|b| (b, uw.buildings[b].upgrade_cost.souls))
        .next()
}

/// Worth building at all: not capped by the Heart. The Gate hands out one
/// more lure a day up to level 5 and nothing whatsoever after, so it stops
/// there. The Pit may pass the Heart (the game allows it from 16 on), so
/// it is not held to the cap up there.
///
/// **The Gold Pit is the exception to the order**, by the guild leader's
/// rule: it only goes up when there is nothing else to build. It is the
/// one building down here that pays in silver rather than in anything the
/// character carries into a fight, so a level on it is souls the other
/// three wanted. `underworld_gold_pit_due` adds the once-a-day half of
/// that rule, which needs a calendar and so does not belong here
#[must_use]
pub fn underworld_eligible(
    uw: &sf_api::gamestate::underworld::Underworld,
    b: UnderworldBuildingType,
) -> bool {
    use UnderworldBuildingType as B;
    let heart = uw.buildings[B::HeartOfDarkness].level;
    let level = uw.buildings[b].level;
    match b {
        B::HeartOfDarkness => true,
        B::SoulExtractor => level <= heart,
        B::Gate => level < 5 && level < heart,
        B::GoldPit => {
            (level < heart || level >= 16) && nothing_else_to_build(uw)
        }
        _ => level < heart,
    }
}

/// None of the three that matter can go up right now
fn nothing_else_to_build(
    uw: &sf_api::gamestate::underworld::Underworld,
) -> bool {
    use UnderworldBuildingType as B;
    let heart = uw.buildings[B::HeartOfDarkness].level;
    let capped = |b: B| uw.buildings[b].level >= heart;
    // The Extractor may match the Heart (`<=`), the Trainer may not
    uw.buildings[B::SoulExtractor].level > heart && capped(B::GladiatorTrainer)
}

/// How long the souls still missing will take to arrive, at the rate the
/// underworld produces them. `None` when nothing is missing, or when
/// nothing is being produced - "never" is not a waiting time to print
#[must_use]
pub fn souls_eta_secs(
    uw: &sf_api::gamestate::underworld::Underworld,
    cost: u64,
) -> Option<i64> {
    use sf_api::gamestate::underworld::UnderworldResourceType;
    let missing = cost.checked_sub(uw.souls_current).filter(|m| *m > 0)?;
    let per_hour = uw.production[UnderworldResourceType::Souls].per_hour;
    if per_hour == 0 {
        return None;
    }
    Some((missing as f64 / per_hour as f64 * 3600.0) as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Beer takes half (never over ten), the rest splits three tenths to
    /// each shop and the remainder to paid dungeon fights; nothing from
    /// nothing, and a small budget still goes somewhere
    #[test]
    fn the_mushroom_budget_goes_to_beer_first_then_shops_then_dungeon() {
        // Beer takes half, never more than ten; the rest splits 3/3/4
        assert_eq!(
            mushroom_plan(10),
            MushroomPlan {
                beer: 5,
                per_shop: 1,
                dungeon: 3
            }
        );
        assert_eq!(
            mushroom_plan(0),
            MushroomPlan {
                beer: 0,
                per_shop: 0,
                dungeon: 0
            }
        );
        assert_eq!(
            mushroom_plan(1),
            MushroomPlan {
                beer: 0,
                per_shop: 0,
                dungeon: 1
            }
        );
        assert_eq!(
            mushroom_plan(100),
            MushroomPlan {
                beer: 10,
                per_shop: 10,
                dungeon: 70
            }
        );
        // Nothing is left on the table: the split adds up to the budget
        for b in [0u32, 1, 5, 10, 37, 100, 250] {
            let p = mushroom_plan(b);
            let spent = u32::from(p.beer)
                + 2 * u32::from(p.per_shop)
                + u32::from(p.dungeon);
            assert!(spent <= b, "{b}: plan spends {spent}");
            assert!(
                b < 2 || spent >= b - 2,
                "{b}: plan leaves {} unspent",
                b - spent
            );
        }
    }

    #[test]
    fn the_mount_ladder_takes_the_best_affordable() {
        let rich = (u64::MAX / 2, 100);
        // No mount: the dragon when the mushrooms are there, else down
        // the ladder
        assert_eq!(
            mount_to_buy(None, false, rich.0, rich.1),
            Some(Mount::Dragon)
        );
        assert_eq!(mount_to_buy(None, false, 100_000, 5), Some(Mount::Tiger));
        assert_eq!(mount_to_buy(None, false, 100_000, 0), Some(Mount::Horse));
        assert_eq!(mount_to_buy(None, false, 200, 0), Some(Mount::Cow));
        assert_eq!(mount_to_buy(None, false, 0, 0), None);
        // A running dragon is left alone; a running horse is upgraded to
        // the dragon the moment 25 mushrooms are there, never downgraded
        assert_eq!(
            mount_to_buy(Some(Mount::Dragon), false, rich.0, rich.1),
            None
        );
        assert_eq!(
            mount_to_buy(Some(Mount::Horse), false, rich.0, rich.1),
            Some(Mount::Dragon)
        );
        assert_eq!(mount_to_buy(Some(Mount::Tiger), false, 100_000, 0), None);
        // Running out: renewed with the best affordable, a bridge if need be
        assert_eq!(
            mount_to_buy(Some(Mount::Dragon), true, 100_000, 3),
            Some(Mount::Tiger)
        );
        assert_eq!(
            mount_to_buy(Some(Mount::Dragon), true, rich.0, rich.1),
            Some(Mount::Dragon)
        );
    }

    /// Auto follows the heart: souls first, the character's bonuses from
    /// ten on; a chosen phase is never overridden. The two orders differ
    /// The Gold Pit is the one building held back on purpose: it pays in
    /// silver, not in anything the character carries into a fight, so it
    /// only goes up once the three that matter cannot
    #[test]
    fn the_gold_pit_waits_until_there_is_nothing_else_to_build() {
        use UnderworldBuildingType as B;
        let mut u = Underworld::default();
        u.buildings[B::HeartOfDarkness].level = 10;
        // The Extractor can still go up, so the Pit may not
        u.buildings[B::SoulExtractor].level = 4;
        u.buildings[B::GladiatorTrainer].level = 10;
        u.buildings[B::GoldPit].level = 0;
        assert!(underworld_eligible(&u, B::SoulExtractor));
        assert!(!underworld_eligible(&u, B::GoldPit));
        assert_eq!(
            underworld_saving_for(&u, UnderworldPhase::Souls).map(|(b, _)| b),
            Some(B::SoulExtractor)
        );

        // Both of the three are at the cap: now the Pit is the only thing
        // left, and it is what the souls go on
        u.buildings[B::SoulExtractor].level = 11;
        assert!(underworld_eligible(&u, B::GoldPit));
        assert_eq!(
            underworld_saving_for(&u, UnderworldPhase::Souls).map(|(b, _)| b),
            Some(B::GoldPit)
        );
    }

    /// right after the heart - extractor against trainer
    #[test]
    fn the_underworld_phase_follows_the_heart_unless_chosen() {
        use UnderworldBuildingType as B;
        use UnderworldPhase as P;
        assert_eq!(underworld_phase(P::Auto, 0), P::Souls);
        assert_eq!(underworld_phase(P::Auto, AUTO_SWITCH_HEART - 1), P::Souls);
        assert_eq!(underworld_phase(P::Auto, AUTO_SWITCH_HEART), P::Character);
        assert_eq!(underworld_phase(P::Souls, 15), P::Souls);
        assert_eq!(underworld_phase(P::Character, 0), P::Character);
        // The guild leader's own order, and it does not depend on the
        // phase: the Heart, the Extractor, the Trainer. Everything after
        // those three is surplus
        for phase in [P::Souls, P::Character, P::Auto] {
            let order = underworld_order(phase);
            assert_eq!(
                order[..3],
                [B::HeartOfDarkness, B::SoulExtractor, B::GladiatorTrainer],
                "{phase:?}"
            );
            // Trolls and goblins close the list - nothing in the
            // underworld is worth less to the character
            assert_eq!(order[8..], [B::TrollBlock, B::GoblinPit]);
        }
    }

    #[test]
    fn lure_quota_comes_from_the_gate_not_the_opponent() {
        let mut u = Underworld {
            lure_level: 213,
            ..Underworld::default()
        };
        assert_eq!(remaining_lures(&u), 0);
        u.buildings[UnderworldBuildingType::Gate].level = 3;
        u.lured_today = 1;
        assert_eq!(remaining_lures(&u), 2);
        u.buildings[UnderworldBuildingType::Gate].level = 15;
        assert_eq!(daily_lures(&u), 5);
        u.lured_today = 6;
        assert_eq!(remaining_lures(&u), 0);
    }

    #[test]
    fn event_limits_change_together_and_expire() {
        let mut gs = GameState::default();
        assert_eq!(pet_feed_limit(&gs), 3);
        assert_eq!(wheel_spin_limit(&gs), 20);
        gs.specials
            .events
            .active
            .insert(Event::AssemblyOfAwesomeAnimals);
        gs.specials.events.active.insert(Event::LuckyDay);
        assert_eq!(pet_feed_limit(&gs), 9);
        assert_eq!(wheel_spin_limit(&gs), 40);
        gs.specials.events.ends =
            Some(chrono::Local::now() - chrono::Duration::seconds(1));
        assert_eq!(pet_feed_limit(&gs), 3);
        assert_eq!(wheel_spin_limit(&gs), 20);
    }
}


#[cfg(test)]
mod thirst_tests {
    use super::*;

    /// The two numbers the Czech strategy wiki gives, which is the
    /// reference this app's game logic is built from: "100 bodu/den ...
    /// Pivo = +20 bodu, max. 10 piv/den (celkem 300), od carodejnice
    /// (lvl 66) s ocarovanim opasku 11 piv (320)"
    #[test]
    fn the_ceiling_matches_the_wiki_for_ten_beers_and_for_eleven() {
        assert_eq!(thirst_ceiling_min(10), 300);
        assert_eq!(thirst_ceiling_min(11), 320);
    }

    #[test]
    fn a_character_who_cannot_drink_still_has_a_days_thirst() {
        assert_eq!(thirst_ceiling_min(0), THIRST_BASE_MIN);
    }

    #[test]
    fn the_plate_reads_the_way_the_guild_leader_asked_for_it() {
        assert_eq!(thirst_line(114, 10), "114/300");
        assert_eq!(thirst_line(0, 11), "0/320");
    }

    #[test]
    fn a_bar_never_reads_past_its_own_end() {
        // The Adventuromat hands back thirst saved over midnight, so the
        // day can open above what it would otherwise hold
        assert_eq!(thirst_line(340, 11), "340/340");
    }
}

// ------------------------------------------- Arena Manager: runes and resets

/// Runes pay in completed blocks of twenty and in nothing else. Nineteen
/// runes are worth exactly as much as none
pub const RUNES_PER_BLOCK: u128 = 20;
/// What one rune inside a completed block adds to earnings
pub const EARNINGS_PER_RUNE_PCT: u128 = 5;

/// A rune figure from the game as a number the arithmetic below can use.
///
/// Runes grow by orders of magnitude with every reset and an old account
/// is past what 64 bits hold. Reading such a pile as zero - which is what
/// `to_u64().unwrap_or(0)` did - made the 400 % threshold come out at 80
/// runes, so a reset worth nothing passed it. Anything past 128 bits is
/// held at the top, where no offer reaches the threshold
#[must_use]
pub fn runes_of(n: &num_bigint::BigInt) -> u128 {
    use num_traits::{Signed, ToPrimitive};
    if n.is_negative() {
        return 0;
    }
    n.to_u128().unwrap_or(u128::MAX)
}

/// How many completed blocks a pile of runes is worth
#[must_use]
pub fn rune_blocks(runes: u128) -> u128 {
    runes / RUNES_PER_BLOCK
}

/// What the runes multiply earnings by. One block is twenty runes at five
/// per cent each, so every block is another whole multiple of the base
#[must_use]
pub fn rune_multiplier(runes: u128) -> u128 {
    (rune_blocks(runes).saturating_mul(RUNES_PER_BLOCK * EARNINGS_PER_RUNE_PCT)
        / 100)
        .saturating_add(1)
}

/// What a reset would actually add, as a percentage of the earnings
/// multiplier already owned.
///
/// This is the number the decision turns on, and it is not the number the
/// game shows. The game offers a count of runes - "SACRIFICE 746" - which
/// says nothing on its own: 746 runes is everything to an account with none
/// and a rounding error to one with a million. A reset costs every building
/// level on the board, so what it has to be weighed against is what the
/// runes already do
#[must_use]
pub fn sacrifice_gain_pct(owned: u128, offer: u128) -> f64 {
    let before = rune_multiplier(owned);
    let after = rune_multiplier(owned.saturating_add(offer));
    #[allow(clippy::cast_precision_loss)]
    {
        (after - before) as f64 * 100.0 / before as f64
    }
}

/// The settings the player picks from: what a reset has to hand over, as
/// a percentage of the runes already owned. "When you can see I have
/// thirty million runes, the sacrifice has to answer to that" - so the
/// base is the runes on the screen and nothing the app has to remember.
/// 25 on thirty million is seven and a half million; 400 is four times
/// the pile
pub const SACRIFICE_STEPS: [u32; 10] =
    [25, 50, 75, 100, 125, 150, 175, 200, 300, 400];

/// The nearest of the steps to whatever is stored. The setting has
/// changed meaning twice and old files hold numbers the picker does not
/// offer; a picker showing a value it cannot be set back to is a control
/// nobody can trust
#[must_use]
pub fn snap_sacrifice_step(pct: u32) -> u32 {
    SACRIFICE_STEPS
        .into_iter()
        .min_by_key(|s| s.abs_diff(pct))
        .unwrap_or(50)
}

/// The exact minimum gain selected by the user. UI and automation share
/// this value; reconnects and resets must not change it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SacrificeBar {
    pub pct: f64,
}

#[must_use]
pub fn sacrifice_bar(pct_setting: u32) -> SacrificeBar {
    SacrificeBar { pct: f64::from(snap_sacrifice_step(pct_setting)) }
}

/// How many runes an offer needs to be worth `gain_pct` on top of what is
/// already owned - the exact inverse of `sacrifice_gain_pct`, counted from
/// where the pile actually stands: thirty runes owned is one block and ten
/// into the next, so ten more finish it
#[must_use]
pub fn runes_for_gain(owned: u128, gain_pct: f64) -> u128 {
    #[allow(clippy::cast_precision_loss)]
    let before = rune_multiplier(owned) as f64;
    let wanted = (before * gain_pct / 100.0).ceil().max(1.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let wanted = wanted as u128;
    let target = rune_blocks(owned).saturating_add(wanted);
    target
        .saturating_mul(RUNES_PER_BLOCK)
        .saturating_sub(owned)
        .max(1)
}

/// The figure this reset is held to, in runes - the one number the screen
/// and the automation both read
#[must_use]
pub fn sacrifice_bar_runes(owned: u128, bar: SacrificeBar) -> u128 {
    runes_for_gain(owned, bar.pct)
}

/// Whether to reset now: the offer completes at least one new block of
/// twenty (or it buys literally nothing), and it reaches the bar
#[must_use]
pub fn should_sacrifice(owned: u128, offer: u128, bar: SacrificeBar) -> bool {
    if rune_blocks(owned.saturating_add(offer)) == rune_blocks(owned) {
        return false;
    }
    offer >= sacrifice_bar_runes(owned, bar)
}

#[cfg(test)]
mod rune_tests {
    use super::*;

    fn bar(pct: f64) -> SacrificeBar {
        SacrificeBar { pct }
    }

    /// Straight from the Czech strategy wiki: "Kazda runa = +5 % k vydelku,
    /// ale bonus se pocita jen po 20 runach (19 run = 0 %)"
    #[test]
    fn nineteen_runes_are_worth_as_much_as_none() {
        assert_eq!(rune_blocks(19), 0);
        assert_eq!(rune_multiplier(19), rune_multiplier(0));
        assert_eq!(rune_blocks(20), 1);
        assert_eq!(rune_multiplier(20), 2);
        assert_eq!(rune_multiplier(40), 3);
    }

    /// The screenshot that started this: 1.219 million runes owned, the
    /// game offering 746 for a reset that costs levels 351, 401 and 311
    #[test]
    fn the_offer_that_looked_generous_was_six_hundredths_of_a_per_cent() {
        let gain = sacrifice_gain_pct(1_219_000, 746);
        assert!(
            (0.05..0.07).contains(&gain),
            "expected about 0.06 %, got {gain}"
        );
    }

    #[test]
    fn an_offer_that_completes_no_new_block_is_never_taken() {
        // Thirty runes owned is one block; nine more is still one block, so
        // the reset would buy nothing at all
        assert!(!should_sacrifice(30, 9, bar(1.0)));
        // The tenth completes the second block
        assert!(should_sacrifice(30, 10, bar(25.0)));
    }

    /// "kdyz vidis ze mam 30 MILIONU RUN tak to ma tomu ten sacrifice
    /// odpovidat" - the bar is a share of the runes on the screen
    #[test]
    fn the_bar_is_a_share_of_the_runes_owned() {
        let owned = 29_558_727u128;
        let quarter = sacrifice_bar_runes(owned, bar(25.0));
        let whole = sacrifice_bar_runes(owned, bar(100.0));
        let four = sacrifice_bar_runes(owned, bar(400.0));
        // Within a few blocks of the plain arithmetic - the multiplier
        // has a base of one on top of the blocks, which at 400 % is four
        // blocks of difference and at thirty million is nothing
        let near = |a: u128, b: u128| a.abs_diff(b) <= RUNES_PER_BLOCK * 6;
        assert!(near(quarter, owned / 4), "25 % gave {quarter}");
        assert!(near(whole, owned), "100 % gave {whole}");
        assert!(near(four, owned * 4), "400 % gave {four}");
        // And what it is held to is worth exactly that
        let g = sacrifice_gain_pct(owned, quarter);
        assert!((24.99..25.01).contains(&g), "25 % bar clears at {g} %");
        // The screenshot's offer against the smallest setting: not yet
        assert!(!should_sacrifice(owned, 234_752, bar(25.0)));
    }

    /// Nothing to remember: a new account and an old one are judged by
    /// the same rule, off what they have
    #[test]
    fn it_scales_with_the_account_and_needs_no_history() {
        let mut last = 0;
        for owned in [0u128, 40, 1_000, 87_711, 1_228_999, 29_558_727] {
            let b = sacrifice_bar_runes(owned, bar(50.0));
            assert!(b >= last, "the bar fell as the account grew");
            last = b;
        }
        // A new account: the first block is the whole multiplier
        assert!(should_sacrifice(0, 20, bar(100.0)));
    }

    #[test]
    fn every_step_is_exact_and_does_not_trigger_early() {
        for step in SACRIFICE_STEPS {
            let b = sacrifice_bar(step);
            assert_eq!(b.pct, f64::from(step));
            for owned in [0, 30, 1_219_000, 29_558_727] {
                let target = sacrifice_bar_runes(owned, b);
                assert!(!should_sacrifice(owned, target - 1, b));
                assert!(should_sacrifice(owned, target, b));
                assert!(sacrifice_gain_pct(owned, target) >= f64::from(step));
            }
        }
        assert_eq!(sacrifice_bar(10).pct, 25.0);
    }

    /// 28 October, 89 resets in: "400 %" showed a threshold of 80 runes.
    /// The pile was past 64 bits and read as zero - a reset handing over a
    /// sliver of a per cent passed the bar
    #[test]
    fn a_pile_past_64_bits_is_not_read_as_zero() {
        use num_bigint::BigInt;
        let owned = BigInt::from(u64::MAX) * 40;
        let owned = runes_of(&owned);
        assert!(owned > u128::from(u64::MAX));
        let bar = sacrifice_bar(400);
        assert!(sacrifice_bar_runes(owned, bar) > owned * 3);
        assert!(!should_sacrifice(owned, 1_006_053_139_415_040, bar));
        // Beyond 128 bits: held at the top, never at zero
        let huge = BigInt::from(u128::MAX) * 1_000;
        assert_eq!(runes_of(&huge), u128::MAX);
        assert!(!should_sacrifice(runes_of(&huge), u128::from(u64::MAX), bar));
        assert_eq!(runes_of(&BigInt::from(-5)), 0);
    }

    #[test]
    fn repeated_checks_never_redraw_the_sacrifice_threshold() {
        let target = sacrifice_bar_runes(29_558_727, sacrifice_bar(100));
        for _ in 0..300 {
            assert_eq!(sacrifice_bar_runes(29_558_727, sacrifice_bar(100)), target);
        }
    }

}

// ------------------------------------------- Arena Manager: money in hand

/// The cheapest single level on the board: the building, what one level
/// costs and the level it stands at now.
///
/// The game's money figure is a snapshot - the server works it out only
/// when it answers an Arena Manager purchase, and every other answer, the
/// login included, hands back the same old number. When that number
/// covers nothing, buying this one level is how the app asks the game what
/// is really in hand: the answer carries the money worked out to the
/// second, and the round goes on from there. If the game says no, there
/// really is not enough
#[must_use]
pub fn idle_cheapest_level(
    idle: &sf_api::gamestate::idle::IdleGame,
) -> Option<(sf_api::gamestate::idle::IdleBuildingType, f64, u32)> {
    use num_traits::ToPrimitive;
    idle.buildings
        .iter()
        .filter_map(|(typ, b)| {
            let cost = b.upgrade_cost.to_f64()?;
            (cost.is_finite() && cost > 0.0).then_some((typ, cost, b.level))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
}

/// One line of what the game says the Arena Manager holds: money, resets
/// and every built building with its level, payout and where its cycle
/// stands. Goes into the log when the game refuses a purchase
#[must_use]
pub fn idle_snapshot_line(
    idle: &sf_api::gamestate::idle::IdleGame,
    now: chrono::DateTime<chrono::Local>,
) -> String {
    let mut out = format!(
        "money {} (resets {}, runes {})",
        idle.current_money, idle.resets, idle.current_runes
    );
    for (typ, b) in idle.buildings.iter().filter(|(_, b)| b.level > 0) {
        let cycle = match (b.cycle_start, b.cycle_end) {
            (Some(s), Some(e)) => format!(
                "cycle {} s, ends in {} s",
                (e - s).num_seconds(),
                (e - now).num_seconds()
            ),
            _ => "no cycle".to_string(),
        };
        out.push_str(&format!(
            "; {typ:?} L{} pays {} ({cycle}), next level {}",
            b.level, b.earning, b.upgrade_cost
        ));
    }
    out
}

#[cfg(test)]
mod idle_money_tests {
    use super::*;
    use num_bigint::BigInt;
    use sf_api::gamestate::idle::{IdleBuildingType as B, IdleGame};

    /// 28 October after the 89th reset: Seats at level 1 for 6, nothing
    /// else built, the purse at 0 - the check is one Seat
    #[test]
    fn the_purse_check_is_the_cheapest_single_level() {
        let mut g = IdleGame::default();
        g.buildings[B::Seat].level = 1;
        g.buildings[B::Seat].upgrade_cost = BigInt::from(6);
        g.buildings[B::PopcornStand].upgrade_cost = BigInt::from(100);
        g.buildings[B::Toilet].upgrade_cost = BigInt::from(5_000_000_000_000u64);
        assert_eq!(idle_cheapest_level(&g), Some((B::Seat, 6.0, 1)));
    }

    #[test]
    fn no_price_on_the_board_means_no_check() {
        assert_eq!(idle_cheapest_level(&IdleGame::default()), None);
    }
}

// ------------------------------------------------- Fortress: the raid cycle

/// The barracks, as three numbers: standing, in training, and the most
/// the two together may be
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Barracks {
    pub soldiers: u32,
    pub in_training: u32,
    pub limit: u32,
}

impl Barracks {
    #[must_use]
    pub fn full(self) -> bool {
        self.in_training == 0 && self.soldiers >= self.limit && self.limit > 0
    }

    /// How many are missing, counting the ones already queued as present
    #[must_use]
    pub fn missing(self) -> u32 {
        self.limit
            .saturating_sub(self.soldiers)
            .saturating_sub(self.in_training)
    }
}

/// What the cycle does between two raids.
///
/// The fortress used to be raided on a clock - "about every three
/// minutes" - which is the one rhythm nobody real has. A person raids
/// when they sit down and the barracks are full, uses the soldiers up one
/// opponent at a time, queues a new batch and leaves. The training time
/// is what sets the pace, and the training time is the game's
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RaidStep {
    /// Full and nobody training - the raid may start once the notice
    /// delay has run
    Full,
    /// Queue this many soldiers
    Recruit(u32),
    /// Training, or nothing affordable - nothing to do but wait
    Wait,
}

/// The step for one look at the barracks. `affordable` is how many
/// soldiers the stock pays for right now
#[must_use]
pub fn raid_step(b: Barracks, affordable: u32) -> RaidStep {
    if b.full() {
        return RaidStep::Full;
    }
    let missing = b.missing();
    if missing == 0 || affordable == 0 {
        return RaidStep::Wait;
    }
    RaidStep::Recruit(missing.min(affordable))
}

/// Whether an opponent is worth the soldiers the game says it takes.
///
/// "It always pays - but do not throw five soldiers at somebody with a
/// hundred stone." The loot the game shows for the raid has to cover
/// what the soldiers sent cost to train, wood and stone together
#[must_use]
pub fn raid_pays(
    loot_wood: u64,
    loot_stone: u64,
    soldiers: u32,
    wood_each: u64,
    stone_each: u64,
) -> bool {
    let cost = u64::from(soldiers).saturating_mul(wood_each + stone_each);
    loot_wood.saturating_add(loot_stone) >= cost
}

/// The least a raid waits after the barracks fill, whatever the setting.
/// A person does not click the second the last soldier is trained
pub const NOTICE_MIN_SECS: i64 = 180;

/// How to get past an opponent that is not worth it or too strong.
///
/// In his order: a free new opponent when the game offers one; a paid
/// one, once, when the price is small change against the purse; and
/// failing both, one soldier thrown at them - the fight is lost, it
/// costs one soldier, and the game puts somebody else in front of you
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skip {
    Free,
    Paid,
    OneSoldier,
    GiveUp,
}

/// A paid new opponent is taken while the price is at most this share of
/// the silver in the purse - "50k when you have plenty of gold"
pub const PAID_SKIP_MAX_SHARE: u64 = 50;

#[must_use]
pub fn skip_choice(
    free: bool,
    price: u64,
    silver: u64,
    paid_already: bool,
    soldiers: u32,
) -> Skip {
    if free {
        return Skip::Free;
    }
    if !paid_already
        && price > 0
        && price.saturating_mul(PAID_SKIP_MAX_SHARE) <= silver
    {
        return Skip::Paid;
    }
    if soldiers >= 1 {
        return Skip::OneSoldier;
    }
    Skip::GiveUp
}

#[cfg(test)]
mod raid_tests {
    use super::*;

    fn b(soldiers: u32, in_training: u32, limit: u32) -> Barracks {
        Barracks {
            soldiers,
            in_training,
            limit,
        }
    }

    #[test]
    fn full_means_full_and_nobody_training() {
        assert_eq!(raid_step(b(30, 0, 30), 99), RaidStep::Full);
        // Thirty standing and one in the queue is not full yet - the
        // limit counts both, so this one is over it and still training
        assert_eq!(raid_step(b(29, 1, 30), 99), RaidStep::Wait);
        assert_eq!(raid_step(b(0, 30, 30), 99), RaidStep::Wait);
    }

    #[test]
    fn empty_barracks_are_refilled_in_one_order() {
        // Soldiers ran out: queue the lot, as far as the stock goes
        assert_eq!(raid_step(b(0, 0, 30), 99), RaidStep::Recruit(30));
        assert_eq!(raid_step(b(0, 0, 30), 12), RaidStep::Recruit(12));
        // Half queued already: only the rest
        assert_eq!(raid_step(b(0, 18, 30), 99), RaidStep::Recruit(12));
        // No stock: wait for the stores, not for ever
        assert_eq!(raid_step(b(4, 0, 30), 0), RaidStep::Wait);
    }

    #[test]
    fn no_barracks_is_never_full() {
        assert_eq!(raid_step(b(0, 0, 0), 99), RaidStep::Wait);
    }

    /// "kdyz je skip zadarmo skipnes.. kdyz stoji 50k a mas hodne goldu
    /// jeste jednou klidne skipnes... a kdyz to nevyjde posilas po jednom
    /// vojaku pro free skip"
    #[test]
    fn a_bad_opponent_is_skipped_free_then_paid_once_then_with_a_soldier() {
        // Free wins over everything
        assert_eq!(skip_choice(true, 50_000, 0, true, 0), Skip::Free);
        // 50k against 10 million: small change, pay
        assert_eq!(skip_choice(false, 50_000, 10_000_000, false, 5), Skip::Paid);
        // 50k against 200k: that is not "plenty of gold"
        assert_eq!(skip_choice(false, 50_000, 200_000, false, 5), Skip::OneSoldier);
        // Already paid once this raid: the next one costs a soldier
        assert_eq!(skip_choice(false, 50_000, 10_000_000, true, 5), Skip::OneSoldier);
        // Nothing left to send
        assert_eq!(skip_choice(false, 50_000, 200_000, true, 0), Skip::GiveUp);
    }

    /// "abys neplacal 5 vojaku na capka co ma 100 kamene"
    #[test]
    fn five_soldiers_are_not_thrown_at_a_hundred_stone() {
        // A soldier costs 150 wood and 100 stone here
        assert!(!raid_pays(0, 100, 5, 150, 100));
        // The same five against a fortress worth raiding
        assert!(raid_pays(900, 800, 5, 150, 100));
        // One soldier against anything at all that covers him
        assert!(raid_pays(200, 60, 1, 150, 100));
        // A soldier that costs nothing is always worth sending
        assert!(raid_pays(1, 0, 40, 0, 0));
    }
}
