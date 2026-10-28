//! Raising the worn gear at the blacksmith.
//!
//! Every piece can be raised twenty times for metal and arcane splinters,
//! 3 % of its highest attribute a level. The materials are the scarce
//! thing, so the rule is: never raise a piece that is about to be replaced.
//! A slot is only raised when neither shop nor the bag holds anything
//! better for its wearer - the shop step runs first and buys what it can,
//! and what it can not afford yet is left alone here too. The character
//! comes before Bert, Mark and Kunigunde, and among the rest the level
//! that buys the most for its materials is bought first, which spreads the
//! levels evenly over the gear instead of pouring them into one piece.
//!
//! The decisions live here; the request goes out from `message.rs`.

use sf_api::gamestate::{
    GameState,
    character::Class,
    dungeons::CompanionClass,
    items::{EquipmentSlot, Item, ItemCommandIdent},
};

use crate::shopping::item_score;

/// The blacksmith stops at twenty
pub const MAX_LEVEL: u8 = 20;

/// Arcane is the scarce material: it takes this many metal to weigh as
/// much as one splinter when levels are ranked by what they cost
const ARCANE_WEIGHT: f64 = 4.0;

/// The most of the stock one level may take - the rest stays for the
/// next cheap levels and for what the day brings in
pub const STOCK_SHARE_MAX: f64 = 0.4;

/// One level to buy
#[derive(Debug, Clone)]
pub struct Plan {
    /// `None` is the character
    pub wearer: Option<CompanionClass>,
    pub slot: EquipmentSlot,
    pub item_ident: ItemCommandIdent,
    /// The level the piece has now
    pub from: u8,
    /// For the log
    pub what: String,
}

/// "you", "Bert", ...
#[must_use]
pub fn wearer_name(wearer: Option<CompanionClass>) -> &'static str {
    match wearer {
        None => "you",
        Some(c) => crate::companions::companion_name(c),
    }
}

/// The class a wearer's gear is judged by
fn wearer_class(gs: &GameState, wearer: Option<CompanionClass>) -> Class {
    match wearer {
        None => gs.character.class,
        Some(c) => Class::from(c),
    }
}

/// Can this wearer put the piece on at all?
fn can_wear(gs: &GameState, wearer: Option<CompanionClass>, item: &Item) -> bool {
    match wearer {
        None => item.can_be_equipped_by(gs.character.class),
        Some(c) => gs.dungeons.can_companion_equip(c, item),
    }
}

/// Is there a better piece for this wearer's slot within reach - in
/// either shop, whatever it costs, or already in the bag? Then the worn
/// one is on its way out and not worth a single level. `min_gain_pct` is
/// the shop's own bar, so "better" here means the same as it does there
#[must_use]
pub fn better_available(
    gs: &GameState,
    wearer: Option<CompanionClass>,
    slot: EquipmentSlot,
    worn: &Item,
    min_gain_pct: f64,
) -> bool {
    let class = wearer_class(gs, wearer);
    let have = item_score(worn, class);
    let beats = |item: &Item| {
        item.typ.equipment_slot() == Some(slot)
            && can_wear(gs, wearer, item)
            && have > 0.0
            && (item_score(item, class) / have - 1.0) * 100.0 >= min_gain_pct
    };
    // Only a shop piece the purse covers right now counts (5.3). "Whatever
    // it costs" held the levels back on every slot with some dear piece
    // a little better in the shop - and that is nearly every slot, so
    // the metal piled up and nothing was raised
    let in_shops = gs.shops.values().any(|shop| {
        shop.iter().any(|(_, item)| {
            beats(item)
                && item.mushroom_price == 0
                && u64::from(item.price) <= gs.character.silver
        })
    });
    let in_bag = gs
        .character
        .inventory
        .iter()
        .any(|(_, item)| item.is_some_and(beats));
    in_shops || in_bag
}

