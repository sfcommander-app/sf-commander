//! Pet fights, worked out before they are fought.
//!
//! A pet is nothing but its habitat, its number in that habitat, its level
//! and the pack behind it: the game builds the whole fighter out of those
//! four things, and so does this. The arithmetic is SF Tools' pet
//! simulator (`js/sim/pets.js`, HafisCZ) written out in Rust, and the fight
//! itself is the library's battle simulation, which is a port of the same
//! thing. So the odds here are the odds SF Tools gives - that was the
//! point of asking for it.
//!
//! Two questions get answered with it. In a habitat: which of our pets to
//! send against the next boss, and how it would go. In the pet arena: how
//! our strongest pet of a habitat does against the opponent's, whose level
//! is typed in because the game does not hand it over.

use sf_api::{
    command::AttributeType,
    gamestate::{
        character::Class,
        unlockables::{HabitatType, Pet, PetStats, Pets},
    },
    simulate::{Fighter, Monster, simulate_battle},
};

/// Pets per habitat
pub const PER_HABITAT: usize = 20;

/// How many fights decide a figure that is shown. Ten thousand puts the
/// noise under half a percent
pub const ITERATIONS: u32 = 10_000;

/// How many fights decide which pet is best. Coarser: this runs for every
/// pet of a habitat, and the choice only has to be right, not precise
pub const PICK_ITERATIONS: u32 = 2_000;

/// The main stat of a pet per level, by its number in the habitat. The
/// twentieth is no stronger than the nineteenth
const FACTOR: [f64; PER_HABITAT] = [
    10.0, 11.0, 12.0, 13.0, 14.0, 16.0, 18.0, 20.0, 25.0, 30.0, 35.0, 40.0,
    50.0, 60.0, 70.0, 80.0, 100.0, 130.0, 160.0, 160.0,
];

/// Luck per level, three quarters of the main stat
const FACTOR_LUCK: [f64; PER_HABITAT] = [
    7.5, 8.5, 9.0, 9.5, 10.5, 12.0, 13.5, 15.0, 19.0, 22.5, 26.0, 30.0, 37.5,
    45.0, 52.5, 60.0, 75.0, 97.5, 120.0, 120.0,
];

/// The level of each habitat boss, by its number in the habitat
pub const BOSS_LEVEL: [u16; PER_HABITAT] = [
    1, 3, 6, 10, 14, 18, 22, 26, 30, 34, 38, 42, 46, 50, 54, 58, 62, 66, 70,
    75,
];

/// The class of every pet: Shadow, Light, Earth, Fire, Water, twenty each.
/// W = warrior, M = mage, S = scout
#[rustfmt::skip]
const CLASS: [[u8; PER_HABITAT]; 5] = [
    [b'S',b'W',b'W',b'M',b'M', b'M',b'S',b'S',b'S',b'W', b'M',b'M',b'S',b'S',b'W', b'W',b'M',b'W',b'W',b'S'],
    [b'W',b'W',b'M',b'M',b'S', b'S',b'M',b'W',b'W',b'M', b'M',b'S',b'S',b'M',b'M', b'W',b'W',b'W',b'M',b'S'],
    [b'W',b'W',b'S',b'S',b'W', b'S',b'M',b'M',b'W',b'W', b'S',b'W',b'S',b'S',b'M', b'M',b'M',b'W',b'W',b'W'],
    [b'S',b'S',b'W',b'M',b'M', b'S',b'S',b'M',b'W',b'M', b'M',b'S',b'S',b'S',b'S', b'S',b'M',b'W',b'M',b'W'],
    [b'M',b'W',b'W',b'W',b'W', b'S',b'W',b'S',b'S',b'W', b'M',b'M',b'M',b'W',b'M', b'M',b'W',b'M',b'W',b'S'],
];

