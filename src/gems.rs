//! What to do with a gem once the mine has finished digging it up.
//!
//! The mine can not be steered - the game only tells you what it found after
//! the search is over. So everything here happens *after* collecting: look at
//! the gem, put it into a piece of gear that can use it, and sell what is left
//! over if that is what was asked for.
//!
//! Four people can wear gems: the character, and the three companions from the
//! tower - Bert the warrior, Mark the mage and Kunigunde the scout. A gem is
//! worth socketing for someone whose main attribute it matches; the
//! all-attribute and legendary gems fit everyone.

use sf_api::gamestate::{
    character::Class,
    dungeons::CompanionClass,
    items::{
        BagPosition, EquipmentSlot, Gem, GemSlot, GemType, Inventory, Item,
        ItemType,
    },
};

use crate::config::GemWanted;

/// Who a gem could go to
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GemOwner {
    /// The player character
    Character,
    /// Bert, Mark or Kunigunde
    Companion(CompanionClass),
}

impl GemOwner {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            GemOwner::Character => "the character",
            GemOwner::Companion(CompanionClass::Warrior) => "Bert",
            GemOwner::Companion(CompanionClass::Mage) => "Mark",
            GemOwner::Companion(CompanionClass::Scout) => "Kunigunde",
        }
    }
}

/// What should happen with one gem in the backpack
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GemAction {
    /// Put it into a gem slot of a worn piece - an empty one, or one that
    /// holds a weaker gem of the same colour (`replaces`), which the game
    /// destroys on the way
    Socket {
        bag_pos: BagPosition,
        owner: GemOwner,
        slot: EquipmentSlot,
        gem: Gem,
        replaces: Option<Gem>,
    },
    /// Not wanted and not rare - turn it into gold
    Sell { bag_pos: BagPosition, gem: Gem },
}

/// The rare gems only come from events. They fit every class and they can not
/// be farmed again, so they are never sold, whatever the settings say
#[must_use]
pub fn is_rare(typ: GemType) -> bool {
    matches!(typ, GemType::All | GemType::Legendary)
}

/// Whether this gem is worth a slot on that owner. The main attribute is the
/// obvious one; constitution and luck help every class, and the rare gems fit
/// everyone by definition
#[must_use]
pub fn fits_class(typ: GemType, class: Class) -> bool {
    use sf_api::command::AttributeType as A;
    if is_rare(typ) || matches!(typ, GemType::Constitution | GemType::Luck) {
        return true;
    }
    matches!(
        (typ, class.main_attribute()),
        (GemType::Strength, A::Strength)
            | (GemType::Dexterity, A::Dexterity)
            | (GemType::Intelligence, A::Intelligence)
    )
}

/// The gem inside an item, if it is a gem at all
#[must_use]
pub fn gem_of(item: &Item) -> Option<Gem> {
    match item.typ {
        ItemType::Gem(gem) => Some(gem),
        _ => None,
    }
}

/// The worn slots in the order they are considered. Weapons and body armour
/// first, because those are the pieces that stay the longest
const SLOT_ORDER: [EquipmentSlot; 10] = [
    EquipmentSlot::Weapon,
    EquipmentSlot::BreastPlate,
    EquipmentSlot::Shield,
    EquipmentSlot::Amulet,
    EquipmentSlot::Hat,
    EquipmentSlot::Belt,
    EquipmentSlot::Gloves,
    EquipmentSlot::FootWear,
    EquipmentSlot::Ring,
    EquipmentSlot::Talisman,
];

/// One owner and the gear they are wearing
pub struct Wearer<'a> {
    pub owner: GemOwner,
    pub class: Class,
    pub equipment: &'a sf_api::gamestate::items::Equipment,
}

/// The first worn piece of `wearer` with an empty gem slot
fn free_slot(wearer: &Wearer) -> Option<EquipmentSlot> {
    SLOT_ORDER.into_iter().find(|slot| {
        matches!(
            wearer.equipment.0[*slot]
                .as_ref()
                .and_then(|i| i.gem_slot.as_ref()),
            Some(GemSlot::Empty)
        )
    })
}

/// A gem only replaces a worn one of the same colour when it is at least
/// this much stronger - the guild leader's rule. The game destroys the old
/// gem, so a 233 for a 250 is a loss of a gem for 17 points
pub const REPLACE_MIN_GAIN: u32 = 100;

