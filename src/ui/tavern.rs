use super::glyphs::ticked as checkbox;
use crate::i18n::t;
use iced::{
    Alignment, Color, Element, Length, theme,
    widget::{button, column, horizontal_space, row, scrollable},
};
// His lettering stands in for the toolkit's `text`, so every line on
// this screen is set in the face the purse card is
use super::glyphs::lettered as text;
use sf_api::gamestate::tavern::{CurrentAction, ExpeditionStage};

use super::{glyphs, remaining_minutes};
use crate::{
    expedition::ExpeditionGoal,
    message::{ExpeditionOption, Message, TavernOption},
    player::{AccountInfo, AccountStatus},
    ui::style,
    util::LockExt,
};

fn stat(name: &str, value: String) -> Element<'_, Message> {
    row!(
        text(name).size(12).width(Length::FillPortion(1)),
        text(value)
            .size(12)
            .width(Length::FillPortion(1))
            .horizontal_alignment(iced::alignment::Horizontal::Right)
    )
    .into()
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum TavernTab {
    #[default]
    Expeditions,
    Rewards,
    Settings,
}

pub fn view_tavern(
    player: &AccountInfo,
    tab: TavernTab,
) -> Element<'_, Message> {
    let lock = player.status.lock_safe();
    let gs = match &*lock {
        AccountStatus::Idle(_, gs) | AccountStatus::Busy(gs, _) => gs,
        AccountStatus::FatalError(err) => {
            return text(format!("Error: {err}")).size(20).into();
        }
        _ => return text(t("Logging in")).size(20).into(),
    };

    let ident = player.ident;
    let ei = &player.expedition_info;
    let tavern = &gs.tavern;

    // The six figures a tavern run is decided by, as the game's own number
    // boxes across the top. They were six `label: value` rows, which is a
    // paragraph to read where this is a glance
    let cur_min = tavern.thirst_for_adventure_sec / 60;
    let beers_left =
        u32::from(tavern.beer_max.saturating_sub(tavern.beer_drunk));
    // The game only reports the thirst it has already handed out. Every beer
    // still available today is worth another 20 min once drunk, so the real
    // daily potential is the current thirst plus the beers not yet drunk
    let potential = cur_min + beers_left * 20;
    // What it is, and under it when it ends. One line of "mestska straz do
    // 07:00" does not fit the plate and was being cut off mid-word, and a
    // cut-off word is worse than no word: the plate said "MESTSKA STRAZ D"
    let (doing, doing_until): (String, Option<String>) = match &tavern
        .current_action
    {
        CurrentAction::Idle => (t("nothing").to_string(), None),
        CurrentAction::Quest { busy_until, .. } => (
            t("quest").to_string(),
            Some(format!("{} {}", t("until"), busy_until.format("%H:%M"))),
        ),
        CurrentAction::CityGuard { busy_until, .. } => (
            t("city guard").to_string(),
            Some(format!("{} {}", t("until"), busy_until.format("%H:%M"))),
        ),
        CurrentAction::Expedition => (t("expedition").to_string(), None),
        CurrentAction::Unknown(_) => (t("something unknown").to_string(), None),
    };
    // "Available" has to mean "can I start one right now", not "is the
    // event on". With the thirst at zero and no beer left the answer is no,
    // whatever the server says about the event - the box read YES next to a
    // flat 0/300, which is the screen contradicting itself
    let (exped_says, exped_colour) = match readiness(
        tavern.expeditions.is_event_ongoing(),
        cur_min,
        beers_left,
        ei.beer_affordable(player.free_mushrooms(gs), player.tavern_info.beer_free),
    ) {
        Ready::EventOff => (t("not right now"), style::MUTED),
        Ready::Yes => (t("yes"), style::GOOD),
        Ready::AfterBeer => (t("after a beer"), style::WARN),
        Ready::NoBeerFunds => (t("No mushrooms for beer"), style::WARN),
        Ready::NoThirst => (t("no thirst left"), style::BAD),
    };
    let tiles = row!(
        // One line. Thirst is a count out of a daily ceiling and reads as
        // "0/300" the way the beers read "11/11" - stacking the ceiling
        // under the figure made one number look like two
        style::tile(
            t("Thirst"),
            crate::mechanics::thirst_line(cur_min, tavern.beer_max),
            if cur_min > 0 { style::GOOD } else { style::BAD },
        ),
        style::tile_sub(
            t("Beer today"),
            format!("{}/{}", tavern.beer_drunk, tavern.beer_max),
            if beers_left > 0 {
                style::GOOD
            } else {
                style::MUTED
            },
            Some(format!("{} {potential} min", t("today up to"))),
        ),
        // The purse's own hourglass and mushroom, so the two boxes that
        // count the things in the purse carry the same picture it does
        style::tile_pic(
            crate::ui::hourglass_pic(),
            t("Quicksand glasses"),
            tavern.quicksand_glasses.to_string(),
            glyphs::PURSE_BLUE,
        ),
    )
    .spacing(16)
    .width(Length::Fill);
    let current = row!(
        text(format!(
            "{}: {} {}",
            t("Currently doing"),
            doing,
            doing_until.unwrap_or_default()
        ))
        .size(13),
        horizontal_space(),
        text(format!("{}: {}", t("Expeditions available"), exped_says))
            .size(13)
            .style(theme::Text::Color(exped_colour)),
    )
    .spacing(16)
    .align_items(Alignment::Center);

    let mut col = column!().spacing(7).width(Length::Fill);

    // ------------------------------------------------ running expedition
    if let Some(active) = tavern.expeditions.active() {
        col = col.push(stat(
            t("Task:"),
            format!(
                "{} {}/{}",
                crate::expedition::thing_name(active.target_thing),
                active.target_current,
                active.target_amount
            ),
        ));
        // The four slots are what every chain decision is made from, so they
        // belong on screen next to the decision
        let carried: Vec<String> = active
            .items
            .iter()
            .flatten()
            .map(|i| crate::expedition::thing_name(*i))
            .collect();
        col = col.push(stat(
            t("Carrying:"),
            if carried.is_empty() {
                "nothing yet".to_string()
            } else {
                carried.join(" > ")
            },
        ));
        col = col
            .push(stat(t("Clearing:"), format!("{}/10", active.current_floor)));
        let heroism_bonus = match active.heroism {
            h if h >= 40 => "+35% XP",
            h if h >= 30 => "+20% XP",
            h if h >= 20 => "+10% XP",
            _ => "no bonus yet",
        };
        col = col.push(stat(
            t("Heroism:"),
            format!("{} ({heroism_bonus})", active.heroism),
        ));
        let stage = match active.current_stage() {
            ExpeditionStage::Encounters(list) => {
                let opts: Vec<String> = list
                    .iter()
                    .map(|e| {
                        format!(
                            "{} {:+}",
                            crate::expedition::thing_name(e.typ),
                            e.heroism
                        )
                    })
                    .collect();
                format!("crossroads: {}", opts.join(" / "))
            }
            ExpeditionStage::Boss(b) => format!("boss ({} items)", b.items),
            ExpeditionStage::Rewards(r) => {
                format!("{} rewards to choose", r.len())
            }
            ExpeditionStage::Waiting { busy_until, .. } => {
                if busy_until > chrono::Local::now() {
                    format!("waiting {}", remaining_minutes(busy_until))
                } else {
                    "wait over".to_string()
                }
            }
            ExpeditionStage::Finished => "finished".to_string(),
            ExpeditionStage::Unknown => "unknown".to_string(),
        };
        col = col.push(text(format!("Stage: {stage}")).size(12));
    } else {
        col = col.push(
            text(t("No expedition running"))
                .size(12)
                .style(theme::Text::Color(style::MUTED)),
        );
        for (i, e) in tavern.expeditions.available.iter().enumerate() {
            col = col.push(
                text(format!(
                    "Option {}: {}, {} min{}",
                    i + 1,
                    crate::expedition::thing_name(e.target),
                    e.thirst_for_adventure_sec / 60,
                    match e.special {
                        Some(s) => format!(", {s:?}"),
                        None => String::new(),
                    }
                ))
                .size(12),
            );
        }
    }

    // ------------------------------------------------------- automation
    let opt = |label: &'static str, option: ExpeditionOption, on: bool| {
        checkbox(label, on)
            .style(crate::ui::style::check())
            .on_toggle(move |state| Message::ExpeditionSetOption {
                ident,
                option,
                state,
            })
            .size(18)
            .text_size(13)
    };
    col = col.push(opt(
        t("Run expeditions automatically"),
        ExpeditionOption::Auto,
        ei.auto,
    ));
    // A switch, not a checkbox: it decides what the encounters, the boss
    // reward, the expedition and the quest are all picked for
    let gold_on = ei.goal == ExpeditionGoal::Gold;
    let goal_button = |label: &'static str, on: bool, gold: bool| {
        button(
            text(t(label))
                .size(12)
                .width(Length::Fill)
                .horizontal_alignment(iced::alignment::Horizontal::Center),
        )
        .padding([4, 6])
        .width(Length::Fixed(90.0))
        .style(crate::ui::style::segment(
            on,
            if gold {
                crate::ui::style::WARN
            } else {
                crate::ui::style::ACCENT
            },
        ))
        .on_press(Message::ExpeditionSetOption {
            ident,
            option: ExpeditionOption::Gold,
            state: gold,
        })
    };
    col = col.push(
        row!(
            text(t("Playing for:")).size(13),
            goal_button("XP", !gold_on, false),
            goal_button("Gold", gold_on, true),
        )
        .spacing(8)
        .align_items(Alignment::Center),
    );
    col = col.push(opt(
        t("Skip waits with quicksand glasses"),
        ExpeditionOption::SkipGlass,
        ei.skip_glass,
    ));
    // Quests are not offered: expeditions pay better for the same thirst,
    // and the guild leader does not want the choice on the page at all
    let ti = &player.tavern_info;
    col = col.push(
        row!(
            checkbox(
                t("Beer when the thirst is used up (1 mushroom each)"),
                ti.beer_auto
            )
            .style(crate::ui::style::check())
            .on_toggle(move |state| Message::TavernSetOption {
                ident,
                option: TavernOption::Beer,
                state,
            })
            .size(18)
            .text_size(13),
            text(t("up to")).size(12),
            crate::ui::num_field(ti.beer_max, 20u8, move |n| {
                Message::BeerSetMax { ident, n }
            }),
            text(t("a day")).size(12),
        )
        .spacing(8)
        .align_items(Alignment::Center),
    );
    col = col.push(
        checkbox(t("Beer is free for this character (VIP)"), ti.beer_free)
            .style(crate::ui::style::check())
            .on_toggle(move |state| Message::SetBeerFree { ident, state })
            .size(18)
            .text_size(13),
    );
    let mut shrooms = column!().spacing(7).width(Length::Fill);
    // The day's budget, the same box as on the shops page - it is the one
    // number a player looks for, so it is wherever mushrooms are spent
    // Buttons, one lit at a time: the number boxes could not be typed
    // into and took a click per mushroom
    // `budget` picks the message: the day's budget or the reserve
    let choice = |steps: &[u32], current: u32, budget: bool| {
        let mut r = row!().spacing(3);
        for &v in steps {
            r = r.push(
                button(text(v.to_string()).size(11))
                    .padding([3, 7])
                    .style(if current == v {
                        crate::ui::style::primary()
                    } else {
                        crate::ui::style::quiet()
                    })
                    .on_press(if budget {
                        Message::SetMushroomBudget { ident, n: v }
                    } else {
                        Message::SetMushroomReserve { ident, n: v }
                    }),
            );
        }
        r
    };
    shrooms = shrooms.push(
        row!(
            text(t("Mushrooms a day:")).size(13),
            choice(&[0, 5, 10, 20, 30, 50, 100], ti.mushroom_budget, true),
            text(format!(
                "{} {}/{}",
                t("spent today"),
                ti.mushroom_spent_today,
                ti.mushroom_budget
            ))
            .size(13)
            .style(iced::theme::Text::Color(
                if ti.mushroom_spent_today >= ti.mushroom_budget {
                    crate::ui::style::BAD
                } else {
                    crate::ui::style::GOOD
                }
            )),
            text(t("- the bot's budget for the server day, for anything it pays mushrooms for"))
                .size(11)
                .style(iced::theme::Text::Color(crate::ui::style::MUTED)),
        )
        .spacing(8)
        .align_items(Alignment::Center),
    );
    // The guild leader's own rule as a button: keep what is there minus
    // 35 %, whatever that is right now - and a box to type any number
    let minus_35 = gs.character.mushrooms * 65 / 100;
    shrooms = shrooms.push(
        row!(
            text(t("Keep untouched:")).size(12),
            choice(&[0, 10, 25, 50, 100, 250], ti.mushroom_reserve, false),
            button(
                text(format!(
                    "{} ({} \u{2212} 35 %)",
                    minus_35, gs.character.mushrooms
                ))
                .size(11)
            )
            .padding([3, 7])
            .style(if ti.mushroom_reserve == minus_35 {
                crate::ui::style::primary()
            } else {
                crate::ui::style::quiet()
            })
            .on_press(Message::SetMushroomReserve { ident, n: minus_35 }),
            crate::ui::num_field(ti.mushroom_reserve, 9999u32, move |n| {
                Message::SetMushroomReserve { ident, n }
            }),
        )
        .spacing(8)
        .align_items(Alignment::Center),
    );
    shrooms = shrooms.push(style::note(t(
        "mushrooms - beer, shop rerolls and epics only spend what is above \
         this (plus the dragon's 25 when its renewal is within 3 days)",
    )));
    col = col.push(
        text(t(
            "Play the thirst down to nothing, then five beers in a row (a \
             full 100 min), play that down, five more, and the last beer with \
             the last quest. The city guard never starts while there is \
             thirst or beer left.",
        ))
        .size(11)
        .style(iced::theme::Text::Color(crate::ui::style::MUTED)),
    );
    col = col.push(
        text(t(
            "How it chooses: almost every pickup is part of a chain that has \
             to be collected in order and only pays on the last link (horn > \
             donkey > rainbow > unicorn). It reads the four item slots above, \
             so a chain that is already started gets finished instead of \
             dropped, and a chain that no longer fits into the clearings left \
             is not started at all. On top of that: the item this expedition \
             is looking for (+20 heroism at the end, -5 if missed), bounties \
             only when the thing they pay for is still reachable, then plain \
             heroism - minus the dead ends (cupcake and cake shrink the next \
             crossroads, the sword chain ends on a penalty, toilet paper only \
             pays as a set of three). Heroism 20/30/40 = +10/20/35% XP. \
             Playing for gold takes keys and treasure chests and the gold at \
             the boss instead. Long and short expeditions pay the same per \
             thirst, so the longest one that fits is started; daily-task and \
             egg expeditions come first.",
        ))
        .size(11)
        .style(iced::theme::Text::Color(crate::ui::style::MUTED)),
    );

    col = col.push(
        row!(
            button(text(t("Do the next step now")).size(12))
                .padding(4)
                .style(crate::ui::style::quiet())
                .on_press(Message::ExpeditionTimer { ident }),
            text(ei.status.clone().unwrap_or_default())
                .size(11)
                .style(theme::Text::Color(Color::from_rgb(0.95, 0.80, 0.40))),
        )
        .spacing(10)
        .align_items(Alignment::Center),
    );

    // ------------------------------------------------ daily free things
    let mut daily = column!().spacing(7).width(Length::Fill);
    let di = &player.dailies_info;
    let calendar = &gs.specials.calendar;
    let now = chrono::Local::now();
    daily = daily.push(stat(
        t("Login calendar:"),
        match calendar.next_possible {
            Some(when) if when > now => {
                format!("next door {}", when.format("%d.%m. %H:%M"))
            }
            _ if calendar.collected >= calendar.rewards.len() => {
                "done for this month".to_string()
            }
            _ => "a door is open".to_string(),
        },
    ));
    let chest_line = |chests: &[sf_api::gamestate::rewards::RewardChest],
                      points: u32| {
        chests
            .iter()
            .map(|c| {
                if c.opened {
                    "opened".to_string()
                } else if points >= c.required_points {
                    "ready".to_string()
                } else {
                    format!("{points}/{}", c.required_points)
                }
            })
            .collect::<Vec<_>>()
            .join(" \u{00B7} ")
    };
    let daily_tasks = &gs.specials.tasks.daily;
    daily = daily.push(stat(
        t("Gleeman - daily chests:"),
        chest_line(&daily_tasks.rewards, daily_tasks.earned_points()),
    ));
    let event_tasks = &gs.specials.tasks.event;
    if event_tasks.end.is_none_or(|t| t > now) {
        daily = daily.push(stat(
            t("Gleeman - event chests:"),
            chest_line(&event_tasks.rewards, event_tasks.earned_points()),
        ));
    }
    daily = daily.push(
        checkbox(t("Collect them automatically"), di.auto)
            .style(crate::ui::style::check())
            .on_toggle(move |state| Message::DailiesSetAuto { ident, state })
            .size(18)
            .text_size(13),
    );
    daily = daily.push(stat(
        t("Mount:"),
        match (gs.character.mount, gs.character.mount_end) {
            (Some(m), Some(end)) if end > now => {
                format!("{m:?} until {}", end.format("%d.%m. %H:%M"))
            }
            (Some(m), _) => format!("{m:?}"),
            (None, _) => "none - lapsed".to_string(),
        },
    ));
    daily = daily.push(
        checkbox(
            t("Keep the best mount rented: dragon, then tiger, horse, cow (needs the above on)"),
            di.mount_auto,
        )
.style(crate::ui::style::check())
            .on_toggle(move |state| Message::MountSetAuto { ident, state })
            .size(18)
            .text_size(13),
    );
    daily = daily.push(
        text(t(
            "The best mount the purse covers: dragon (25 mushrooms), then \
             tiger, horse, cow. Renewed the day before it runs out, bought \
             when there is none, upgraded the moment a better one is \
             affordable - never downgraded while it runs.",
        ))
        .size(11)
        .style(iced::theme::Text::Color(crate::ui::style::MUTED)),
    );
    if let Some(status) = &di.status {
        daily = daily.push(
            text(status)
                .size(11)
                .style(theme::Text::Color(Color::from_rgb(0.95, 0.80, 0.40))),
        );
    }

    // -------------------------------------------------------- dice
    let mut dice = column!().spacing(7).width(Length::Fill);
    dice = dice.push(stat(
        t("Games left today:"),
        format!(
            "{}, next free game {}",
            gs.tavern.dice_game.remaining,
            match gs.tavern.dice_game.next_free {
                Some(t) if t > chrono::Local::now() =>
                    t.format("%H:%M").to_string(),
                _ => "now".to_string(),
            }
        ),
    ));
    dice = dice.push(
        checkbox(t("Play automatically"), ti.dice_auto)
            .style(crate::ui::style::check())
            .on_toggle(move |state| Message::DiceSetAuto { ident, state })
            .size(18)
            .text_size(13),
    );
    dice = dice.push(
        checkbox(t("Skip the wait with quicksand glasses"), ti.dice_skip)
            .style(crate::ui::style::check())
            .on_toggle(move |state| Message::DiceSetSkip { ident, state })
            .size(18)
            .text_size(13),
    );
    dice = dice.push(
        button(text(t("Play now")).size(12))
            .padding(4)
            .style(crate::ui::style::quiet())
            .on_press(Message::DiceTimer { ident }),
    );
    dice = dice.push(
        text(t(
            "Free games only. Throws all five, keeps the symbol that came up \
             most often, throws the rest again and takes the winnings - three \
             of a kind pays. On a tie it keeps the one worth more: hourglass, \
             then arcane, souls, stone, wood.",
        ))
        .size(11)
        .style(iced::theme::Text::Color(crate::ui::style::MUTED)),
    );
    if let Some(status) = &ti.dice_status {
        dice = dice.push(
            text(status)
                .size(11)
                .style(theme::Text::Color(Color::from_rgb(0.95, 0.80, 0.40))),
        );
    }

    // ------------------------------------------------------- wheel
    let mut wheel = column!().spacing(7).width(Length::Fill);
    wheel = wheel.push(stat(
        t("Spins today:"),
        format!(
            "{}/20, {} lucky coins, free spin {}",
            gs.specials.wheel.spins_today,
            gs.specials.wheel.lucky_coins,
            match gs.specials.wheel.next_free_spin {
                Some(t) if t > chrono::Local::now() =>
                    t.format("%H:%M").to_string(),
                _ => "now".to_string(),
            }
        ),
    ));
    wheel = wheel.push(
        row!(
            checkbox(t("Spin automatically"), ti.wheel_auto)
                .style(crate::ui::style::check())
                .on_toggle(move |state| Message::TavernSetOption {
                    ident,
                    option: TavernOption::Wheel,
                    state,
                })
                .size(18)
                .text_size(13),
            crate::ui::num_field(ti.wheel_spins, 20u8, move |n| {
                Message::WheelSetSpins { ident, n }
            }),
            text(t("spins a day")).size(12),
        )
        .spacing(8)
        .align_items(Alignment::Center),
    );
    wheel = wheel.push(
        button(text(t("Spin now")).size(12))
            .padding(4)
            .style(crate::ui::style::quiet())
            .on_press(Message::WheelTimer { ident }),
    );
    wheel = wheel.push(
        text(t(
            "The free spin first, then lucky coins. Stops at the number of \
             spins you set.",
        ))
        .size(11)
        .style(iced::theme::Text::Color(crate::ui::style::MUTED)),
    );
    if let Some(status) = &ti.wheel_status {
        wheel = wheel.push(
            text(status)
                .size(11)
                .style(theme::Text::Color(Color::from_rgb(0.95, 0.80, 0.40))),
        );
    }
    if !ti.wheel_log.is_empty() {
        let mut log = column!().spacing(3).padding(5);
        for (time, msg, ok) in ti.wheel_log.iter().rev().take(20) {
            log = log.push(
                row!(
                    text(time.format("%H:%M").to_string()).size(11),
                    text(msg).size(11).style(theme::Text::Color(if *ok {
                        Color::from_rgb(0.36, 0.78, 0.45)
                    } else {
                        Color::from_rgb(0.90, 0.45, 0.45)
                    })),
                    horizontal_space(),
                )
                .spacing(8),
            );
        }
        wheel = wheel.push(scrollable(log).height(Length::Fixed(90.0)));
    }

    let mut exped_log = None;
    if !ei.log.is_empty() {
        let mut log = column!().spacing(3);
        for (time, msg, ok) in ei.log.iter().rev().take(60) {
            log = log.push(
                row!(
                    text(time.format("%H:%M").to_string()).size(11),
                    text(msg).size(11).style(theme::Text::Color(if *ok {
                        style::GOOD
                    } else {
                        style::BAD
                    })),
                    horizontal_space(),
                )
                .spacing(8),
            );
        }
        exped_log = Some(style::panel(
            t("Expedition log"),
            scrollable(log).height(Length::Fixed(180.0)),
        ));
    }

    let tab_button = |label, value| {
        button(text(t(label)).size(14))
            .padding([10, 18])
            .style(style::brass(tab == value))
            .on_press(Message::TavernTab(value))
    };
    let tabs = row!(
        tab_button("Expeditions", TavernTab::Expeditions),
        tab_button("Daily rewards", TavernTab::Rewards),
        tab_button("Settings", TavernTab::Settings),
    )
    .spacing(8);
    let body: Element<Message> = match tab {
        TavernTab::Expeditions => {
            let mut content =
                column!(style::panel(t("Tavern - expeditions"), col))
                    .spacing(18);
            if let Some(log) = exped_log {
                content = content.push(log);
            }
            content.into()
        }
        TavernTab::Rewards => column!(
            style::panel(t("Daily rewards"), daily),
            row!(
                style::panel(t("Game of dice"), dice),
                style::panel(t("Wheel of fortune"), wheel)
            )
            .spacing(18),
        )
        .spacing(18)
        .into(),
        TavernTab::Settings => {
            style::panel(t("Mushroom budget"), shrooms).into()
        }
    };
    style::page(
        column!(tiles, current, tabs, body)
            .spacing(20)
            .width(Length::Fill),
    )
}

