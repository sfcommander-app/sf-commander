//! The city guard: the "work" building. Its own page, because it is not
//! part of the tavern - a shift and a quest/expedition exclude each other

use crate::i18n::t;
use iced::{
    Alignment, Color, Element, Length, theme,
    widget::{
        button, column, horizontal_space, row,
        scrollable, },
};
use super::glyphs::ticked as checkbox;
// His lettering stands in for the toolkit's `text`, so every line on
// this screen is set in the face the purse card is
use super::glyphs::lettered as text;
use sf_api::gamestate::tavern::CurrentAction;

use crate::{
    message::Message,
    player::{AccountInfo, AccountStatus},
    util::LockExt,
};

pub fn view_guard(player: &AccountInfo) -> Element<'_, Message> {
    let lock = player.status.lock_safe();
    let gs = match &*lock {
        AccountStatus::Idle(_, gs) | AccountStatus::Busy(gs, _) => gs,
        AccountStatus::FatalError(err) => {
            return text(format!("Error: {err}")).size(20).into();
        }
        _ => return text(t("Logging in")).size(20).into(),
    };
    let ident = player.ident;
    let tavern = &gs.tavern;

    let mut col = column!().spacing(7).width(Length::Fill);
    let doing = match &tavern.current_action {
        CurrentAction::Idle => t("nothing").to_string(),
        CurrentAction::Quest { busy_until, .. } => {
            format!("{} {}", t("quest until"), busy_until.format("%H:%M"))
        }
        CurrentAction::CityGuard { busy_until, hours } => format!(
            "{} ({hours} h) {} {}",
            t("city guard"),
            t("until"),
            busy_until.format("%H:%M")
        ),
        CurrentAction::Expedition => t("expedition").to_string(),
        CurrentAction::Unknown(_) => t("something unknown").to_string(),
    };
    col = col.push(row!(
        text(t("Currently doing:"))
            .size(12)
            .width(Length::FillPortion(1)),
        text(doing)
            .size(12)
            .width(Length::FillPortion(1))
            .horizontal_alignment(iced::alignment::Horizontal::Right)
    ));
    let gi = &player.guard_info;
    let on_duty =
        matches!(tavern.current_action, CurrentAction::CityGuard { .. });
    col = col.push(
        row!(
            checkbox(t("Work as city guard automatically"), gi.auto)
                .style(crate::ui::style::check())
                .on_toggle(move |state| Message::GuardSetAuto { ident, state })
                .size(18)
                .text_size(13),
            crate::ui::num_field(gi.hours, 10u8, move |hours| {
                Message::GuardSetHours { ident, hours }
            }),
            text(t("hours per shift")).size(12),
        )
        .spacing(8)
        .align_items(Alignment::Center),
    );
    col = col.push(
        row!(
            checkbox(t("End the last shift before"), gi.cutoff_enabled)
.style(crate::ui::style::check())
                .on_toggle(move |state| Message::GuardSetCutoffEnabled {
                    ident,
                    state
                })
                .size(16)
                .text_size(13),
            iced::widget::text_input("23:30", &gi.cutoff).font(crate::ui::style::FAT)
                .on_input(move |text| Message::GuardSetCutoff { ident, text })
                .size(12)
                .padding(4)
                .width(Length::Fixed(70.0)),
            text(t("(shorter shift if needed; nothing starts between that time and midnight)"))
                .size(11),
        )
        .spacing(8)
        .align_items(Alignment::Center),
    );
    let shift_button = if on_duty {
        button(text(t("Stop the shift")).size(12))
            .padding(4)
            .style(crate::ui::style::danger())
            .on_press(Message::GuardCancel { ident })
    } else {
        button(text(t("Start a shift now")).size(12))
            .padding(4)
            .style(crate::ui::style::quiet())
            .on_press(Message::GuardTimer { ident })
    };
    col = col.push(shift_button);
    col = col.push(
        text(t(
            "Quests and expeditions come first: a shift starts only once the \
             thirst for adventure is used up (or when expeditions are not \
             automated). A finished shift is collected and the next one \
             starts right away. Everything else - arena, dungeons, pets, \
             fortress, underworld, shops, attributes - keeps running during \
             the shift; only the tavern waits.",
        ))
        .size(11)
        .style(iced::theme::Text::Color(crate::ui::style::MUTED)),
    );
    col = col.push(
        text(format!(
            "{} {} {}",
            t("Wage:"),
            crate::ui::gold_str(tavern.guard_wage),
            t("gold per hour")
        ))
        .size(11)
        .style(iced::theme::Text::Color(crate::ui::style::MUTED)),
    );
    if let Some(status) = &gi.status {
        col = col.push(
            text(status)
                .size(11)
                .style(theme::Text::Color(Color::from_rgb(0.95, 0.80, 0.40))),
        );
    }
    if !gi.log.is_empty() {
        let mut log = column!().spacing(3).padding(5);
        for (time, msg, ok) in gi.log.iter().rev().take(20) {
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
        col = col.push(scrollable(log).height(Length::Fixed(90.0)));
    }

    crate::ui::style::page(crate::ui::style::panel(t("City guard"), col))
}
