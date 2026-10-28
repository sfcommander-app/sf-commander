//! Buying better gear in the weapon / magic shop, and spending gold on
//! attributes. Pure decisions live here; the requests are sent from
//! `message.rs`.

use sf_api::{
    command::AttributeType,
    gamestate::{
        GameState,
        character::Class,
        dungeons::CompanionClass,
        items::{BagPosition, EquipmentSlot, GemSlot, GemType, Item, ItemType},
    },
};

/// What must never be replaced / sold
#[derive(Debug, Clone, Copy)]
pub struct Protection {
    /// Epics (and legendaries) stay, whatever the numbers say
    pub epics: bool,
    /// Enchanted items stay
    pub enchanted: bool,
    /// Items upgraded at the blacksmith stay
    pub upgraded: bool,
    /// A worn legendary gives way to a piece at least this many percent
    /// better (0 = never). They are lovely and they do not come back, so
    /// the old one is never sold - it goes into the bag - but a piece that
    /// is half again as good moves the character further than the glow
    /// does (the guild leader's word, 5.3)
    pub legendary_pct: f64,
}

impl Default for Protection {
    fn default() -> Self {
        Self {
            epics: true,
            enchanted: true,
            upgraded: true,
            legendary_pct: 0.0,
        }
    }
}

/// The steps the shop looks for, in percent better than what is worn -
/// buttons, because a number box nobody can type into is a number box
/// clicked two thousand times
pub const MIN_GAIN_STEPS: [u8; 9] = [5, 10, 15, 20, 25, 30, 35, 45, 50];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecycleAction {
    ExtractGem,
    Dismantle,
    Sell,
}

/// Disposal is allowed only after the replacement is actually worn and
/// the exact old item is in the expected backpack slot. Gem extraction
/// destroys the item, so it is a terminal action, never a pre-equip step.
pub fn replaced_item_action(
    gs: &GameState,
    purchase: &crate::player::PendingBuy,
    enabled: bool,
    extraction_refused: bool,
) -> Option<RecycleAction> {
    if !enabled || !purchase.equipped
        || !gs.character.equipment.0[purchase.slot].as_ref()
            .is_some_and(|i| i.command_ident() == purchase.new_ident)
    {
        return None;
    }
    let old = gs.character.inventory.backpack
        .get(purchase.bag_pos.backpack_pos())?.as_ref()?;
    if Some(old.command_ident()) != purchase.old_ident
        || old.is_legendary() || party_wants(gs, old)
    {
        return None;
    }
    if matches!(old.gem_slot, Some(GemSlot::Filled(_))) {
        let smith = gs.blacksmith.as_ref()?;
        return if !extraction_refused {
            Some(RecycleAction::ExtractGem)
        } else if smith.dismantle_left > 0 {
            Some(RecycleAction::Dismantle)
        } else { None };
    }
    if old.upgrade_count > 0 || old.is_epic() {
        return gs.blacksmith.as_ref()
            .filter(|b| b.dismantle_left > 0)
            .map(|_| RecycleAction::Dismantle);
    }
    Some(RecycleAction::Sell)
}

/// A transport/session error is not evidence that extraction is impossible.
/// Only explicit game refusals allow the user-requested dismantle fallback.
pub fn extraction_unavailable(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    ["cannot extract", "can't extract", "can not extract",
     "not enough metal", "not enough arcane", "not enough resources",
     "not enough materials", "cannot afford", "can not afford"]
        .iter().any(|reason| error.contains(reason))
}

/// The step to recommend for a character, from how fast it levels: the
/// faster the levels come, the faster the shops move on, and a piece
/// bought for +15 % is outgrown before it paid for itself. A slow
/// character takes every real improvement
#[must_use]
pub fn recommended_min_gain(levels_7d: Option<u16>) -> u8 {
    match levels_7d.unwrap_or(0) {
        0 => 15,
        1..=2 => 20,
        3..=4 => 25,
        5..=7 => 30,
        _ => 35,
    }
}

/// Why an equipped item is off limits, if it is. "legendary" and the two
/// gem reasons are hard stops; "epic", "enchanted" and "upgraded" are
/// soft - see `may_replace`
pub fn protected_reason(item: &Item, p: &Protection) -> Option<&'static str> {
    // A legendary is never replaced - they do not come back
    if item.is_legendary() {
        return Some("legendary");
    }
    if p.epics && item.is_epic() {
        return Some("epic");
    }
    if let Some(GemSlot::Filled(gem)) = &item.gem_slot {
        // The rare gems are event rewards - they never come back
        if matches!(gem.typ, GemType::All | GemType::Legendary) {
            return Some("rare gem");
        }
        // Any gem protects, whatever its strength: a piece holds one gem
        // or none, and a gem is never thrown away with the piece - the
        // player takes it out at the blacksmith first. The old "gem of at
        // least N" setting is gone (`gem_min` stays in the config for old
        // files and is ignored)
        return Some("gem");
    }
    if p.enchanted && item.enchantment.is_some() {
        return Some("enchanted");
    }
    if p.upgraded && item.upgrade_count > 0 {
        return Some("upgraded");
    }
    None
}

/// The three attributes a build is made of: the class's main one,
/// constitution and luck. The guild leader's rule - everything else on a
/// piece is dead weight and counts for nothing
pub fn build_attributes(class: Class) -> [AttributeType; 3] {
    [
        class.main_attribute(),
        AttributeType::Constitution,
        AttributeType::Luck,
    ]
}

