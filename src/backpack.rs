//! Keeping room in the backpack.
//!
//! A full backpack stops dungeon fights: the game will not hand out loot it
//! has nowhere to put. Mining gems and fighting dungeons at the same time
//! fills it within a couple of hours, and from then on the free dungeon
//! fight expires every hour without anything happening - which is exactly
//! what it looks like from the outside when "auto dungeon stopped working".
//!
//! So one slot is kept free. What goes is the weakest piece that is not
//! protected and not better than what the character (or a companion) is
//! already wearing - the same protections the shopping page uses, so
//! whatever is safe from the shop is safe here too.

use sf_api::gamestate::{
    GameState,
    items::{BagPosition, Item, ItemType},
};

use crate::shopping::{Protection, item_score, protected_reason};

/// Slots in use and slots there are, counting the extended inventory
#[must_use]
pub fn space(gs: &GameState) -> (usize, usize) {
    let bp = &gs.character.inventory.backpack;
    (bp.iter().flatten().count(), bp.len())
}

/// How many slots are still empty
#[must_use]
pub fn free_slots(gs: &GameState) -> usize {
    let (used, total) = space(gs);
    total.saturating_sub(used)
}

/// Things that are never sold to make room, whatever the settings say -
/// they are not gear and the game has other uses for them
fn is_gear(item: &Item) -> bool {
    is_gear_type(item.typ)
}

/// Same question, asked about the type alone - which is all it depends on
fn is_gear_type(typ: ItemType) -> bool {
    matches!(
        typ,
        ItemType::Hat
            | ItemType::BreastPlate
            | ItemType::Gloves
            | ItemType::FootWear
            | ItemType::Weapon { .. }
            | ItemType::Amulet
            | ItemType::Belt
            | ItemType::Ring
            | ItemType::Talisman
            | ItemType::Shield { .. }
    )
}

/// What happens to the piece that makes room
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposal {
    /// Sold to the shop for gold
    Sell,
    /// Taken apart at the blacksmith for metal and arcane splinters - what
    /// an epic or a raised piece is worth far more as than as gold
    Dismantle,
}

/// The piece that makes room, how it goes, and a line for the log
#[derive(Debug, Clone)]
pub struct Sale {
    pub pos: BagPosition,
    pub what: String,
    pub disposal: Disposal,
}

/// One piece to make room with. `None` means there is either enough room
/// or nothing that may go.
///
/// Two tiers. The normal one sells the weakest unprotected piece nobody in
/// the party wants. That alone deadlocked: a bag full of epics from the
/// dungeons has no unprotected piece in it, so nothing was ever sold, and
/// from then on the dungeons, the gem mine, the free spin and the daily
/// reward were all refused with "backpack full" - for days.
///
/// So when the bag is *completely* full and the normal tier finds nothing,
/// the emergency tier may take the weakest plain epic or raised piece: never
/// a legendary, never a piece with a gem in it, never an enchanted one, and
/// never one somebody in the party would wear. It is dismantled at the
/// blacksmith when `can_dismantle` (the materials are what such a piece is
/// worth), otherwise sold
#[must_use]
pub fn next_sale(
    gs: &GameState,
    protection: &Protection,
    keep_free: usize,
    can_dismantle: bool,
) -> Option<Sale> {
    if free_slots(gs) >= keep_free {
        return None;
    }
    if let Some((pos, what)) = weakest(gs, protection) {
        // An epic or a raised piece is worth its materials: the day's
        // dismantles are there to be used (5.3). Plain junk is sold
        let worth_materials = gs
            .character
            .inventory
            .backpack
            .get(pos.backpack_pos())
            .and_then(|i| i.as_ref())
            .is_some_and(|i| i.is_epic() || i.upgrade_count > 0);
        return Some(Sale {
            pos,
            what,
            disposal: if can_dismantle && worth_materials {
                Disposal::Dismantle
            } else {
                Disposal::Sell
            },
        });
    }
    if free_slots(gs) > 0 {
        // Still a slot to work with - the emergency tier waits until the
        // bag really blocks something
        return None;
    }
    let emergency = Protection {
        epics: false,
        enchanted: true,
        upgraded: false,
        legendary_pct: 0.0,
    };
    let (pos, what) = weakest(gs, &emergency)?;
    let item = gs
        .character
        .inventory
        .backpack
        .get(pos.backpack_pos())
        .and_then(|i| i.as_ref())?;
    let disposal = if can_dismantle
        && (item.is_epic() || item.upgrade_count > 0)
    {
        Disposal::Dismantle
    } else {
        Disposal::Sell
    };
    Some(Sale {
        pos,
        what: format!("{what} (bag completely full, nothing else could go)"),
        disposal,
    })
}

/// The weakest piece of gear the given protections let go and nobody in the
/// party wants
fn weakest(
    gs: &GameState,
    protection: &Protection,
) -> Option<(BagPosition, String)> {
    let mut worst: Option<(f64, BagPosition, String)> = None;
    for (pos, slot) in gs.character.inventory.iter() {
        let Some(item) = slot else { continue };
        if !is_gear(item) {
            continue;
        }
        if protected_reason(item, protection).is_some() {
            continue;
        }
        // An upgrade for anybody in the party stays
        let is_upgrade = crate::shopping::party_wants(gs, item);
        if is_upgrade {
            continue;
        }
        let score = item_score(item, gs.character.class);
        if worst.as_ref().is_none_or(|(best, ..)| score < *best) {
            let kind = if item.is_epic() { "epic " } else { "" };
            worst = Some((
                score,
                pos,
                format!("{kind}{:?} worth {score:.0}", item.typ),
            ));
        }
    }
    let (_, pos, what) = worst?;
    Some((pos, what))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_gear_is_ever_sold_for_room() {
        // Everything that is not a wearable piece has another use: potions,
        // dungeon keys, resources, pet fruit, the scrapbook itself
        let gear = [
            ItemType::Hat,
            ItemType::Belt,
            ItemType::Ring,
            ItemType::Weapon {
                min_dmg: 1,
                max_dmg: 2,
            },
        ];
        let not_gear = [
            ItemType::Scrapbook,
            ItemType::QuickSandGlass,
            ItemType::ToiletKey,
            ItemType::Gral,
            ItemType::Unknown(7),
        ];
        for typ in gear {
            assert!(is_gear_type(typ), "{typ:?} should count as gear");
        }
        for typ in not_gear {
            assert!(!is_gear_type(typ), "{typ:?} should be left alone");
        }
    }
}
