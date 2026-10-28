use crate::i18n::t;
use iced::{
    Alignment, Element, Length, theme,
    widget::{
        button, column, container, pick_list, row,
        scrollable, text_input,
    },
};
use super::glyphs::ticked as checkbox;
// His lettering stands in for the toolkit's `text`, so every line on
// this screen is set in the face the purse card is
use super::glyphs::lettered as text;
use sf_api::gamestate::unlockables::HabitatType;

use crate::{
    message::{Message, PetSimField},
    pet_sim::{self, Matchup, PetSpec},
    player::{AccountInfo, AccountStatus, PetSimReport},
    ui::style,
    util::LockExt,
};
use sf_api::gamestate::character::Class;

/// A habitat in a drop-down: the enum has no name of its own
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HabitatPick(pub HabitatType);

impl std::fmt::Display for HabitatPick {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(habitat_name(self.0))
    }
}

const HABITAT_PICKS: [HabitatPick; 5] = [
    HabitatPick(HabitatType::Shadow),
    HabitatPick(HabitatType::Light),
    HabitatPick(HabitatType::Earth),
    HabitatPick(HabitatType::Fire),
    HabitatPick(HabitatType::Water),
];

/// One of the twenty pets of a habitat in a drop-down: "3 Okodil"
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PetPick {
    pub index: usize,
    pub name: &'static str,
}

impl std::fmt::Display for PetPick {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.index + 1, self.name)
    }
}

fn pet_picks(habitat: HabitatType) -> Vec<PetPick> {
    (0..pet_sim::PER_HABITAT)
        .map(|index| PetPick {
            index,
            name: pet_sim::name(habitat, index),
        })
        .collect()
}

fn class_word(class: Class) -> &'static str {
    match class {
        Class::Mage => t("mage"),
        Class::Scout => t("scout"),
        _ => t("warrior"),
    }
}

/// Green when it is a fight to take, gold when it is a coin toss, red
/// when it is a loss
fn odds_colour(odds: f64) -> iced::Color {
    if odds >= 0.75 {
        style::GOOD
    } else if odds >= 0.5 {
        style::WARN
    } else {
        style::BAD
    }
}

fn pct(odds: f64) -> String {
    format!("~{:.0} %", odds * 100.0)
}

/// "Okodil lvl 25" - a pet the way it is talked about
fn pet_word(spec: &PetSpec) -> String {
    format!("{} lvl {}", spec.name(), spec.level)
}

/// Every habitat's next boss and which of our pets to send. The whole
/// point of the simulator: the fight is fought here first. Columns, so
/// the five rows read as a table: habitat, the boss, who to send, the
/// odds, and the two runners-up in grey
fn boss_rows<'a>(report: &PetSimReport) -> Element<'a, Message> {
    const W_HABITAT: f32 = 60.0;
    const W_BOSS: f32 = 270.0;
    const W_SEND: f32 = 210.0;
    const W_ODDS: f32 = 58.0;
    let mut rows = column!().spacing(5).width(Length::Fill);
    for (habitat, boss, ranked) in &report.bosses {
        let head = text(habitat_name(*habitat))
            .size(12)
            .width(Length::Fixed(W_HABITAT))
            .style(theme::Text::Color(style::GOLD));
        let muted = |s: String| {
            text(s).size(12).style(theme::Text::Color(style::MUTED))
        };
        let boss_text = |b: &PetSpec| {
            format!(
                "#{} {} ({}, lvl {})",
                b.index + 1,
                b.name(),
                class_word(b.class()),
                b.level,
            )
        };
        let mut runners: Option<String> = None;
        let line = match (boss, ranked.first()) {
            (None, _) => row!(
                muted(t("explored").to_string()).width(Length::Fixed(W_BOSS))
            ),
            (Some(b), None) => row!(
                text(boss_text(b)).size(12).width(Length::Fixed(W_BOSS)),
                muted(t("no pet of this habitat yet").to_string()),
            ),
            (Some(b), Some(best)) => {
                // The runners-up, so a second choice is visible when the
                // first is out of fights for the day
                let others: Vec<String> = ranked
                    .iter()
                    .skip(1)
                    .take(2)
                    .map(|m: &Matchup| {
                        format!("{} {}", pet_word(&m.ours), pct(m.odds))
                    })
                    .collect();
                if !others.is_empty() {
                    runners = Some(format!("{} {}", t("then"), others.join(", ")));
                }
                row!(
                    text(boss_text(b)).size(12).width(Length::Fixed(W_BOSS)),
                    text(format!("{}: {}", t("send"), pet_word(&best.ours)))
                        .size(12)
                        .width(Length::Fixed(W_SEND)),
                    text(pct(best.odds))
                        .size(12)
                        .width(Length::Fixed(W_ODDS))
                        .style(theme::Text::Color(odds_colour(best.odds))),
                )
            }
        };
        rows = rows.push(
            row!(head, line.spacing(8).align_items(Alignment::Center))
                .spacing(6)
                .align_items(Alignment::Center),
        );
        // The runners-up on a line of their own, under the boss, in grey
        if let Some(r) = runners {
            rows = rows.push(
                row!(
                    iced::widget::Space::with_width(Length::Fixed(
                        W_HABITAT + 6.0
                    )),
                    text(r).size(11).style(theme::Text::Color(style::MUTED)),
                )
                .align_items(Alignment::Center),
            );
        }
    }
    rows.into()
}

