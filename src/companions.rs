//! Dressing Bert, Mark and Kunigunde out of what the character already found.
//!
//! The three of them fight every shadow dungeon alongside the character, and
//! their gear only ever comes from the backpack - there is no shop for them.
//! So every piece the character does not want is worth a look before it is
//! sold, and until now the good ones simply sat in the bag until the
//! backpack step sold them or the bag filled up and stopped the dungeons.
//!
//! Two rules come from the game, not from a guess: a companion can not wear
//! a shield, and it can only wear what its own class can wear. `sf-api`
//! answers both through [`Dungeons::can_companion_equip`].

use sf_api::gamestate::{
    GameState,
    character::Class,
    dungeons::CompanionClass,
    items::{BagPosition, EquipmentSlot},
};

use crate::shopping::{Protection, character_wants, item_score, protected_reason};

/// One piece to put on one companion
#[derive(Debug, Clone)]
pub struct Move {
    pub bag_pos: BagPosition,
    pub slot: EquipmentSlot,
    pub companion: CompanionClass,
    /// For the log
    pub what: String,
}

/// The name the game uses, so the log says who got the piece
#[must_use]
pub fn companion_name(companion: CompanionClass) -> &'static str {
    match companion {
        CompanionClass::Warrior => "Bert",
        CompanionClass::Mage => "Mark",
        CompanionClass::Scout => "Kunigunde",
    }
}

/// The single biggest improvement available right now, or `None` when there
/// is nothing in the bag worth putting on anybody
#[must_use]
pub fn next_move(gs: &GameState, protection: &Protection) -> Option<Move> {
    let companions = gs.dungeons.companions.as_ref()?;

    let mut best: Option<(f64, Move)> = None;
    for (bag_pos, slot) in gs.character.inventory.iter() {
        let Some(item) = slot else { continue };
        let Some(eq_slot) = item.typ.equipment_slot() else {
            continue;
        };
        if character_wants(gs, item) {
            continue;
        }
        for (typ, companion) in companions {
            // Shields and the wrong class are refused by the game itself
            if !gs.dungeons.can_companion_equip(typ, item) {
                continue;
            }
            let class = Class::from(typ);
            if companion.equipment.0[eq_slot].as_ref()
                .is_some_and(|w| protected_reason(w, protection).is_some())
            {
                continue;
            }
            let worn = companion.equipment.0[eq_slot]
                .as_ref()
                .map_or(0.0, |w| item_score(w, class));
            let gain = item_score(item, class) - worn;
            if gain <= 0.0 {
                continue;
            }
            if best.as_ref().is_none_or(|(g, _)| gain > *g) {
                best = Some((
                    gain,
                    Move {
                        bag_pos,
                        slot: eq_slot,
                        companion: typ,
                        what: format!(
                            "{} gets a {:?} (+{gain:.0})",
                            companion_name(typ),
                            item.typ
                        ),
                    },
                ));
            }
        }
    }
    best.map(|(_, m)| m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn everyone_has_a_name() {
        assert_eq!(companion_name(CompanionClass::Warrior), "Bert");
        assert_eq!(companion_name(CompanionClass::Mage), "Mark");
        assert_eq!(companion_name(CompanionClass::Scout), "Kunigunde");
    }

    /// The rule is the game's, not ours - a companion never wears a shield,
    /// whatever the numbers on it say. Worth pinning down, because a shield
    /// scores well and would otherwise look like the best move on the table
    #[test]
    fn a_companion_never_wears_a_shield() {
        use sf_api::gamestate::items::ItemType;
        let shield = ItemType::Shield { block_chance: 50 };
        assert!(
            shield.equipment_slot().is_some(),
            "a shield is equipment - it is only companions that may not wear \
             one, which is exactly why this needs its own check"
        );
    }
}