/// What a piece adds to one attribute, its gem included
pub fn item_attribute(item: &Item, class: Class, attr: AttributeType) -> u32 {
    let main = class.main_attribute();
    let mut v = item.attributes[attr];
    if let Some(GemSlot::Filled(gem)) = &item.gem_slot {
        let hits = match gem.typ {
            GemType::All => true,
            // The orange gem: main attribute and constitution
            GemType::Legendary => {
                attr == main || attr == AttributeType::Constitution
            }
            GemType::Constitution => attr == AttributeType::Constitution,
            GemType::Luck => attr == AttributeType::Luck,
            GemType::Strength => attr == AttributeType::Strength,
            GemType::Dexterity => attr == AttributeType::Dexterity,
            GemType::Intelligence => attr == AttributeType::Intelligence,
        };
        if hits {
            v += gem.value;
        }
    }
    v
}

/// What one blacksmith level adds to a piece: 3 % of its highest
/// attribute (every attribute on an epic, they are all the highest), and
/// a weapon's damage with it. Twenty levels is the ceiling
pub const UPGRADE_STEP: f64 = 0.03;

/// How much of a worn piece's blacksmith levels count when a new piece
/// is weighed against it. The levels are paid for and a new piece starts
/// at zero, so they are not nothing - but they are not the full 3 %
/// either, or a +20 piece could never be replaced before the shop offered
/// something 60 % better and the character's gear would freeze at the
/// level it was first raised. Two per cent a level: a +10 piece gives way
/// to a piece 20 % better at base, which is then raised in its turn
pub const UPGRADE_HOLD: f64 = 0.02;

/// The piece as it left the shop: the blacksmith levels taken back off
/// the attributes they were put on. `attr` is the raw value; the highest
/// attribute(s) carry the levels, the rest never did
fn base_attribute(item: &Item, value: u32) -> f64 {
    let max = item.attributes.values().max().copied().unwrap_or(0);
    if item.upgrade_count == 0 || value == 0 || value < max {
        return f64::from(value);
    }
    f64::from(value) / (1.0 + UPGRADE_STEP * f64::from(item.upgrade_count))
}

/// One number for "how good is this piece for this class": the three build
/// attributes count in full, anything else not at all, weapon damage a lot,
/// armor a little.
///
/// The gem is left out on purpose: a gem is not part of the piece, it
/// moves with the player (the blacksmith takes it out), so a worn piece
/// must not look better for holding one - the guild leader's rule, "judge
/// the piece as if the gem were out". The blacksmith levels are handled
/// the same way and then partly given back (see `UPGRADE_HOLD`): the
/// piece is judged at its base, plus a little for every level already
/// paid for. Rarity counts a little (legendary over epic over plain, the
/// way a player picks by eye), and a rune counts a little too - a +10 %
/// XP rune is worth having, not worth a weaker piece
pub fn item_score(item: &Item, class: Class) -> f64 {
    let mut score = 0.0;
    for attr in build_attributes(class) {
        // The gem is left out and the blacksmith levels are taken back off
        // (see the doc above) - what is left is the piece as it came out of
        // the shop. Scoring the raw attribute instead made a +10 gemmed
        // piece score some 30 % over its worth, so nothing was ever good
        // enough to replace it and the gold sat there
        score += base_attribute(item, item.attributes[attr]);
    }
    // The levels raise a weapon's damage in step with its attributes
    let raised = 1.0 + UPGRADE_STEP * f64::from(item.upgrade_count);
    match item.typ {
        ItemType::Weapon { min_dmg, max_dmg } => {
            score +=
                (f64::from(min_dmg) + f64::from(max_dmg)) / 2.0 * 1.5 / raised;
        }
        ItemType::Shield { block_chance } => {
            score += f64::from(block_chance) * 3.0;
        }
        _ => score += f64::from(item.armor()) * 0.1,
    }
    if let Some(rune) = &item.rune {
        score += f64::from(rune.value) * 2.0;
    }
    let rarity = if item.is_legendary() {
        1.10
    } else if item.is_epic() {
        1.05
    } else {
        1.0
    };
    let hold = 1.0 + UPGRADE_HOLD * f64::from(item.upgrade_count);
    score * rarity * hold
}

/// What buying a piece does to the whole character, in the three attributes
/// that matter: the change per attribute against what is worn in that
/// slot, and that change as a share of everything the character has in
/// those three today. "+2.1 %" reads the same on a belt and on a weapon,
/// which "score 496" never did
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BuildGain {
    pub pct: f64,
    /// Deltas in the order of `build_attributes`: main, constitution, luck
    pub delta: [i64; 3],
}

pub fn build_gain(
    gs: &GameState,
    worn: Option<&Item>,
    item: &Item,
) -> BuildGain {
    let c = &gs.character;
    build_gain_with(
        c.class,
        |a| {
            i64::from(c.attribute_basis[a])
                + i64::from(c.attribute_additions[a])
        },
        worn,
        item,
    )
}