/// The average item level of what a wearer has on - the level their gear
/// is at as a whole. `None` with nothing worn
#[must_use]
pub fn average_item_level(
    equipment: &sf_api::gamestate::items::Equipment,
) -> Option<f64> {
    let levels: Vec<f64> = equipment
        .0
        .iter()
        .filter_map(|(_, item)| item.as_ref().map(|i| f64::from(i.item_quality)))
        .collect();
    if levels.is_empty() {
        return None;
    }
    Some(levels.iter().sum::<f64>() / levels.len() as f64)
}

/// How far under the wearer's average item level a piece may sit and still
/// be worth a level, as a share of that average. A piece far below the
/// rest is on its way out with the next shop - levels on it are
/// materials thrown after it. The caller narrows this to nothing for a
/// character that levels fast, where the shop moves on every few days
pub const BELOW_AVERAGE_TOLERANCE: f64 = 0.15;

/// The next level worth buying, or `None` when nothing is: no blacksmith
/// yet, no materials for any level, every piece at twenty, or every piece
/// that is not at twenty about to be replaced. Protected pieces (a gem,
/// an enchantment) are raised like any other - they are staying.
/// `tolerance` is how far under the wearer's average item level a piece
/// may be (see `BELOW_AVERAGE_TOLERANCE`)
#[cfg(test)]
#[must_use]
pub fn next_level(gs: &GameState, min_gain_pct: f64, tolerance: f64) -> Option<Plan> {
    next_level_explained(gs, min_gain_pct, tolerance).0
}

/// What the weighing went through - the bot's thinking, for the feed
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Weighing {
    /// Pieces looked at (worn, under +20)
    pub considered: usize,
    /// Left alone: under the wearer's average item level by more than the
    /// tolerance - the next to be replaced
    pub below_floor: usize,
    /// Left alone: the next level costs more than there is
    pub no_materials: usize,
    /// Left alone: the level would take over `STOCK_SHARE_MAX` of the stock
    pub over_share: usize,
    /// Left alone: something better is in a shop or the bag
    pub better_in_sight: usize,
}