/// The worn piece of `wearer` whose gem is the weakest one of the same
/// colour as `gem` and at least `REPLACE_MIN_GAIN` weaker - the best place
/// for an upgrade. A new gem put on a piece destroys the old one, so only
/// the same colour counts (a strength gem over a dexterity gem is not an
/// upgrade), and the rare ones - all-attribute, legendary - are never
/// replaced by anything
fn weakest_same_colour(wearer: &Wearer, gem: Gem) -> Option<(EquipmentSlot, Gem)> {
    SLOT_ORDER
        .into_iter()
        .filter_map(|slot| {
            let worn = wearer.equipment.0[slot].as_ref()?;
            match worn.gem_slot.as_ref()? {
                GemSlot::Filled(old)
                    if old.typ == gem.typ
                        && !is_rare(old.typ)
                        && gem.value >= old.value + REPLACE_MIN_GAIN =>
                {
                    Some((slot, *old))
                }
                _ => None,
            }
        })
        .min_by_key(|(_, old)| old.value)
}

/// Decides what to do with the gems lying in the backpack.
///
/// `wanted` is what the player asked to keep - an empty list means keep
/// everything. `socket` is the switch; when off this returns nothing and the
/// gems just pile up, which is the old behaviour.
///
/// Only one action is returned at a time: every one of them changes the
/// backpack, so the next decision has to be made from the state after it.
#[must_use]
pub fn next_action(
    inventory: &Inventory,
    wearers: &[Wearer],
    wanted: &[GemWanted],
    socket: bool,
) -> Option<GemAction> {
    let gems: Vec<(BagPosition, Gem)> = inventory
        .iter()
        .filter_map(|(pos, item)| Some((pos, gem_of(item?)?)))
        .collect();
    if gems.is_empty() {
        return None;
    }

    // 1) Socketing beats everything - a gem in a slot is worth more than the
    //    gold it sells for. The strongest gem goes first, so a weak one does
    //    not take the only free slot
    if socket {
        let mut by_value = gems.clone();
        by_value.sort_by_key(|(_, gem)| std::cmp::Reverse(gem.value));
        for (bag_pos, gem) in &by_value {
            for wearer in wearers {
                if !fits_class(gem.typ, wearer.class) {
                    continue;
                }
                if let Some(slot) = free_slot(wearer) {
                    return Some(GemAction::Socket {
                        bag_pos: *bag_pos,
                        owner: wearer.owner,
                        slot,
                        gem: *gem,
                        replaces: None,
                    });
                }
            }
        }
        // 1b) No empty slot anywhere: a bigger gem still goes in, over the
        //     weakest one of the same colour - a 300 dexterity gem over a
        //     233 is worth the 233 the game destroys. The character first,
        //     then the companions, each only for the colour they fight with.
        //     A protected colour is never replaced - that is what the
        //     protector is for
        for (bag_pos, gem) in &by_value {
            if protected(wanted, gem.typ) {
                continue;
            }
            for wearer in wearers {
                if !fits_class(gem.typ, wearer.class) {
                    continue;
                }
                if let Some((slot, old)) = weakest_same_colour(wearer, *gem) {
                    return Some(GemAction::Socket {
                        bag_pos: *bag_pos,
                        owner: wearer.owner,
                        slot,
                        gem: *gem,
                        replaces: Some(old),
                    });
                }
            }
        }
    }

    // 2) The guild leader's rule against a bag full of leftovers: a gem
    //    that is 100 or more under the average of the gems the party
    //    wears is never going in anywhere - sold. Rare and protected
    //    colours excepted
    if socket && let Some(avg) = worn_gem_average(wearers) {
        for (bag_pos, gem) in &gems {
            if !is_rare(gem.typ)
                && !protected(wanted, gem.typ)
                && f64::from(gem.value) + f64::from(SELL_UNDER_AVERAGE) <= avg
            {
                return Some(GemAction::Sell {
                    bag_pos: *bag_pos,
                    gem: *gem,
                });
            }
        }
    }
    None
}

/// How far under the worn gems' average a bag gem has to be to be sold
pub const SELL_UNDER_AVERAGE: u32 = 100;