/// The pets' names, the same order. The boss of a habitat stage is the pet
/// won there, so the one table names both
#[rustfmt::skip]
const NAME: [[&str; PER_HABITAT]; 5] = [
    ["Schlorps", "Buddemull", "Zähnaug", "Okulontakel", "Spynn", "Jackobu", "Shrimpfly", "Schnigrim", "Rubdikaz", "Mykon", "Anglion", "Kucklitz", "Flederadatz", "Rosalynx", "Pharamum", "Nynnja", "Luchtablong", "Zornox", "Deiblatz", "Phoison"],
    ["Zossl", "Quaklops", "Tinck", "Witteblitz", "Niesat", "Plutoid", "Dschinntonik", "Blaxta", "Plinze", "Tezzler", "Fusonn", "Bofux", "Tiwitsch", "Dafuk", "Mesmerit", "Antlar", "Uuf", "Kylrich", "Heraldon", "Unikor"],
    ["Mamoton", "Rängatäng", "Lilipon", "Naghezan", "Groar", "Muscudon", "Apstock", "Rambok", "Urrf", "Mameloth", "Nahon", "Stonor", "Rofus", "Kloppdiwop", "Waltschrek", "Scheer", "Tauerbok", "Kanockle", "Tricerabor", "Mauhel"],
    ["Ratzfratz", "Gullps", "Pyrophibus", "Loderfleuch", "Tektospei", "Pyroplant", "Kokofö", "Jappaelo", "Zischbum", "Tikkon", "Fachan", "Feugel", "Infernox", "Sumsum", "Dragopyr", "Mantiflam", "Flossnessel", "Rocki", "Zyngel", "Devastor"],
    ["Goldi", "Orkal", "Okodil", "Püng", "Walruphin", "Schnögg", "Aguaphant", "Naar", "Tinntak", "Ewilgryn", "Separd", "Mussli", "Mingho", "Untabis", "Nixoid", "Fluoch", "Patörtel", "Unnda", "Tritostach", "Hydrospir"],
];

/// All five, in the game's order
pub const HABITATS: [HabitatType; 5] = [
    HabitatType::Shadow,
    HabitatType::Light,
    HabitatType::Earth,
    HabitatType::Fire,
    HabitatType::Water,
];

/// The name of pet `index` (0-based) of a habitat
#[must_use]
pub fn name(habitat: HabitatType, index: usize) -> &'static str {
    NAME[habitat as usize][index.min(PER_HABITAT - 1)]
}

/// The class of pet `index` (0-based) of a habitat
#[must_use]
pub fn class_of(habitat: HabitatType, index: usize) -> Class {
    match CLASS[habitat as usize][index.min(PER_HABITAT - 1)] {
        b'M' => Class::Mage,
        b'S' => Class::Scout,
        _ => Class::Warrior,
    }
}

/// A pet's number in its habitat, 0-based, from the global id the game
/// gives it (1..=100, twenty per habitat)
#[must_use]
pub fn index_of(pet: &Pet) -> usize {
    (pet.id.saturating_sub(1) as usize) % PER_HABITAT
}

/// What stands behind a pet: how many of its habitat are owned, and how
/// many of those are at 100, 150 and 200. Every one of them makes every
/// pet of the habitat stronger
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Pack {
    pub owned: u32,
    pub at100: u32,
    pub at150: u32,
    pub at200: u32,
}

impl Pack {
    /// Counted off the habitat as the game reports it
    #[must_use]
    pub fn of(pets: &Pets, habitat: HabitatType) -> Self {
        let mut p = Pack::default();
        for pet in &pets.habitats[habitat].pets {
            match pet.level {
                0 => {}
                1..100 => p.owned += 1,
                100..150 => {
                    p.owned += 1;
                    p.at100 += 1;
                }
                150..200 => {
                    p.owned += 1;
                    p.at150 += 1;
                }
                _ => {
                    p.owned += 1;
                    p.at200 += 1;
                }
            }
        }
        p
    }

    /// The bonus in percent on every stat: five per pet owned, plus half
    /// of that per pet at 100, three quarters per pet at 150, the whole
    /// again per pet at 200 - the fractions added up and rounded down
    /// before the five is applied, the way the game does it
    #[must_use]
    pub fn bonus_pct(self) -> u32 {
        let sum = f64::from(self.owned)
            + f64::from(self.at200)
            + f64::from(self.at150) * 0.75
            + f64::from(self.at100) * 0.5;
        5 * sum.trunc() as u32
    }
}