/// The pet arena: our strongest of a habitat against the opponent's pet,
/// typed in off the game's screen
fn arena_block<'a>(
    player: &'a AccountInfo,
    report: Option<&PetSimReport>,
) -> Element<'a, Message> {
    let ident = player.ident;
    let sim = &player.pet_info.sim;
    let field = |what: PetSimField, value: &str, width: f32| {
        text_input("", value)
            .on_input(move |text| Message::PetSimField {
                ident,
                field: what,
                text,
            })
            .size(12)
            .padding(4)
            .width(Length::Fixed(width))
    };
    let label = |s: &'static str| {
        text(s).size(11).style(theme::Text::Color(style::GOLD_DIM))
    };
    let picks = pet_picks(sim.habitat);
    let their_pick = picks.get(sim.their_index).copied();
    let choose = row!(
        label(t("Habitat")),
        pick_list(HABITAT_PICKS, Some(HabitatPick(sim.habitat)), move |h| {
            Message::PetSimHabitat {
                ident,
                habitat: h.0,
            }
        })
        .text_size(12)
        .padding(3),
        label(t("Their pet")),
        pick_list(picks, their_pick, move |p| Message::PetSimTheirPet {
            ident,
            index: p.index,
        })
        .text_size(12)
        .padding(3),
        label(t("Level")),
        field(PetSimField::Level, &sim.their_level, 48.0),
    )
    .spacing(6)
    .align_items(Alignment::Center);
    let pack = row!(
        label(t("Their pack")),
        field(PetSimField::Pack, &sim.their_pack, 40.0),
        label(t("at 100")),
        field(PetSimField::At100, &sim.their_at100, 40.0),
        label(t("at 150")),
        field(PetSimField::At150, &sim.their_at150, 40.0),
        label(t("at 200")),
        field(PetSimField::At200, &sim.their_at200, 40.0),
        label(t("Gladiator")),
        field(PetSimField::Gladiator, &sim.their_gladiator, 40.0),
    )
    .spacing(6)
    .align_items(Alignment::Center);

    let verdict: Element<'a, Message> = match report.and_then(|r| r.arena) {
        None => text(t("no pet of this habitat yet"))
            .size(12)
            .style(theme::Text::Color(style::MUTED))
            .into(),
        Some(m) => {
            let side = |who: &'static str, s: &PetSpec| {
                let st = s.stats();
                text(format!(
                    "{who}: {} ({}, {} {} %, {} {}) · {} {} · {} {}",
                    pet_word(s),
                    class_word(st.class),
                    t("pack"),
                    s.bonus_pct,
                    t("gladiator"),
                    s.gladiator,
                    t("HP"),
                    crate::ui::thousands(st.hp),
                    t("hit"),
                    st.damage,
                ))
                .size(12)
            };
            column!(
                side(t("Ours"), &m.ours),
                side(t("Theirs"), &m.theirs),
                text(format!("{} {}", t("We win"), pct(m.odds)))
                    .size(13)
                    .style(theme::Text::Color(odds_colour(m.odds))),
            )
            .spacing(3)
            .into()
        }
    };
    column!(
        choose,
        pack,
        verdict,
        style::note(t(
            "The game does not hand over the opponent's pet, so it is typed \
             in off the arena screen: which pet, its level, and how many pets \
             the opponent owns in that habitat. The game sends our strongest \
             pet of the habitat, so that is the one counted.",
        )),
    )
    .spacing(6)
    .width(Length::Fill)
    .into()
}