/// `next_level` with the count of what was weighed and why each piece
/// was left alone
#[must_use]
pub fn next_level_explained(
    gs: &GameState,
    min_gain_pct: f64,
    tolerance: f64,
) -> (Option<Plan>, Weighing) {
    let mut weighing = Weighing::default();
    let Some(smith) = gs.blacksmith.as_ref() else {
        return (None, weighing);
    };
    let mut best: Option<(f64, Plan)> = None;
    let mut consider = |wearer: Option<CompanionClass>,
                        slot: EquipmentSlot,
                        item: &Item,
                        average: Option<f64>,
                        w: &mut Weighing| {
        if item.upgrade_count >= MAX_LEVEL {
            return;
        }
        w.considered += 1;
        // A piece well under the rest of the set is the next to go - not
        // worth a level. Rings, amulets and talismans count like the rest
        if let Some(avg) = average
            && f64::from(item.item_quality) < avg * (1.0 - tolerance)
        {
            w.below_floor += 1;
            return;
        }
        let Some(cost) = item.upgrade_costs() else {
            return;
        };
        if cost.metal > smith.metal || cost.arcane > smith.arcane {
            w.no_materials += 1;
            return;
        }
        // The stock is never emptied on one level: a level may take at
        // most this share of what is there, so the materials keep pace
        // with what comes in and a single dear level never swallows a
        // week's arcane
        if cost.metal as f64 > smith.metal as f64 * STOCK_SHARE_MAX
            || cost.arcane as f64 > smith.arcane as f64 * STOCK_SHARE_MAX
        {
            w.over_share += 1;
            return;
        }
        // A piece the shop step would not touch is still raised - the
        // protection says it stays, and what stays is worth raising. The
        // one thing that is not: a piece with something better in sight
        if better_available(gs, wearer, slot, item, min_gain_pct) {
            w.better_in_sight += 1;
            return;
        }
        let class = wearer_class(gs, wearer);
        // What the level buys: 3 % of the piece, against what it costs.
        // The character's gear fights everywhere, the companions' only in
        // the shadow dungeons - so a level on the character weighs double
        let gain = item_score(item, class) * crate::shopping::UPGRADE_STEP
            * if wearer.is_none() { 2.0 } else { 1.0 };
        let price = cost.metal as f64 + cost.arcane as f64 * ARCANE_WEIGHT;
        let value = gain / price.max(1.0);
        if best.as_ref().is_none_or(|(v, _)| value > *v) {
            best = Some((
                value,
                Plan {
                    wearer,
                    slot,
                    item_ident: item.command_ident(),
                    from: item.upgrade_count,
                    what: format!(
                        "Raised the {slot:?} of {} to +{} at the blacksmith \
                         ({} metal, {} arcane)",
                        wearer_name(wearer),
                        item.upgrade_count + 1,
                        cost.metal,
                        cost.arcane
                    ),
                },
            ));
        }
    };
    let own_avg = average_item_level(&gs.character.equipment);
    for (slot, item) in &gs.character.equipment.0 {
        if let Some(item) = item {
            consider(None, slot, item, own_avg, &mut weighing);
        }
    }
    if let Some(companions) = &gs.dungeons.companions {
        for (typ, companion) in companions {
            let avg = average_item_level(&companion.equipment);
            for (slot, item) in &companion.equipment.0 {
                if let Some(item) = item {
                    consider(Some(typ), slot, item, avg, &mut weighing);
                }
            }
        }
    }
    (best.map(|(_, p)| p), weighing)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sf_api::{
        command::{AttributeType, ShopType},
        gamestate::{items::ItemType, unlockables::Blacksmith},
    };

    fn hat(class: Class, strength: u32) -> Item {
        let mut item = sf_api::gamestate::Shop::default().items[0].clone();
        item.typ = ItemType::Hat;
        item.class = Some(class);
        item.price = 100;
        item.mushroom_price = 0;
        item.attributes[AttributeType::Strength] = strength;
        item
    }

    fn state() -> GameState {
        let mut gs = GameState::default();
        gs.character.class = Class::Warrior;
        gs.blacksmith = Some(Blacksmith {
            metal: 1_000_000,
            arcane: 1_000_000,
            ..Default::default()
        });
        gs.character.equipment.0[EquipmentSlot::Hat] =
            Some(hat(Class::Warrior, 100));
        gs
    }

    #[test]
    fn a_piece_with_nothing_better_in_sight_is_raised() {
        let gs = state();
        let plan = next_level(&gs, 5.0, BELOW_AVERAGE_TOLERANCE).unwrap();
        assert_eq!(plan.slot, EquipmentSlot::Hat);
        assert!(plan.wearer.is_none());
        assert_eq!(plan.from, 0);
        assert!(plan.what.contains("+1"));
    }

    #[test]
    fn a_better_piece_in_the_shop_or_the_bag_holds_the_level_back() {
        let mut gs = state();
        let mut dear = hat(Class::Warrior, 150);
        dear.price = 1_000;
        gs.shops[ShopType::Weapon].items[0] = dear;
        // 5.3: a piece the purse does not cover does not hold the level
        gs.character.silver = 999;
        assert!(next_level(&gs, 5.0, BELOW_AVERAGE_TOLERANCE).is_some());
        gs.character.silver = 1_000;
        assert!(next_level(&gs, 5.0, BELOW_AVERAGE_TOLERANCE).is_none());
        // A piece for the wrong class is nobody's upgrade
        gs.shops[ShopType::Weapon].items[0] = hat(Class::Mage, 150);
        assert!(next_level(&gs, 5.0, BELOW_AVERAGE_TOLERANCE).is_some());
        // In the bag
        gs.character.inventory.backpack = vec![Some(hat(Class::Warrior, 150))];
        assert!(next_level(&gs, 5.0, BELOW_AVERAGE_TOLERANCE).is_none());
        // Only just better - under the shop's own bar - does not count
        gs.character.inventory.backpack = vec![Some(hat(Class::Warrior, 103))];
        assert!(next_level(&gs, 5.0, BELOW_AVERAGE_TOLERANCE).is_some());
    }

    #[test]
    fn no_materials_no_level_and_twenty_is_the_end() {
        let mut gs = state();
        gs.blacksmith.as_mut().unwrap().metal = 0;
        assert!(next_level(&gs, 5.0, BELOW_AVERAGE_TOLERANCE).is_none());
        // Enough for the level, but it would take more than the share of
        // the stock a single level may: the level waits for more to come in
        let mut gs = state();
        let cost = gs.character.equipment.0[EquipmentSlot::Hat]
            .as_ref()
            .unwrap()
            .upgrade_costs()
            .unwrap();
        gs.blacksmith.as_mut().unwrap().metal = cost.metal + 1;
        assert!(next_level(&gs, 5.0, BELOW_AVERAGE_TOLERANCE).is_none());
        gs.blacksmith.as_mut().unwrap().metal = cost.metal * 3;
        assert!(next_level(&gs, 5.0, BELOW_AVERAGE_TOLERANCE).is_some());
        let mut gs = state();
        gs.character.equipment.0[EquipmentSlot::Hat]
            .as_mut()
            .unwrap()
            .upgrade_count = MAX_LEVEL;
        assert!(next_level(&gs, 5.0, BELOW_AVERAGE_TOLERANCE).is_none());
        // No blacksmith at all
        let mut gs = state();
        gs.blacksmith = None;
        assert!(next_level(&gs, 5.0, BELOW_AVERAGE_TOLERANCE).is_none());
    }

    /// A piece far under the wearer's average item level is the next to
    /// be replaced - no level for it; a fast-levelling character (no
    /// tolerance) only raises pieces at or above the average
    #[test]
    fn a_piece_well_under_the_average_item_level_is_left_alone() {
        let mut gs = state();
        // Hat at item level 100 next to a belt at 150: average 125
        gs.character.equipment.0[EquipmentSlot::Hat]
            .as_mut()
            .unwrap()
            .item_quality = 100;
        let mut belt = hat(Class::Warrior, 100);
        belt.typ = ItemType::Belt;
        belt.item_quality = 150;
        belt.upgrade_count = MAX_LEVEL;
        gs.character.equipment.0[EquipmentSlot::Belt] = Some(belt);
        assert_eq!(average_item_level(&gs.character.equipment), Some(125.0));
        // 100 is 20 % under 125: out at the normal tolerance of 15 %
        assert!(next_level(&gs, 5.0, BELOW_AVERAGE_TOLERANCE).is_none());
        // At item level 135 (average 142.5, 5 % under) it is raised -
        // unless the character levels fast and only the average or better
        // counts
        gs.character.equipment.0[EquipmentSlot::Hat]
            .as_mut()
            .unwrap()
            .item_quality = 135;
        assert!(next_level(&gs, 5.0, BELOW_AVERAGE_TOLERANCE).is_some());
        assert!(next_level(&gs, 5.0, 0.0).is_none());
    }

    #[test]
    fn the_character_comes_before_the_companions() {
        let mut gs = state();
        gs.dungeons.companions = Some(Default::default());
        let companions = gs.dungeons.companions.as_mut().unwrap();
        companions[CompanionClass::Warrior].equipment.0[EquipmentSlot::Hat] =
            Some(hat(Class::Warrior, 100));
        let plan = next_level(&gs, 5.0, BELOW_AVERAGE_TOLERANCE).unwrap();
        assert!(plan.wearer.is_none());
        // With the character's hat done, Bert's turn
        gs.character.equipment.0[EquipmentSlot::Hat]
            .as_mut()
            .unwrap()
            .upgrade_count = MAX_LEVEL;
        let plan = next_level(&gs, 5.0, BELOW_AVERAGE_TOLERANCE).unwrap();
        assert_eq!(plan.wearer, Some(CompanionClass::Warrior));
        assert!(plan.what.contains("Bert"));
    }
}