/// Everything a pet fighter is made of
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PetSpec {
    pub habitat: HabitatType,
    /// 0-based number in the habitat
    pub index: usize,
    pub level: u16,
    /// The pack bonus in percent
    pub bonus_pct: u32,
    /// The gladiator trainer level, 0..=15
    pub gladiator: u32,
    /// A habitat boss. Bosses do not have the type advantage, and neither
    /// does anything fighting one
    pub boss: bool,
}

/// The stats a spec works out to - what the game's pet screen shows
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PetStatsLine {
    pub class: Class,
    pub hp: u64,
    pub main: u32,
    pub luck: u32,
    pub armor: u32,
    pub damage: u32,
}

impl PetSpec {
    /// One of ours, as it stands right now
    #[must_use]
    pub fn own(pets: &Pets, pet: &Pet, gladiator: u32) -> Self {
        PetSpec {
            habitat: pet.element,
            index: index_of(pet),
            level: pet.level,
            bonus_pct: Pack::of(pets, pet.element).bonus_pct(),
            gladiator,
            boss: false,
        }
    }

    /// The boss at stage `index` (0-based) of a habitat. Its level is the
    /// one the game reports for the next fight when it does, the table's
    /// otherwise; its pack bonus is five percent per stage, starting at
    /// five
    #[must_use]
    pub fn boss(habitat: HabitatType, index: usize, level: Option<u16>) -> Self {
        let index = index.min(PER_HABITAT - 1);
        PetSpec {
            habitat,
            index,
            level: level.unwrap_or(BOSS_LEVEL[index]),
            bonus_pct: 5 + 5 * index as u32,
            gladiator: 0,
            boss: true,
        }
    }

    #[must_use]
    pub fn name(&self) -> &'static str {
        name(self.habitat, self.index)
    }

    #[must_use]
    pub fn class(&self) -> Class {
        class_of(self.habitat, self.index)
    }

    /// The stats: the main attribute is the factor times level plus one,
    /// times the pack bonus, rounded down; the two others are half of it,
    /// constitution the same as it, luck three quarters. Armour is the
    /// level times the class's cap, the weapon hits for level plus one
    /// times the class's multiplier, every time
    #[must_use]
    pub fn stats(&self) -> PetStatsLine {
        let class = self.class();
        let multiplier =
            (f64::from(self.level) + 1.0) * (1.0 + f64::from(self.bonus_pct) / 100.0);
        let main = (FACTOR[self.index] * multiplier).trunc() as u32;
        let luck = (FACTOR_LUCK[self.index] * multiplier).trunc() as u32;
        // Hit points per point of constitution and level: five for a
        // warrior, four for a scout, two for a mage
        let per_con: u64 = match class {
            Class::Warrior => 5,
            Class::Mage => 2,
            _ => 4,
        };
        let hp = u64::from(main) * per_con * (u64::from(self.level) + 1);
        let damage = ((f64::from(self.level) + 1.0) * class.weapon_multiplier())
            .trunc() as u32;
        PetStatsLine {
            class,
            hp,
            main,
            luck,
            armor: u32::from(self.level) * class.max_armor_reduction(),
            damage,
        }
    }

    /// Whether this pet's type beats the other's: shadow beats water,
    /// light beats shadow, earth beats light, fire beats earth, water beats
    /// fire. A quarter more damage - never against or for a boss
    #[must_use]
    pub fn has_advantage(&self, other: &PetSpec) -> bool {
        use HabitatType as H;
        if self.boss || other.boss {
            return false;
        }
        matches!(
            (self.habitat, other.habitat),
            (H::Shadow, H::Water)
                | (H::Light, H::Shadow)
                | (H::Earth, H::Light)
                | (H::Fire, H::Earth)
                | (H::Water, H::Fire)
        )
    }

    /// The fighter for the battle simulation, with the type advantage
    /// against `other` folded in as a damage bonus
    #[must_use]
    pub fn fighter(&self, other: &PetSpec) -> Fighter {
        let s = self.stats();
        let half = s.main / 2;
        // The map type's crate is not a dependency of ours; an empty one
        // is borrowed off a default pet-stats record instead
        let mut attributes = PetStats::default().attributes;
        attributes[AttributeType::Strength] = half;
        attributes[AttributeType::Dexterity] = half;
        attributes[AttributeType::Intelligence] = half;
        attributes[s.class.main_attribute()] = s.main;
        attributes[AttributeType::Constitution] = s.main;
        attributes[AttributeType::Luck] = s.luck;
        let monster = Monster {
            name: self.name(),
            level: self.level,
            class: s.class,
            attributes,
            hp: s.hp,
            min_dmg: s.damage,
            max_dmg: s.damage,
            armor: s.armor,
            runes: None,
        };
        let mut f = Fighter::from(&monster);
        f.crit_dmg_multi = 2.0 + f64::from(self.gladiator) * 0.11;
        f.gladiator_lvl = self.gladiator;
        if self.has_advantage(other) {
            f.portal_dmg_bonus = 25.0;
        }
        f
    }
}