/// Whether an expedition can be started, and if not, why.
///
/// The box used to ask the server one question - is the expedition event
/// running - and print YES. That is not what a player reads it as: with the
/// thirst at a flat 0/300 and every beer drunk, YES is the screen
/// contradicting the box right next to it.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Ready {
    /// The game is not running expeditions at all just now
    EventOff,
    /// There is thirst to spend
    Yes,
    /// No thirst, but a beer left to buy some with
    AfterBeer,
    NoBeerFunds,
    /// Nothing left today
    NoThirst,
}

fn readiness(event_on: bool, thirst_min: u32, beers_left: u32, beer_affordable: bool) -> Ready {
    if !event_on {
        Ready::EventOff
    } else if thirst_min > 0 {
        Ready::Yes
    } else if beers_left > 0 && !beer_affordable {
        Ready::NoBeerFunds
    } else if beers_left > 0 {
        Ready::AfterBeer
    } else {
        Ready::NoThirst
    }
}

#[cfg(test)]
mod expedition_tests {
    use super::{Ready, readiness};

    #[test]
    fn nothing_is_available_with_the_thirst_at_nothing() {
        // The bug as the guild leader found it: 0/300 thirst, every beer
        // drunk, and the box still saying YES
        assert_eq!(readiness(true, 0, 0, true), Ready::NoThirst);
        // A beer left is a way back to some thirst, so it is neither a yes
        // nor a flat no
        assert_eq!(readiness(true, 0, 3, true), Ready::AfterBeer);
        assert_eq!(readiness(true, 0, 3, false), Ready::NoBeerFunds);
        assert_eq!(readiness(true, 114, 0, false), Ready::Yes);
    }

    #[test]
    fn the_event_being_off_outranks_everything() {
        for (thirst, beers) in [(0, 0), (0, 5), (300, 5)] {
            assert_eq!(readiness(false, thirst, beers, false), Ready::EventOff);
        }
    }
}