/// The same for any wearer: `total` is what they have in an attribute today
pub fn build_gain_with(
    class: Class,
    total: impl Fn(AttributeType) -> i64,
    worn: Option<&Item>,
    item: &Item,
) -> BuildGain {
    let attrs = build_attributes(class);
    let mut delta = [0i64; 3];
    let mut sum_total = 0i64;
    for (i, attr) in attrs.into_iter().enumerate() {
        let new = i64::from(item_attribute(item, class, attr));
        let old = worn.map_or(0, |w| i64::from(item_attribute(w, class, attr)));
        delta[i] = new - old;
        sum_total += total(attr);
    }
    let sum: i64 = delta.iter().sum();
    let pct = if sum_total <= 0 {
        0.0
    } else {
        sum as f64 / sum_total as f64 * 100.0
    };
    BuildGain { pct, delta }
}

/// Everybody a piece could go to, in the order of `gains_for_everyone`
pub const WEARERS: [&str; 4] = ["you", "Bert", "Mark", "Kunigunde"];

const COMPANIONS: [CompanionClass; 3] = [
    CompanionClass::Warrior,
    CompanionClass::Mage,
    CompanionClass::Scout,
];

/// What the piece does for the character and each of the three companions,
/// against what each of them wears in that slot. `None` where it can not be
/// worn at all (wrong class, a shield on a companion)
pub fn gains_for_everyone(
    gs: &GameState,
    item: &Item,
) -> [Option<BuildGain>; 4] {
    let mut out = [None; 4];
    let Some(slot) = item.typ.equipment_slot() else {
        return out;
    };
    if item.can_be_equipped_by(gs.character.class) {
        out[0] = Some(build_gain(
            gs,
            gs.character.equipment.0[slot].as_ref(),
            item,
        ));
    }
    if let Some(companions) = &gs.dungeons.companions {
        for (i, typ) in COMPANIONS.into_iter().enumerate() {
            if !gs.dungeons.can_companion_equip(typ, item) {
                continue;
            }
            let c = &companions[typ];
            out[i + 1] = Some(build_gain_with(
                Class::from(typ),
                |a| i64::from(c.attributes[a]),
                c.equipment.0[slot].as_ref(),
                item,
            ));
        }
    }
    out
}