/// How often `us` beats `them`, 0.0..=1.0
#[must_use]
pub fn odds(us: &PetSpec, them: &PetSpec, iterations: u32) -> f64 {
    let a = us.fighter(them);
    let b = them.fighter(us);
    simulate_battle(&[a], &[b], iterations, false).win_ratio
}

/// One of our pets against something, with the answer
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Matchup {
    pub pet_id: u32,
    pub ours: PetSpec,
    pub theirs: PetSpec,
    /// 0.0..=1.0
    pub odds: f64,
}

/// Every owned pet of the habitat against `them`, best first. The choice
/// is made on the coarse count; the winner is then re-run on the fine
/// one so the figure shown is a figure
#[must_use]
pub fn ranked(
    pets: &Pets,
    habitat: HabitatType,
    gladiator: u32,
    them: &PetSpec,
) -> Vec<Matchup> {
    let mut out: Vec<Matchup> = pets.habitats[habitat]
        .pets
        .iter()
        .filter(|p| p.level > 0)
        .map(|p| {
            let ours = PetSpec::own(pets, p, gladiator);
            Matchup {
                pet_id: p.id,
                ours,
                theirs: *them,
                odds: odds(&ours, them, PICK_ITERATIONS),
            }
        })
        .collect();
    out.sort_by(|a, b| {
        b.odds
            .partial_cmp(&a.odds)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b.ours.level.cmp(&a.ours.level))
    });
    if let Some(best) = out.first_mut() {
        best.odds = odds(&best.ours, them, ITERATIONS);
    }
    out
}

/// The strongest owned pet of a habitat - the one the game sends into the
/// pet arena
#[must_use]
pub fn strongest(pets: &Pets, habitat: HabitatType) -> Option<&Pet> {
    pets.habitats[habitat]
        .pets
        .iter()
        .filter(|p| p.level > 0)
        .max_by_key(|p| (p.level, p.id))
}