/// Is this colour on the protector list - never sold, never replaced?
fn protected(wanted: &[GemWanted], typ: GemType) -> bool {
    wanted.iter().any(|w| w.matches(typ))
}

/// The average strength of the plain gems the party wears - `None` while
/// nobody wears one, so the rule does not fire on an empty set
pub fn worn_gem_average(wearers: &[Wearer]) -> Option<f64> {
    let worn: Vec<u32> = wearers
        .iter()
        .flat_map(|w| SLOT_ORDER.into_iter().filter_map(|slot| w.equipment.0[slot].as_ref()))
        .filter_map(|item| match item.gem_slot {
            Some(GemSlot::Filled(gem)) if !is_rare(gem.typ) => Some(gem.value),
            _ => None,
        })
        .collect();
    if worn.is_empty() {
        return None;
    }
    Some(f64::from(worn.iter().sum::<u32>()) / worn.len() as f64)
}

/// "green intelligence gem (42)" - the colour is what the player sees, the
/// attribute is what it does
#[must_use]
pub fn gem_name(gem: Gem) -> String {
    let name = match gem.typ {
        GemType::Strength => "blue strength",
        GemType::Dexterity => "yellow dexterity",
        GemType::Intelligence => "green intelligence",
        GemType::Constitution => "purple constitution",
        GemType::Luck => "luck",
        GemType::All => "all-attribute",
        GemType::Legendary => "legendary",
    };
    format!("{name} gem ({})", gem.value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rare_gems_fit_everyone_and_are_never_sold() {
        for class in [Class::Warrior, Class::Mage, Class::Scout] {
            assert!(fits_class(GemType::All, class));
            assert!(fits_class(GemType::Legendary, class));
        }
        assert!(is_rare(GemType::All));
        assert!(is_rare(GemType::Legendary));
        assert!(!is_rare(GemType::Strength));
    }

    #[test]
    fn a_gem_only_fits_the_class_that_fights_with_it() {
        assert!(fits_class(GemType::Strength, Class::Warrior));
        assert!(!fits_class(GemType::Strength, Class::Mage));
        assert!(fits_class(GemType::Intelligence, Class::Mage));
        assert!(fits_class(GemType::Dexterity, Class::Scout));
        assert!(!fits_class(GemType::Dexterity, Class::Warrior));
    }

    /// Purple (constitution) and red (luck) go to everybody - Bert, Mark
    /// and Kunigunde each fight with their own colour and these two
    #[test]
    fn purple_and_red_fit_every_companion() {
        for class in [Class::Warrior, Class::Mage, Class::Scout] {
            assert!(fits_class(GemType::Constitution, class));
            assert!(fits_class(GemType::Luck, class));
        }
        // ...and a companion with a free slot takes one when the
        // character has none free
        use sf_api::gamestate::items::Equipment;
        let mut hat = sf_api::gamestate::Shop::default().items[0].clone();
        hat.typ = ItemType::Hat;
        hat.gem_slot = Some(GemSlot::Empty);
        let mut bert = Equipment::default();
        bert.0[EquipmentSlot::Hat] = Some(hat.clone());
        let character = Equipment::default();
        let wearers = [
            Wearer {
                owner: GemOwner::Character,
                class: Class::Mage,
                equipment: &character,
            },
            Wearer {
                owner: GemOwner::Companion(CompanionClass::Warrior),
                class: Class::Warrior,
                equipment: &bert,
            },
        ];
        let mut purple = sf_api::gamestate::Shop::default().items[0].clone();
        purple.typ = ItemType::Gem(Gem {
            typ: GemType::Constitution,
            value: 200,
        });
        let inventory = Inventory {
            backpack: vec![Some(purple)],
        };
        match next_action(&inventory, &wearers, &[], true) {
            Some(GemAction::Socket { owner, .. }) => {
                assert_eq!(owner, GemOwner::Companion(CompanionClass::Warrior));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn an_empty_backpack_gives_nothing_to_do() {
        let empty = Inventory::default();
        assert_eq!(next_action(&empty, &[], &[], true), None);
    }

    /// A bigger gem of the same colour replaces the weakest worn one; a
    /// different colour or a smaller one does not
    #[test]
    fn a_bigger_gem_of_the_same_colour_replaces_the_weakest_one() {
        use sf_api::gamestate::items::Equipment;
        let mut equipment = Equipment::default();
        let mut hat = sf_api::gamestate::Shop::default().items[0].clone();
        hat.typ = ItemType::Hat;
        hat.gem_slot = Some(GemSlot::Filled(Gem {
            typ: GemType::Dexterity,
            value: 233,
        }));
        let mut belt = hat.clone();
        belt.typ = ItemType::Belt;
        belt.gem_slot = Some(GemSlot::Filled(Gem {
            typ: GemType::Dexterity,
            value: 180,
        }));
        equipment.0[EquipmentSlot::Hat] = Some(hat);
        equipment.0[EquipmentSlot::Belt] = Some(belt);
        let scout = Wearer {
            owner: GemOwner::Character,
            class: Class::Scout,
            equipment: &equipment,
        };
        let big = Gem {
            typ: GemType::Dexterity,
            value: 300,
        };
        assert_eq!(
            weakest_same_colour(&scout, big).map(|(s, old)| (s, old.value)),
            Some((EquipmentSlot::Belt, 180))
        );
        let small = Gem {
            typ: GemType::Dexterity,
            value: 100,
        };
        assert_eq!(weakest_same_colour(&scout, small), None);
        // Better, but not by the hundred the rule wants: 180 -> 250 is 70
        let a_bit = Gem {
            typ: GemType::Dexterity,
            value: 250,
        };
        assert_eq!(weakest_same_colour(&scout, a_bit), None);
        let other_colour = Gem {
            typ: GemType::Strength,
            value: 999,
        };
        assert_eq!(weakest_same_colour(&scout, other_colour), None);
        // A rare gem on the belt is never replaced, however small
        equipment.0[EquipmentSlot::Belt].as_mut().unwrap().gem_slot =
            Some(GemSlot::Filled(Gem {
                typ: GemType::All,
                value: 10,
            }));
        let scout = Wearer {
            owner: GemOwner::Character,
            class: Class::Scout,
            equipment: &equipment,
        };
        let rare_big = Gem {
            typ: GemType::All,
            value: 999,
        };
        assert_eq!(weakest_same_colour(&scout, rare_big), None);
        // ...while the 233 hat still takes a 340 (a gain of 107)
        let hat_upgrade = Gem {
            typ: GemType::Dexterity,
            value: 340,
        };
        assert_eq!(
            weakest_same_colour(&scout, hat_upgrade).map(|(s, _)| s),
            Some(EquipmentSlot::Hat)
        );
    }

    /// Hat 233, belt 180 worn: average 206.5. A 100 in the bag is 106
    /// under - sold; a 150 is only 56 under - kept
    #[test]
    fn a_gem_far_under_the_worn_average_is_sold() {
        use sf_api::gamestate::items::Equipment;
        let mut equipment = Equipment::default();
        let mut hat = sf_api::gamestate::Shop::default().items[0].clone();
        hat.typ = ItemType::Hat;
        hat.gem_slot = Some(GemSlot::Filled(Gem {
            typ: GemType::Dexterity,
            value: 233,
        }));
        let mut belt = hat.clone();
        belt.typ = ItemType::Belt;
        belt.gem_slot = Some(GemSlot::Filled(Gem {
            typ: GemType::Dexterity,
            value: 180,
        }));
        equipment.0[EquipmentSlot::Hat] = Some(hat);
        equipment.0[EquipmentSlot::Belt] = Some(belt);
        let scout = Wearer {
            owner: GemOwner::Character,
            class: Class::Scout,
            equipment: &equipment,
        };
        let avg = worn_gem_average(&[scout]).unwrap();
        assert!((avg - 206.5).abs() < 0.01);
        assert!(f64::from(100 + SELL_UNDER_AVERAGE) <= avg);
        assert!(f64::from(150 + SELL_UNDER_AVERAGE) > avg);
        // Nobody wears a gem: no average, nothing is sold by this rule
        let bare = Equipment::default();
        let nobody = Wearer {
            owner: GemOwner::Character,
            class: Class::Scout,
            equipment: &bare,
        };
        assert!(worn_gem_average(&[nobody]).is_none());
    }

    #[test]
    fn colours_are_the_ones_on_the_gem_in_the_game() {
        assert_eq!(
            gem_name(Gem {
                typ: GemType::Intelligence,
                value: 42
            }),
            "green intelligence gem (42)"
        );
    }
}