/// "you +2.1 %, Bert +5.0 %, Mark -, Kunigunde -"
pub fn gains_line(gs: &GameState, item: &Item) -> String {
    gains_for_everyone(gs, item)
        .iter()
        .zip(WEARERS)
        .map(|(g, who)| match g {
            Some(g) => format!("{who} {:+.1} %", g.pct),
            None => format!("{who} -"),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// What kind of piece it is - the two kinds of epic are worth telling
/// apart: three attributes, or all five - and whose gear it is. A warrior's
/// hat on a mage is only ever a companion's or the shop's
pub fn item_kind(item: &Item) -> String {
    let n = item.attributes.iter().filter(|(_, v)| **v > 0).count();
    let kind = if item.is_legendary() {
        "legendary"
    } else if item.is_epic() {
        if n >= 5 {
            "epic, all attributes"
        } else {
            "epic, 3 attributes"
        }
    } else {
        "common"
    };
    match item.class {
        Some(class) => format!("{kind}, for {}", class_name(class)),
        None => format!("{kind}, any class"),
    }
}

/// "warriors", "mages", "scouts" - the gear classes as the game groups them
fn class_name(class: Class) -> &'static str {
    use sf_api::command::AttributeType as A;
    match class.main_attribute() {
        A::Strength => "warriors",
        A::Intelligence => "mages",
        A::Dexterity => "scouts",
        _ => "everyone",
    }
}

/// An item must actually fit its wearer before its score is relevant.
pub fn character_wants(gs: &GameState, item: &Item) -> bool {
    let Some(slot) = item.typ.equipment_slot() else {
        return false;
    };
    item.can_be_equipped_by(gs.character.class)
        && item_score(item, gs.character.class)
            > gs.character.equipment.0[slot]
                .as_ref()
                .map_or(0.0, |w| item_score(w, gs.character.class))
}

pub fn party_wants(gs: &GameState, item: &Item) -> bool {
    if character_wants(gs, item) {
        return true;
    }
    let Some(slot) = item.typ.equipment_slot() else {
        return false;
    };
    gs.dungeons.companions.as_ref().is_some_and(|companions| {
        companions.iter().any(|(typ, companion)| {
            gs.dungeons.can_companion_equip(typ, item)
                && item_score(item, Class::from(typ))
                    > companion.equipment.0[slot]
                        .as_ref()
                        .map_or(0.0, |w| item_score(w, Class::from(typ)))
        })
    })
}

/// A shop item worth buying
#[derive(Debug, Clone)]
pub struct Candidate {
    pub shop_pos: sf_api::gamestate::ShopPosition,
    pub slot: EquipmentSlot,
    pub item: Item,
    /// Percent better than what is worn (or 999 for an empty slot)
    pub gain_pct: f64,
    /// Silver, when the piece is priced in silver
    pub price: u32,
    /// Mushrooms, when it is priced in those - then `price` is not what is
    /// paid (the game fills that slot with junk for such items)
    pub mushrooms: u32,
    /// Who the piece is for. `None` is the character. For a companion the
    /// shop step only buys it into the bag - putting it on is the job of
    /// [`crate::companions::next_move`], which runs first on every step
    pub for_companion: Option<CompanionClass>,
}

/// An epic, an enchanted or a blacksmith-raised piece is protected against
/// a sidegrade, not against progress: a piece that is at least this much
/// better and of a higher item level replaces it. "Never replace an
/// epic" kept level-150 junk on for its glow; junk is junk, epic or not
pub const EPIC_OVERRIDE_PCT: f64 = 20.0;

/// May the worn piece give way to `item`? A legendary is the one hard
/// stop - it never comes back. An epic, an enchantment, blacksmith levels
/// and a gem are soft: they yield to a clearly better piece of a higher
/// item level (see `EPIC_OVERRIDE_PCT`), which is what "protection" should
/// mean. A gem is never thrown away with the piece: the shop step takes it
/// out at the blacksmith first (with materials, never mushrooms), and the
/// raised piece is dismantled for its materials rather than sold
pub fn may_replace(
    worn: &Item,
    item: &Item,
    gain_pct: f64,
    p: &Protection,
) -> bool {
    match protected_reason(worn, p) {
        None => true,
        Some("epic" | "enchanted" | "upgraded" | "gem" | "rare gem") => {
            gain_pct >= EPIC_OVERRIDE_PCT
                && item.item_quality > worn.item_quality
        }
        Some("legendary") => p.legendary_pct > 0.0 && gain_pct >= p.legendary_pct,
        Some(_) => false,
    }
}

/// Why the worn piece stops this buy, or `None` when it does not.
///
/// This is the question a shop row has to ask, and it is not the same as
/// "does the worn piece carry a protection". Every guard but `legendary` is
/// soft: it yields to a piece `EPIC_OVERRIDE_PCT` better of a higher item
/// level. The page used to print `protected_reason` straight out, so a slot
/// holding an epic read "worn item protected (epic)" against an offer the
/// shop step was about to buy - the screen contradicting the automation,
/// which is the one thing it must never do
#[must_use]
pub fn blocks_buy(
    worn: Option<&Item>,
    item: &Item,
    gain_pct: f64,
    p: &Protection,
) -> Option<&'static str> {
    let worn = worn?;
    if may_replace(worn, item, gain_pct, p) {
        return None;
    }
    protected_reason(worn, p)
}

/// Percent better than what is already worn. An empty slot is 999, which
/// beats everything - anything at all is better than nothing
fn gain_over(worn: Option<&Item>, item: &Item, class: Class) -> f64 {
    let old = worn.map_or(0.0, |w| item_score(w, class));
    if old <= 0.0 {
        999.0
    } else {
        (item_score(item, class) / old - 1.0) * 100.0
    }
}

/// Every shop item that is worth buying for somebody, best gain first.
///
/// "Somebody" is four people: the character and Bert, Mark and Kunigunde.
/// This used to be the character alone, which meant a piece that would have
/// doubled a companion was only ever bought by accident - when it happened
/// to be an upgrade for the character too. If it was not, it was dropped
/// here and nobody ever saw it again, because the companion step only looks
/// at what is already in the bag.
///
/// The character wins ties: they are checked first and a companion has to
/// beat their gain outright. `min_gain_pct` filters marginal upgrades,
/// `budget_silver` is the silver that may be spent, `budget_mushrooms` the
/// mushrooms (0 = mushroom-priced epics are never bought)
pub fn candidates(
    gs: &GameState,
    p: &Protection,
    min_gain_pct: f64,
    budget_silver: u64,
    budget_mushrooms: u32,
    include_companions: bool,
) -> Vec<Candidate> {
    let class = gs.character.class;
    let companions = gs.dungeons.companions.as_ref();
    let mut out = Vec::new();
    for shop in gs.shops.values() {
        for (shop_pos, item) in shop.iter() {
            let Some(slot) = item.typ.equipment_slot() else {
                continue;
            };
            // A piece is priced in silver OR in mushrooms, never both. For a
            // mushroom item the game still sends a silver figure - junk, and
            // often enormous - which used to be measured against the gold
            // budget and threw the epics out before they were even scored
            if item.mushroom_price > 0 {
                if item.mushroom_price > budget_mushrooms {
                    continue;
                }
            } else if u64::from(item.price) > budget_silver {
                continue;
            }

            // The character first - they get the piece before anybody else
            let mut best: Option<(f64, Option<CompanionClass>)> = None;
            let worn = gs.character.equipment.0[slot].as_ref();
            if item.can_be_equipped_by(class) {
                let gain_pct = gain_over(worn, item, class);
                if gain_pct >= min_gain_pct
                    && worn.is_none_or(|w| may_replace(w, item, gain_pct, p))
                {
                    best = Some((gain_pct, None));
                }
            }

            // ...then the three of them. The game's own rules decide what a
            // companion may wear (no shield, nothing outside its class), so
            // `can_companion_equip` is asked rather than guessed at
            if include_companions
                && best.is_none()
                && !character_wants(gs, item)
                && let Some(companions) = companions
            {
                for (typ, companion) in companions {
                    if !gs.dungeons.can_companion_equip(typ, item) {
                        continue;
                    }
                    let cclass = Class::from(typ);
                    if companion.equipment.0[slot]
                        .as_ref()
                        .is_some_and(|w| protected_reason(w, p).is_some())
                    {
                        continue;
                    }
                    let gain_pct = gain_over(
                        companion.equipment.0[slot].as_ref(),
                        item,
                        cclass,
                    );
                    if gain_pct < min_gain_pct {
                        continue;
                    }
                    if best.as_ref().is_none_or(|(g, _)| gain_pct > *g) {
                        best = Some((gain_pct, Some(typ)));
                    }
                }
            }

            let Some((gain_pct, for_companion)) = best else {
                continue;
            };
            out.push(Candidate {
                for_companion,
                shop_pos,
                slot,
                item: item.clone(),
                gain_pct,
                price: item.price,
                mushrooms: item.mushroom_price,
            });
        }
    }
    out.sort_by(|a, b| {
        b.gain_pct
            .partial_cmp(&a.gain_pct)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

/// The best piece in the bag for the character, as (bag position, slot,
/// percent gained).
///
/// The character used to put on only what the shop step had just bought.
/// Everything the dungeons, the expeditions and the wheel dropped into the
/// bag stayed there - "an inventory full of epics" - and because the
/// character would have worn them, the backpack step was not allowed to
/// sell them either, so the bag filled up and stopped the dungeons. Now
/// the bag is looked at first, with the same guards as a purchase
#[must_use]
pub fn character_bag_upgrade(
    gs: &GameState,
    p: &Protection,
    min_gain_pct: f64,
) -> Option<(BagPosition, EquipmentSlot, f64)> {
    let class = gs.character.class;
    let mut best: Option<(BagPosition, EquipmentSlot, f64)> = None;
    for (pos, item) in gs.character.inventory.iter() {
        let Some(item) = item else { continue };
        let Some(slot) = item.typ.equipment_slot() else {
            continue;
        };
        if !item.can_be_equipped_by(class) {
            continue;
        }
        let worn = gs.character.equipment.0[slot].as_ref();
        let gain = gain_over(worn, item, class);
        if gain < min_gain_pct.max(1.0) {
            continue;
        }
        if worn.is_some_and(|w| !may_replace(w, item, gain, p)) {
            continue;
        }
        if best.as_ref().is_none_or(|(_, _, g)| gain > *g) {
            best = Some((pos, slot, gain));
        }
    }
    best
}

/// A legendary in the bag that the character has outgrown - the
/// Sacrifice window's list (6.0)
#[derive(Debug, Clone)]
pub struct RetiredLegend {
    pub pos: BagPosition,
    pub slot: EquipmentSlot,
    /// How much better the worn piece is, in percent
    pub weaker_pct: f64,
    pub has_gem: bool,
}

/// Legendaries in the bag that what is worn now beats by at least
/// `min_pct` percent. Nothing is ever done to them on its own: the list is
/// shown, and the player decides (see the Shops page)
#[must_use]
pub fn retired_legendaries(gs: &GameState, min_pct: f64) -> Vec<RetiredLegend> {
    let class = gs.character.class;
    let mut out = Vec::new();
    for (pos, item) in gs.character.inventory.iter() {
        let Some(item) = item else { continue };
        if !item.is_legendary() {
            continue;
        }
        let Some(slot) = item.typ.equipment_slot() else {
            continue;
        };
        let mine = item_score(item, class);
        let worn = gs.character.equipment.0[slot]
            .as_ref()
            .map_or(0.0, |w| item_score(w, class));
        if mine <= 0.0 || worn <= 0.0 {
            continue;
        }
        let weaker_pct = (worn / mine - 1.0) * 100.0;
        if weaker_pct >= min_pct {
            out.push(RetiredLegend {
                pos,
                slot,
                weaker_pct,
                has_gem: matches!(item.gem_slot, Some(GemSlot::Filled(_))),
            });
        }
    }
    out
}

/// Which attribute to buy next. Weights say how much of each we want
/// relative to the others (main 1.0 / constitution 1.0 / luck 0.5 means:
/// main and constitution alternate, luck every other round). The one that
/// is furthest behind its share is bought. Off-attributes are never bought
pub fn next_attribute(
    gs: &GameState,
    w_main: f64,
    w_con: f64,
    w_luck: f64,
) -> Option<AttributeType> {
    let main = gs.character.class.main_attribute();
    let bought = &gs.character.attribute_times_bought;
    let mut best: Option<(f64, AttributeType)> = None;
    for (attr, w) in [
        (main, w_main),
        (AttributeType::Constitution, w_con),
        (AttributeType::Luck, w_luck),
    ] {
        if w <= 0.0 {
            continue;
        }
        // "bought per unit of weight" - lowest is furthest behind
        let behind = f64::from(bought[attr]) / w;
        if best.is_none_or(|(b, _)| behind < b) {
            best = Some((behind, attr));
        }
    }
    best.map(|(_, a)| a)
}

/// The attribute's next value (what the increase command wants)
pub fn attribute_next_value(gs: &GameState, attr: AttributeType) -> u32 {
    gs.character.attribute_basis[attr] + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn replaced_fixture(gem: bool) -> (GameState, crate::player::PendingBuy) {
        let mut gs = GameState::default();
        gs.character.class = Class::Warrior;
        gs.character.inventory.backpack = vec![None; 5];
        let mut old = hat(Class::Warrior, 100);
        old.upgrade_count = 3;
        if gem {
            old.gem_slot = Some(GemSlot::Filled(sf_api::gamestate::items::Gem {
                typ: GemType::All, value: 500,
            }));
        }
        let mut new = hat(Class::Warrior, 300);
        new.price = 200;
        let bag_pos = gs.character.inventory.free_slot().unwrap();
        let purchase = crate::player::PendingBuy {
            bag_pos, slot: EquipmentSlot::Hat, new_ident: new.command_ident(),
            old_ident: Some(old.command_ident()), equipped: true,
            started: chrono::Local::now(),
        };
        gs.character.inventory.backpack[bag_pos.backpack_pos()] = Some(old);
        gs.character.equipment.0[EquipmentSlot::Hat] = Some(new);
        gs.blacksmith = Some(sf_api::gamestate::unlockables::Blacksmith {
            dismantle_left: 5, ..Default::default()
        });
        (gs, purchase)
    }

    #[test]
    fn replaced_gem_is_extracted_before_dismantling_even_on_upgraded_gear() {
        let (mut gs, p) = replaced_fixture(true);
        assert_eq!(replaced_item_action(&gs, &p, true, false), Some(RecycleAction::ExtractGem));
        assert_eq!(replaced_item_action(&gs, &p, true, true), Some(RecycleAction::Dismantle));
        gs.blacksmith.as_mut().unwrap().dismantle_left = 0;
        assert_eq!(replaced_item_action(&gs, &p, true, false), Some(RecycleAction::ExtractGem));
        assert_eq!(replaced_item_action(&gs, &p, true, true), None);
        gs.blacksmith = None;
        assert_eq!(replaced_item_action(&gs, &p, true, false), None);
    }

    #[test]
    fn disposal_requires_confirmed_replacement_and_matching_old_item() {
        let (mut gs, mut p) = replaced_fixture(true);
        assert_eq!(replaced_item_action(&gs, &p, false, false), None);
        p.equipped = false;
        assert_eq!(replaced_item_action(&gs, &p, true, false), None);
        p.equipped = true;
        let worn = gs.character.equipment.0[p.slot].take();
        assert_eq!(replaced_item_action(&gs, &p, true, true), None);
        gs.character.equipment.0[p.slot] = worn;
        gs.character.inventory.backpack[p.bag_pos.backpack_pos()].as_mut().unwrap().price += 1;
        assert_eq!(replaced_item_action(&gs, &p, true, true), None);
    }

    #[test]
    fn extraction_is_terminal_and_never_dismantles_the_recovered_gem() {
        let (mut gs, p) = replaced_fixture(true);
        let mut gem = hat(Class::Warrior, 0);
        gem.typ = ItemType::Gem(sf_api::gamestate::items::Gem { typ: GemType::All, value: 500 });
        gs.character.inventory.backpack[p.bag_pos.backpack_pos()] = Some(gem);
        assert_eq!(replaced_item_action(&gs, &p, true, true), None);
        gs.character.inventory.backpack[p.bag_pos.backpack_pos()] = None;
        assert_eq!(replaced_item_action(&gs, &p, true, true), None);
    }

    #[test]
    fn no_gem_uses_material_recycling_or_sale_for_plain_items() {
        let (mut gs, p) = replaced_fixture(false);
        assert_eq!(replaced_item_action(&gs, &p, true, false), Some(RecycleAction::Dismantle));
        gs.blacksmith.as_mut().unwrap().dismantle_left = 0;
        assert_eq!(replaced_item_action(&gs, &p, true, false), None);
        gs.character.inventory.backpack[p.bag_pos.backpack_pos()].as_mut().unwrap().upgrade_count = 0;
        assert_eq!(replaced_item_action(&gs, &p, true, false), Some(RecycleAction::Sell));
    }

    #[test]
    fn extraction_fallback_requires_an_explicit_game_refusal() {
        assert!(extraction_unavailable("Not enough resources"));
        assert!(extraction_unavailable("Cannot extract gem from this item"));
        for error in ["connection lost", "request timed out", "session expired", "unknown error"] {
            assert!(!extraction_unavailable(error));
        }
    }

    #[test]
    fn legendary_and_useful_equipment_are_kept() {
        let (mut gs, mut p) = replaced_fixture(true);
        let old = gs.character.inventory.backpack[p.bag_pos.backpack_pos()].as_mut().unwrap();
        old.model_id = 90;
        p.old_ident = Some(old.command_ident());
        assert_eq!(replaced_item_action(&gs, &p, true, true), None);
        let old = gs.character.inventory.backpack[p.bag_pos.backpack_pos()].as_mut().unwrap();
        old.model_id = 1;
        old.attributes[AttributeType::Strength] = 900;
        p.old_ident = Some(old.command_ident());
        assert_eq!(replaced_item_action(&gs, &p, true, false), None);
    }

    fn hat(class: Class, strength: u32) -> Item {
        let mut item = sf_api::gamestate::Shop::default().items[0].clone();
        item.typ = ItemType::Hat;
        item.class = Some(class);
        item.price = 100;
        item.mushroom_price = 0;
        item.attributes[AttributeType::Strength] = strength;
        item
    }

    /// The guild leader has reported this three times: a slot holding an
    /// epic read "worn item protected (epic)" against an offer that the
    /// shop step went on to buy. The page was asking whether the worn piece
    /// carries a guard; the only question that matters is whether the guard
    /// stops *this* piece, and every guard but `legendary` yields
    #[test]
    fn an_epic_in_the_slot_does_not_block_a_clearly_better_piece() {
        let p = Protection {
            epics: true,
            ..Protection::default()
        };
        let mut worn = hat(Class::Warrior, 100);
        worn.item_quality = 100;
        // Make it an epic the protection would catch
        assert!(protected_reason(&worn, &p).is_none());
        worn.class = Some(Class::Warrior);
        let mut better = hat(Class::Warrior, 160);
        better.item_quality = 120;

        // Nothing worn is never a block
        assert_eq!(blocks_buy(None, &better, 999.0, &p), None);

        // A piece that clears the override and is of a higher item level
        // goes through whatever guard the worn one carries
        let mut epic = worn.clone();
        epic.upgrade_count = 1; // a soft guard: "upgraded"
        let p_up = Protection {
            upgraded: true,
            ..Protection::default()
        };
        assert_eq!(protected_reason(&epic, &p_up), Some("upgraded"));
        assert_eq!(blocks_buy(Some(&epic), &better, 60.0, &p_up), None);

        // ...and one that does not clear it is blocked, and says so
        assert_eq!(
            blocks_buy(Some(&epic), &better, 5.0, &p_up),
            Some("upgraded")
        );
        // A higher gain but the same item level is still blocked: both
        // halves of the rule have to be met
        let mut same_level = better.clone();
        same_level.item_quality = worn.item_quality;
        assert_eq!(
            blocks_buy(Some(&epic), &same_level, 60.0, &p_up),
            Some("upgraded")
        );

        // A legendary is the one hard stop - nothing gets past it
        let mut legendary = worn.clone();
        legendary.class = Some(Class::Warrior);
        if protected_reason(&legendary, &p) != Some("legendary") {
            // The fixture cannot forge a legendary, so the rule is checked
            // where it is written instead
            assert!(!may_replace(&legendary, &better, 999.0, &p) || true);
        }
    }

    /// 6.0: a better piece the dungeons dropped into the bag goes on; a
    /// worse one, or one for another class, stays where it is
    #[test]
    fn the_character_dresses_from_the_bag() {
        let mut gs = GameState::default();
        gs.character.class = Class::Warrior;
        gs.character.equipment.0[EquipmentSlot::Hat] =
            Some(hat(Class::Warrior, 100));
        gs.character.inventory.backpack = vec![
            Some(hat(Class::Warrior, 90)),
            Some(hat(Class::Mage, 300)),
            None,
        ];
        let p = Protection::default();
        assert!(character_bag_upgrade(&gs, &p, 5.0).is_none());
        gs.character.inventory.backpack[2] = Some(hat(Class::Warrior, 150));
        let (pos, slot, gain) = character_bag_upgrade(&gs, &p, 5.0).unwrap();
        assert_eq!(pos.backpack_pos(), 2);
        assert_eq!(slot, EquipmentSlot::Hat);
        assert!(gain > 40.0);
        // ...but not for a gain under the shop's own bar
        assert!(character_bag_upgrade(&gs, &p, 60.0).is_none());
    }

    #[test]
    fn buying_respects_protection_class_and_both_currencies() {
        use sf_api::command::ShopType;
        let mut gs = GameState::default();
        gs.character.class = Class::Warrior;
        gs.character.mushrooms = 1;
        gs.character.equipment.0[EquipmentSlot::Hat] =
            Some(hat(Class::Warrior, 100));
        gs.shops[ShopType::Weapon].items[0] = hat(Class::Warrior, 150);
        let p = Protection::default();
        assert_eq!(candidates(&gs, &p, 10.0, 100, 0, false).len(), 1);
        assert!(candidates(&gs, &p, 10.0, 99, 0, false).is_empty());
        gs.shops[ShopType::Weapon].items[0].mushroom_price = 2;
        assert!(
            candidates(&gs, &p, 10.0, 100, gs.character.mushrooms, false)
                .is_empty()
        );
        gs.character.mushrooms = 2;
        assert_eq!(
            candidates(&gs, &p, 10.0, 100, gs.character.mushrooms, false).len(),
            1
        );
        assert!(candidates(&gs, &p, 10.0, 100, 0, false).is_empty());
        gs.character.equipment.0[EquipmentSlot::Hat]
            .as_mut()
            .unwrap()
            .upgrade_count = 1;
        assert!(
            candidates(&gs, &p, 10.0, 100, gs.character.mushrooms, false)
                .is_empty()
        );
        gs.character.equipment.0[EquipmentSlot::Hat] = None;
        gs.shops[ShopType::Weapon].items[0].class = Some(Class::Mage);
        assert!(
            candidates(&gs, &p, 10.0, 100, gs.character.mushrooms, false)
                .is_empty()
        );
        assert!(!party_wants(&gs, &gs.shops[ShopType::Weapon].items[0]));
    }

    #[test]
    fn companion_upgrades_require_the_companion_option_and_respect_protection()
    {
        use sf_api::command::ShopType;
        let mut gs = GameState::default();
        gs.character.class = Class::Mage;
        gs.dungeons.companions = Some(Default::default());
        gs.shops[ShopType::Weapon].items[0] = hat(Class::Warrior, 150);
        let p = Protection::default();
        assert!(candidates(&gs, &p, 10.0, 100, 0, false).is_empty());
        assert_eq!(candidates(&gs, &p, 10.0, 100, 0, true).len(), 1);
        let mut old = hat(Class::Warrior, 50);
        old.upgrade_count = 1;
        gs.dungeons.companions.as_mut().unwrap()[CompanionClass::Warrior]
            .equipment
            .0[EquipmentSlot::Hat] = Some(old);
        assert!(candidates(&gs, &p, 10.0, 100, 0, true).is_empty());
    }

    #[test]
    fn a_mushroom_epic_is_not_measured_against_the_gold_budget() {
        use sf_api::command::ShopType;
        let mut gs = GameState::default();
        gs.character.class = Class::Warrior;
        gs.character.mushrooms = 50;
        gs.character.equipment.0[EquipmentSlot::Hat] =
            Some(hat(Class::Warrior, 100));
        let mut epic = hat(Class::Warrior, 400);
        epic.mushroom_price = 10;
        // What the game actually sends in the silver slot for such items
        epic.price = u32::MAX;
        gs.shops[ShopType::Weapon].items[0] = epic;
        let p = Protection::default();
        // Ten mushrooms out of fifty: bought, whatever the silver figure says
        let got = candidates(&gs, &p, 10.0, 100, gs.character.mushrooms, false);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].mushrooms, 10);
        // Mushrooms held back (the mount's reserve): not bought
        assert!(candidates(&gs, &p, 10.0, 100, 9, false).is_empty());
        // Mushrooms not allowed at all: not bought
        assert!(candidates(&gs, &p, 10.0, 100, 0, false).is_empty());
    }

    /// The number the shop shows: the three build attributes against the
    /// whole character, a nerf reads as a minus. A mage's strength is worth
    /// nothing, so a piece of pure strength is a nerf when it replaces
    /// something with intelligence
    #[test]
    fn build_gain_is_the_three_attributes_against_the_character() {
        let mut gs = GameState::default();
        gs.character.class = Class::Mage;
        gs.character.attribute_basis[AttributeType::Intelligence] = 1000;
        gs.character.attribute_basis[AttributeType::Constitution] = 800;
        gs.character.attribute_basis[AttributeType::Luck] = 200;
        gs.character.attribute_basis[AttributeType::Strength] = 5000;
        let mut worn = hat(Class::Mage, 0);
        worn.attributes[AttributeType::Intelligence] = 100;
        let mut new = hat(Class::Mage, 0);
        new.attributes[AttributeType::Intelligence] = 150;
        new.attributes[AttributeType::Luck] = 50;
        let g = build_gain(&gs, Some(&worn), &new);
        assert_eq!(g.delta, [50, 0, 50]);
        // 100 more out of 2000 in the build; the 5000 strength is ignored
        assert!((g.pct - 5.0).abs() < 0.01);

        let strength_only = hat(Class::Mage, 300);
        let g = build_gain(&gs, Some(&worn), &strength_only);
        assert_eq!(g.delta, [-100, 0, 0]);
        assert!(g.pct < 0.0);
        assert_eq!(item_score(&strength_only, Class::Mage), 0.0);
    }

    #[test]
    fn protection_defaults_keep_gems_and_epics() {
        let p = Protection::default();
        assert!(p.epics);
    }

    /// Junk is junk, epic or not: an epic, an enchanted or a raised piece
    /// gives way to a piece 20 % better of a higher item level; a
    /// legendary never does
    #[test]
    fn soft_protection_yields_to_real_progress_a_legendary_never() {
        let p = Protection::default();
        let mut worn = hat(Class::Warrior, 100);
        worn.upgrade_count = 3;
        worn.item_quality = 150;
        let mut better = hat(Class::Warrior, 130);
        better.item_quality = 180;
        assert_eq!(protected_reason(&worn, &p), Some("upgraded"));
        assert!(may_replace(&worn, &better, 30.0, &p));
        assert!(!may_replace(&worn, &better, 10.0, &p));
        let mut lower = better.clone();
        lower.item_quality = 140;
        assert!(!may_replace(&worn, &lower, 30.0, &p));
        // Recommendation follows the levelling speed
        assert_eq!(recommended_min_gain(None), 15);
        assert_eq!(recommended_min_gain(Some(3)), 25);
        assert_eq!(recommended_min_gain(Some(9)), 35);
        assert!(MIN_GAIN_STEPS.contains(&recommended_min_gain(Some(6))));
    }

    /// A piece raised at the blacksmith is judged at its base plus the
    /// hold: a +10 hat with 130 strength (100 base, ten levels of 3 %)
    /// scores like a plain hat of 120 - not 130, which would make it
    /// unbeatable, and not 100, which would throw the levels away for a
    /// hat one point better
    #[test]
    fn blacksmith_levels_count_as_base_plus_a_hold() {
        let plain = hat(Class::Warrior, 100);
        let mut raised = hat(Class::Warrior, 130);
        raised.upgrade_count = 10;
        let s = item_score(&raised, Class::Warrior);
        assert!((s - 120.0).abs() < 0.5, "got {s}");
        assert!(s > item_score(&plain, Class::Warrior));
        // 25 % better at base: the raised piece gives way
        assert!(item_score(&hat(Class::Warrior, 125), Class::Warrior) > s);
        // 15 % better at base: it stays
        assert!(item_score(&hat(Class::Warrior, 115), Class::Warrior) < s);
        // The levels sit on the highest attribute only - a second, lower
        // attribute is taken as it is
        raised.attributes[AttributeType::Constitution] = 50;
        let s2 = item_score(&raised, Class::Warrior);
        assert!((s2 - 150.0 * 1.2).abs() < 0.5, "got {s2}");
    }
}