/// The whole page's worth: every habitat's next boss with our pets ranked
/// against it, and the arena opponent as typed
#[must_use]
pub fn report(
    pets: &Pets,
    gladiator: u32,
    input: &crate::player::PetSimInput,
) -> crate::player::PetSimReport {
    use sf_api::gamestate::unlockables::HabitatExploration;
    let bosses = HABITATS
        .iter()
        .map(|&habitat| {
            let info = &pets.habitats[habitat];
            let boss = match info.exploration {
                HabitatExploration::Finished => None,
                HabitatExploration::Exploring {
                    fights_won,
                    next_fight_lvl,
                } => Some(PetSpec::boss(
                    habitat,
                    fights_won as usize,
                    (next_fight_lvl > 0).then_some(next_fight_lvl),
                )),
            };
            let ranked = boss
                .map(|b| ranked(pets, habitat, gladiator, &b))
                .unwrap_or_default();
            (habitat, boss, ranked)
        })
        .collect();
    let theirs = input.theirs();
    let arena = strongest(pets, input.habitat).map(|p| {
        let ours = PetSpec::own(pets, p, gladiator);
        Matchup {
            pet_id: p.id,
            ours,
            theirs,
            odds: odds(&ours, &theirs, ITERATIONS),
        }
    });
    crate::player::PetSimReport {
        bosses,
        arena,
        gladiator,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bosses of the first two water stages, as SF Tools' data file
    /// has them: Goldi, level 1, a mage with 21 in the main stat, 15 luck
    /// and 84 hit points; Orkal, level 3, a warrior with 48, 37 and 960
    #[test]
    fn the_bosses_come_out_as_the_data_file_says() {
        let goldi = PetSpec::boss(HabitatType::Water, 0, None).stats();
        assert_eq!(goldi.class, Class::Mage);
        assert_eq!((goldi.main, goldi.luck, goldi.hp), (21, 15, 84));
        let orkal = PetSpec::boss(HabitatType::Water, 1, None).stats();
        assert_eq!(orkal.class, Class::Warrior);
        assert_eq!((orkal.main, orkal.luck, orkal.hp), (48, 37, 960));
        assert_eq!(PetSpec::boss(HabitatType::Water, 1, None).name(), "Orkal");
        assert_eq!(PetSpec::boss(HabitatType::Shadow, 19, None).level, 75);
    }

    /// Twenty pets, ten of them at 100, five at 150, two at 200: five
    /// times the rounded-down sum
    #[test]
    fn the_pack_bonus_rounds_the_fractions_down_first() {
        let p = Pack { owned: 20, at100: 10, at150: 5, at200: 2 };
        // 20 + 2 + 3.75 + 5 = 30.75 -> 30 -> 150 %
        assert_eq!(p.bonus_pct(), 150);
        assert_eq!(Pack { owned: 3, at100: 1, ..Default::default() }.bonus_pct(), 15);
        assert_eq!(Pack::default().bonus_pct(), 0);
    }

    /// A pet far above the boss wins nearly always, far below nearly
    /// never, and the same pet against itself is a coin toss
    #[test]
    fn the_odds_go_the_way_they_must() {
        let boss = PetSpec::boss(HabitatType::Water, 5, None); // lvl 18
        let strong = PetSpec {
            habitat: HabitatType::Water,
            index: 5,
            level: 60,
            bonus_pct: 50,
            gladiator: 5,
            boss: false,
        };
        let weak = PetSpec { level: 5, bonus_pct: 5, gladiator: 0, ..strong };
        assert!(odds(&strong, &boss, 2000) > 0.97);
        assert!(odds(&weak, &boss, 2000) < 0.03);
        let same = PetSpec { boss: false, ..boss };
        let even = odds(&same, &same, 4000);
        assert!((0.42..0.58).contains(&even), "{even}");
    }

    /// Water beats fire by a quarter more damage - and only between pets,
    /// never with a boss in the fight
    #[test]
    fn the_type_advantage_is_a_damage_bonus_between_pets_only() {
        let water = PetSpec {
            habitat: HabitatType::Water,
            index: 3,
            level: 30,
            bonus_pct: 20,
            gladiator: 0,
            boss: false,
        };
        let fire = PetSpec { habitat: HabitatType::Fire, ..water };
        assert!(water.has_advantage(&fire));
        assert!(!fire.has_advantage(&water));
        assert_eq!(water.fighter(&fire).portal_dmg_bonus, 25.0);
        assert_eq!(fire.fighter(&water).portal_dmg_bonus, 0.0);
        let boss = PetSpec { boss: true, ..fire };
        assert!(!water.has_advantage(&boss));
    }

    #[test]
    fn the_index_comes_off_the_global_id() {
        let mut p = Pet::default();
        p.id = 1;
        assert_eq!(index_of(&p), 0);
        p.id = 20;
        assert_eq!(index_of(&p), 19);
        p.id = 21;
        assert_eq!(index_of(&p), 0);
        p.id = 100;
        assert_eq!(index_of(&p), 19);
    }
}

#[cfg(test)]
mod timing {
    use super::*;

    /// How long a whole page's worth takes - it runs on the UI thread
    #[test]
    #[ignore]
    fn a_report_is_quick() {
        let mut pets = Pets::default();
        for (index, (element, habitat)) in pets.habitats.iter_mut().enumerate() {
            for (i, pet) in habitat.pets.iter_mut().enumerate() {
                pet.id = (index * 20 + i + 1) as u32;
                pet.element = element;
                pet.level = (6 + i * 4) as u16;
            }
            habitat.exploration =
                sf_api::gamestate::unlockables::HabitatExploration::Exploring {
                    fights_won: 14,
                    next_fight_lvl: 54,
                };
        }
        let input = crate::player::PetSimInput::default();
        let t = std::time::Instant::now();
        let r = report(&pets, 8, &input);
        eprintln!("report: {:?} ({} habitats)", t.elapsed(), r.bosses.len());
    }
}