/// The five habitats by the names a player uses, not the `{:?}` of the
/// library's enum
fn habitat_name(h: HabitatType) -> &'static str {
    match h {
        HabitatType::Shadow => t("Shadow"),
        HabitatType::Light => t("Light"),
        HabitatType::Earth => t("Earth"),
        HabitatType::Fire => t("Fire"),
        HabitatType::Water => t("Water"),
    }
}

pub fn view_pets(player: &AccountInfo) -> Element<'_, Message> {
    let lock = player.status.lock_safe();
    let gs = match &*lock {
        AccountStatus::Idle(_, gs) | AccountStatus::Busy(gs, _) => gs,
        AccountStatus::FatalError(err) => {
            return text(format!("Error: {err}")).size(20).into();
        }
        _ => return text(t("Logging in")).size(20).into(),
    };
    let ident = player.ident;

    let pi = &player.pet_info;
    let mut all = column!().spacing(10).width(Length::Fill);

    match gs.pets.as_ref() {
        None => {
            all = all.push(style::block(
                text(t("This character has no pets yet")).size(13),
            ));
        }
        Some(pets) => {
            all = all.push(
                row!(
                    style::tile(
                        t("Pets collected"),
                        pets.total_collected.to_string(),
                        style::NAME_GOLD
                    ),
                    style::tile(
                        t("Honor"),
                        crate::ui::thousands(u64::from(pets.honor)),
                        style::NAME_GOLD
                    ),
                    style::tile(
                        t("Rank"),
                        crate::ui::thousands(u64::from(pets.rank)),
                        style::NAME_GOLD
                    ),
                    style::tile_sub(
                        t("Pet arena opponent"),
                        pets.opponent.pet_count.to_string(),
                        style::NAME_GOLD,
                        Some(format!(
                            "{} {}",
                            t("level total"),
                            pets.opponent.level_total
                        )),
                    ),
                    style::tile(
                        t("Next free fight"),
                        match pets.opponent.next_free_battle {
                            Some(w) if w > chrono::Local::now() =>
                                w.format("%d.%m. %H:%M").to_string(),
                            _ => t("now").to_string(),
                        },
                        match pets.opponent.next_free_battle {
                            Some(w) if w > chrono::Local::now() =>
                                style::NAME_GOLD,
                            _ => style::GOOD,
                        },
                    ),
                )
                .spacing(8)
                .width(Length::Fill),
            );

            // One card per habitat, the five side by side the way the game
            // shelves them. What matters per habitat is the same four
            // things: how many are owned, how much fruit is waiting, how
            // much of it went out today, and whether the arena fight is
            // still there
            let mut shelf = row!().spacing(8).width(Length::Fill);
            for habitat in [
                HabitatType::Shadow,
                HabitatType::Light,
                HabitatType::Earth,
                HabitatType::Fire,
                HabitatType::Water,
            ] {
                let h = &pets.habitats[habitat];
                let owned = h.pets.iter().filter(|p| p.level > 0).count();
                let best = h.pets.iter().map(|p| p.level).max().unwrap_or(0);
                let fed_today = h
                    .pets
                    .iter()
                    .map(|p| u32::from(p.fruits_today))
                    .sum::<u32>();
                let total: u32 =
                    h.pets.iter().map(|p| u32::from(p.level)).sum();
                shelf = shelf.push(
                    container(
                        column!(
                            text(habitat_name(habitat))
                                .size(13)
                                .style(theme::Text::Color(style::GOLD)),
                            style::line_in(
                                t("Pets"),
                                format!("{owned}/20"),
                                Some(if owned >= 20 {
                                    style::GOOD
                                } else {
                                    style::MUTED
                                }),
                            ),
                            style::line_in(
                                t("Fruit"),
                                h.fruits.to_string(),
                                Some(if h.fruits > 0 {
                                    style::GOOD
                                } else {
                                    style::MUTED
                                }),
                            ),
                            style::line_in(
                                t("Fed today"),
                                fed_today.to_string(),
                                None,
                            ),
                            style::line_in(
                                t("Best / total"),
                                format!("{best} / {total}"),
                                None,
                            ),
                            style::line_in(
                                t("Pet arena"),
                                if h.battled_opponent {
                                    t("done")
                                } else {
                                    t("open")
                                },
                                Some(if h.battled_opponent {
                                    style::MUTED
                                } else {
                                    style::GOOD
                                }),
                            ),
                        )
                        .spacing(3),
                    )
                    .padding([7, 9])
                    .width(Length::Fill)
                    .style(style::well()),
                );
            }
            all = all.push(style::panel(t("Habitats"), shelf));

            // The fights, fought here first. Worked out on the UI tick
            // from the pets as they stand; until the first tick there is
            // nothing to show yet
            let report = pi.sim_report.as_ref().map(|(_, r)| r);
            let bosses: Element<'_, Message> = match report {
                Some(r) => boss_rows(r),
                None => text(t("working it out"))
                    .size(12)
                    .style(theme::Text::Color(style::MUTED))
                    .into(),
            };
            all = all.push(
                row!(
                    style::panel(
                        t("Habitat fights, worked out"),
                        column!(
                            bosses,
                            style::note(format!(
                                "{} {}",
                                t("Ten thousand fights each, SF Tools' arithmetic. \
                                   Green from 75 %, gold from 50 %. Gladiator trainer:"),
                                report.map_or(0, |r| r.gladiator),
                            )),
                        )
                        .spacing(6),
                    )
                    .width(Length::FillPortion(3)),
                    style::panel(t("Pet arena, worked out"), arena_block(player, report))
                        .width(Length::FillPortion(2)),
                )
                .spacing(10)
                .align_items(Alignment::Start),
            );
        }
    }

    // ---------------------------------------------------------- automation
    let mut auto = column!().spacing(6).width(Length::Fill);
    auto = auto.push(
        checkbox(t("Feed the pets automatically"), pi.auto_feed)
            .style(style::check())
            .on_toggle(move |nv| Message::PetSetAutoFeed { ident, state: nv })
            .text_size(13),
    );
    auto = auto.push(style::note(t(
        "Every ~30 minutes. Every pet you own gets fed, the strongest of a \
         habitat first, until the fruit runs out or the game turns the fruit \
         down.",
    )));
    auto = auto.push(
        checkbox(t("Fight the pet arena opponent"), pi.auto_arena)
            .style(style::check())
            .on_toggle(move |nv| Message::PetSetAutoArena { ident, state: nv })
            .text_size(13),
    );
    auto = auto.push(style::note(t(
        "Only the free fight. Each habitat may attack the day's opponent \
         once - the one with the highest level total goes first, the same one \
         the game itself picks.",
    )));
    auto = auto.push(
        button(text(t("Feed and fight now")).size(12))
            .padding([5, 10])
            .style(style::brass(false))
            .on_press(Message::PetFeedTimer { ident }),
    );
    if let Some(status) = &pi.status {
        auto = auto.push(text(status).size(11));
    }
    if let Some(status) = &pi.arena_status {
        auto = auto.push(text(status).size(11));
    }

    let mut right = column!().spacing(10).width(Length::FillPortion(1));
    if pi.log.is_empty() {
        right = right.push(style::block(style::note(t("Nothing yet"))));
    } else {
        let mut log = column!().spacing(3);
        for (time, msg, _) in pi.log.iter().rev().take(40) {
            log = log
                .push(text(format!("{} {msg}", time.format("%H:%M"))).size(11));
        }
        right = right.push(style::panel(
            t("Pet log"),
            scrollable(log).height(Length::Fixed(220.0)),
        ));
    }

    all = all.push(
        row!(
            style::panel(t("Automation"), auto).width(Length::FillPortion(1)),
            right,
        )
        .spacing(10)
        .align_items(Alignment::Start),
    );

    style::page(all)
}
