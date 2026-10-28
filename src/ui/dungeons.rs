use crate::i18n::t;
use iced::{
    Alignment, Color, Element, Length,
    alignment::Horizontal,
    theme,
    widget::{
        button, column, container, progress_bar, row, scrollable,
        },
};
use super::glyphs::ticked as checkbox;
// His lettering stands in for the toolkit's `text`, so every line on
// this screen is set in the face the purse card is
use super::glyphs::lettered as text;

use super::remaining_minutes;
use crate::{
    message::Message,
    player::{AccountInfo, AccountStatus},
    ui::style,
    util::LockExt,
};

/// Red -> green, depending on how likely a win is
fn win_color(win: f64) -> Color {
    match win {
        w if w >= 0.9 => Color::from_rgb(0.29, 0.69, 0.36),
        w if w >= 0.6 => Color::from_rgb(0.65, 0.72, 0.25),
        w if w >= 0.3 => Color::from_rgb(0.85, 0.72, 0.20),
        _ => Color::from_rgb(0.83, 0.33, 0.33),
    }
}

pub fn view_dungeons(player: &AccountInfo) -> Element<'_, Message> {
    let lock = player.status.lock_safe();

    let gs = match &*lock {
        AccountStatus::Idle(_, gs) | AccountStatus::Busy(gs, _) => gs,
        AccountStatus::FatalError(err) => {
            return text(format!("Error: {err}")).size(20).into();
        }
        _ => return text(t("Logging in")).size(20).into(),
    };

    let ident = player.ident;
    let di = &player.dungeon_info;

    let fight_ready = !matches!(
        gs.dungeons.next_free_fight,
        Some(t) if t > chrono::Local::now()
    );
    let free_fight = match gs.dungeons.next_free_fight {
        Some(t2) if !fight_ready => {
            format!("{} {}", t("next free fight in"), remaining_minutes(t2))
        }
        _ => t("free fight ready").to_string(),
    };

    let head = row!(
        text(free_fight).size(12).style(theme::Text::Color(
            if fight_ready { style::GOOD } else { style::NAME_GOLD }
        )),
        button(text(t("Recalculate")).size(12))
            .padding([4, 9])
            .style(style::brass(false))
            .on_press(Message::DungeonRefresh { ident, force: true }),
        checkbox(t("Only enterable"), di.only_open)
            .style(style::check())
            .on_toggle(move |only_open| Message::DungeonOnlyOpen {
                ident,
                only_open
            })
            .size(16)
            .text_size(12),
    )
    .spacing(10)
    .align_items(Alignment::Center);

    let mut info_line = row!().spacing(10).align_items(Alignment::Center);
    if let Some(when) = di.computed {
        info_line = info_line.push(
            text(format!(
                "{} fights simulated per dungeon - calculated at {} ({} ms)",
                crate::dungeon::SIM_ITERATIONS,
                when.time().format("%H:%M:%S"),
                di.sim_millis
            ))
            .size(11)
            .style(iced::theme::Text::Color(crate::ui::style::MUTED)),
        );
    } else {
        info_line = info_line.push(
            text(t(
                "Press 'Recalculate' to simulate every dungeon with your \
                 current equipment",
            ))
            .size(11)
            .style(iced::theme::Text::Color(crate::ui::style::MUTED)),
        );
    }
    let min_chance = crate::ui::num_field(di.min_chance, 100u8, move |nv| {
        Message::DungeonSetMinChance { ident, percent: nv }
    });

    let pets_row = row!(
        checkbox(t("Pet habitat dungeons first"), di.pets_first)
.style(crate::ui::style::check())
            .on_toggle(move |state| Message::DungeonSetPetsFirst { ident, state })
            .size(16)
            .text_size(13),
        text(t(
            "- they share the cooldown; the pet with the best odds in the \
             simulator fights, and only from the minimum below"
        ))
        .size(11),
    )
    .spacing(8)
    .align_items(Alignment::Center);
    let auto_row = row!(
        checkbox(t("Auto dungeon"), di.auto)
            .style(crate::ui::style::check())
            .on_toggle(move |state| Message::DungeonSetAuto { ident, state })
            .size(16)
            .text_size(13),
        text(t("- best odds first; minimum")).size(12),
        min_chance,
        text(t("% (0 = always the best one)")).size(12),
    )
    .spacing(8)
    .align_items(Alignment::Center);

    // Extra fights bought with mushrooms, once the free one is gone. Off by
    // default and capped per day - mushrooms are premium currency
    let mushroom_min =
        crate::ui::num_field(di.mushroom_min, 100u8, move |nv| {
            Message::DungeonSetMushroomMin { ident, percent: nv }
        });
    let mushroom_row = row!(
        checkbox(t("Spend mushrooms on extra fights"), di.mushroom)
            .style(crate::ui::style::check())
            .on_toggle(move |state| Message::DungeonSetMushroom {
                ident,
                state
            })
            .size(16)
            .text_size(13),
        text(t("- only above")).size(12),
        mushroom_min,
        text(format!(
            "{} ({}/{} {})",
            t("% win chance"),
            di.mushroom_today,
            di.mushroom_max,
            t("today")
        ))
        .size(12),
    )
    .spacing(8)
    .align_items(Alignment::Center);

    let portal_row = row!(
        checkbox(t("Fight the demonic portal daily"), di.portal_auto)
            .style(crate::ui::style::check())
            .on_toggle(move |state| Message::DungeonSetPortal { ident, state })
            .size(16)
            .text_size(13),
        text(t("- once a day, on its own")).size(12),
    )
    .spacing(8)
    .align_items(Alignment::Center);

    let status_line: Element<Message> = match &di.status {
        Some(status) => text(status)
            .size(13)
            .style(theme::Text::Color(Color::from_rgb(0.95, 0.80, 0.40)))
            .into(),
        None => text("").size(13).into(),
    };

    let header = row!(
        text(t("Dungeon")).size(12).width(Length::FillPortion(20)),
        text(t("Stage"))
            .size(12)
            .width(Length::FillPortion(5))
            .horizontal_alignment(Horizontal::Center),
        text(t("Enemy")).size(12).width(Length::FillPortion(18)),
        text(t("Lvl"))
            .size(12)
            .width(Length::FillPortion(4))
            .horizontal_alignment(Horizontal::Center),
        text(t("Win chance"))
            .size(12)
            .width(Length::FillPortion(18))
            .horizontal_alignment(Horizontal::Center),
        text("").width(Length::Fixed(60.0)),
    )
    .spacing(6)
    .align_items(Alignment::Center);

    let mut list = column!().spacing(5);
    let mut shown = 0;

    // The list in the game's own two tabs - Light World, then Shadow World
    // (the one fought with the three companions) - with a heading each, so
    // a row can be found the way the game shows it. The "shadow" tag on
    // every row is gone with that
    let groups: [(&str, bool, Color); 2] = [
        ("Light World", false, Color::from_rgb(0.55, 0.75, 0.85)),
        (
            "Shadow World - with Bert, Mark and Kunigunde",
            true,
            Color::from_rgb(0.70, 0.55, 0.85),
        ),
    ];
    for (heading, want_shadow, colour) in groups {
        let rows: Vec<_> = di
            .chances
            .iter()
            .filter(|c| c.shadow == want_shadow && (!di.only_open || c.open))
            .collect();
        if rows.is_empty() {
            continue;
        }
        list = list
            .push(text(t(heading)).size(13).style(theme::Text::Color(colour)));
        for c in rows {
            let playable = c.open;
            let predicted = c.win.is_finite();
            shown += 1;

            let name: Element<Message> =
                text(&c.name).size(13).width(Length::FillPortion(20)).into();

            let chance: Element<Message> = if predicted {
                let pct = c.win * 100.0;
                row!(
                    progress_bar(0.0..=1.0, c.win as f32)
                        .height(Length::Fixed(9.0))
                        .style(style::bar(win_color(c.win)))
                        .width(Length::FillPortion(3)),
                    text(format!("{pct:.0}%"))
                        .size(12)
                        .style(theme::Text::Color(win_color(c.win)))
                        .width(Length::FillPortion(1))
                        .horizontal_alignment(Horizontal::Right),
                )
                .spacing(6)
                .width(Length::FillPortion(18))
                .align_items(Alignment::Center)
                .into()
            } else {
                text(t("-"))
                    .size(12)
                    .width(Length::FillPortion(18))
                    .horizontal_alignment(Horizontal::Center)
                    .into()
            };

            let fight: Element<Message> = if playable && !fight_ready {
                // Sending the fight now would only be refused, so the button
                // waits together with the timer
                text(t("wait")).size(11).into()
            } else if playable {
                button(text(t("Fight")).size(12))
                    .padding(4)
                    .on_press(Message::DungeonFight {
                        ident,
                        dungeon: c.dungeon,
                        name: c.name.clone(),
                        attempt: 0,
                        // The manual button always uses the free fight
                        mushroom: false,
                    })
                    .into()
            } else {
                text("").into()
            };

            list = list.push(
                row!(
                    name,
                    text(if playable {
                        c.stage.to_string()
                    } else {
                        String::new()
                    })
                    .size(12)
                    .width(Length::FillPortion(5))
                    .horizontal_alignment(Horizontal::Center),
                    text(&c.enemy).size(12).width(Length::FillPortion(18)),
                    text(if c.enemy_level > 0 {
                        c.enemy_level.to_string()
                    } else {
                        String::new()
                    })
                    .size(12)
                    .width(Length::FillPortion(4))
                    .horizontal_alignment(Horizontal::Center),
                    chance,
                    row!(fight).width(Length::Fixed(60.0)),
                )
                .spacing(6)
                .align_items(Alignment::Center),
            );
        }
    }

    let body: Element<Message> = if shown == 0 {
        text(t(
            "Nothing to show yet - press 'Recalculate'. Locked and cleared \
             dungeons are hidden while 'Only enterable' is on.",
        ))
        .size(13)
        .into()
    } else {
        scrollable(list).height(Length::Fill).into()
    };

    let mut log_col = column!().spacing(3).padding(5);
    for (time, msg, ok) in di.log.iter().rev().take(20) {
        log_col = log_col.push(
            row!(
                text(time.format("%H:%M").to_string()).size(11),
                text(msg).size(11).style(theme::Text::Color(if *ok {
                    Color::from_rgb(0.36, 0.78, 0.45)
                } else {
                    Color::from_rgb(0.90, 0.45, 0.45)
                })),
            )
            .spacing(8),
        );
    }
    let log: Element<Message> = if di.log.is_empty() {
        text("").into()
    } else {
        scrollable(log_col).height(Length::Fixed(90.0)).into()
    };

    let settings = column!(
        info_line,
        auto_row,
        mushroom_row,
        portal_row,
        pets_row,
        status_line,
    )
    .spacing(6)
    .width(Length::Fill);

    let table = column!(header, body).spacing(5).height(Length::Fill);

    let mut page = column!(
        style::panel(t("Dungeon settings"), settings),
        style::panel_bar(t("Dungeons"), head, table).height(Length::Fill),
    )
    .spacing(10)
    .height(Length::Fill)
    .width(Length::Fill);

    if !di.log.is_empty() {
        page = page.push(style::panel(t("Dungeon log"), log));
    }

    container(page)
        .padding([12, 14])
        .height(Length::Fill)
        .width(Length::Fill)
        .into()
}
