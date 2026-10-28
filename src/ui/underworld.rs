use crate::i18n::t;
use iced::{
    Alignment, Color, Element, Length,
    alignment::Horizontal,
    widget::{
        Image, button, column, container, horizontal_space,
        pick_list, row, scrollable, },
};
use super::glyphs::ticked as checkbox;
// His lettering stands in for the toolkit's `text`, so every line on
// this screen is set in the face the purse card is
use super::glyphs::lettered as text;

use super::{style, view_crawling};
use crate::util::{LockExt, split_camel};
use crate::{
    ClassImages,
    config::Config,
    message::Message,
    player::{AccountInfo, AccountStatus},
    server::ServerInfo,
};

pub fn view_underworld<'a>(
    server: &'a ServerInfo,
    player: &'a AccountInfo,
    config: &'a Config,
    images: &'a ClassImages,
) -> Element<'a, Message> {
    use sf_api::gamestate::underworld::{
        UnderworldBuildingType as B, UnderworldResourceType,
        UnderworldUnitType as U,
    };

    let lock = player.status.lock_safe();
    let gs = match &*lock {
        AccountStatus::LoggingIn => {
            return text(t("Loggin in")).size(20).into();
        }
        AccountStatus::Idle(_, gs) => gs,
        AccountStatus::Busy(gs, _) => gs,
        AccountStatus::FatalError(err) => {
            return text(format!("Error: {err}")).size(20).into();
        }
        AccountStatus::LoggingInAgain => {
            return text(t("Logging in player again").to_string())
                .size(20)
                .into();
        }
    };

    let Some(info) = &player.underworld_info else {
        return text(t("Underworld not unlocked yet").to_string())
            .size(20)
            .into();
    };

    // Always draw from the LIVE game state. `info.underworld` is a copy taken
    // at login and only refreshed after a lure fight, so it sat on the
    // login-time numbers (often zeros, before the game had sent the
    // underworld at all) while the automation - which reads `gs` - kept
    // upgrading buildings the page never showed moving. The copy is only the
    // fallback for the moment the live state has no underworld yet.
    let uw = gs.underworld.as_ref().unwrap_or(&info.underworld);
    let now = chrono::Local::now();
    let muted = iced::theme::Text::Color(style::MUTED);
    let aid = player.ident;
    let lures_max = crate::mechanics::daily_lures(uw).max(1);

    // ------------------------------------------------------------- tiles
    // The numbers a leader glances at: big, on one line across the top,
    // instead of squeezed into a 290 px strip whose right half was clipped
    // away (which is why "Souls:" and the prices looked empty)
    let tile = |label: &'static str, value: String, color: Option<Color>| {
        style::tile(t(label), value, color.unwrap_or(style::NAME_GOLD))
    };
    let lured = tile(
        "Lured today",
        format!("{} / {}", uw.lured_today, lures_max),
        Some(if uw.lured_today >= lures_max {
            style::MUTED
        } else {
            style::GOOD
        }),
    );
    let souls_tile = tile("Souls", human_souls(uw.souls_current), None);
    // The Soul Extractor: what has piled up in it (estimated from the last
    // server figure plus the hourly rate, the same way the collector decides),
    // against its own store cap - the collector takes it somewhere between a
    // third and nine tenths full, a different share every time
    let extractor = {
        let prod = &uw.production[UnderworldResourceType::Souls];
        if prod.limit > 0 {
            let since_h = uw
                .last_collectable_update
                .map(|t| (now - t).num_seconds().max(0) as f64 / 3600.0)
                .unwrap_or(0.0);
            let est = (prod.last_collectable as f64
                + prod.per_hour as f64 * since_h)
                .min(prod.limit as f64) as u64;
            style::tile_sub(
                t("Soul Extractor"),
                format!("~{}", human_souls(est)),
                if est * 2 >= prod.limit {
                    style::GOOD
                } else {
                    style::NAME_GOLD
                },
                Some(format!(
                    "/ {}  \u{00B7}  {}/h",
                    human_souls(prod.limit),
                    human_souls(prod.per_hour)
                )),
            )
        } else {
            tile("Soul Extractor", "-".into(), None)
        }
    };
    let building = match (uw.upgrade_building, uw.upgrade_finish) {
        (Some(b), Some(fin)) => {
            let left = fin - now;
            let when = if left.num_seconds() <= 0 {
                t("done - waiting to be taken").to_string()
            } else {
                format!("{}h {:02}m", left.num_hours(), left.num_minutes() % 60)
            };
            style::tile_sub(
                t("Upgrading"),
                when,
                style::WARN,
                Some(split_camel(&format!("{b:?}"))),
            )
        }
        _ => tile("Upgrading", t("nothing right now").to_string(), None),
    };
    // The game's own number: anybody at or above this level pays the full
    // pile of souls, anybody below has them docked. It is what the auto
    // lure aims at - the weakest name on the crawled list that still
    // clears it, because the souls are the same above it and the fight is
    // easier - so it belongs on screen next to the others
    let bar = tile(
        "Full souls from level",
        uw.lure_level.to_string(),
        Some(style::GOOD),
    );
    let tiles = row!(lured, souls_tile, bar, extractor, building)
        .spacing(8)
        .width(Length::Fill);

    // --------------------------------------------------------- left card
    // Buildings and units as a real table: every row fills the card, so the
    // price column always lands inside it
    let table_row = |name: String, level: String, extra: String, odd: bool| {
        // Level and price get fixed columns so a long name can never push
        // them off the card; the name takes whatever is left
        let r = row!(
            text(name).size(12).width(Length::Fill),
            text(level)
                .size(12)
                .width(Length::Fixed(40.0))
                .horizontal_alignment(Horizontal::Right),
            text(extra)
                .size(11)
                .style(muted)
                .width(Length::Fixed(124.0))
                .horizontal_alignment(Horizontal::Right),
        )
        .spacing(8)
        .align_items(Alignment::Center)
        .width(Length::Fill);
        let c = container(r).padding([3, 6]).width(Length::Fill);
        if odd { c.style(style::stripe()) } else { c }
    };

    let mut buildings = column!().spacing(3).width(Length::Fill);
    // In the order the automation builds them - which depends on the
    // phase - with the next level's price, so the collect-then-upgrade
    // loop can be watched working
    let heart = uw.buildings[B::HeartOfDarkness].level;
    let phase = crate::mechanics::underworld_phase(info.phase, heart);
    buildings = buildings.push(style::note(format!(
        "{} {}",
        t("Build order now:"),
        crate::mechanics::UnderworldPhase::to_string(&phase)
    )));
    for (i, b) in crate::mechanics::underworld_order(phase)
        .into_iter()
        .enumerate()
    {
        let bd = &uw.buildings[b];
        let cost = &bd.upgrade_cost;
        buildings = buildings.push(table_row(
            split_camel(&format!("{b:?}")),
            format!("L{}", bd.level),
            format!(
                "{} s / {} g",
                human_souls(cost.souls),
                human_souls(cost.silver / 100)
            ),
            i % 2 == 1,
        ));
    }
    let mut units = column!().spacing(3).width(Length::Fill);
    for (i, u) in [U::Keeper, U::Troll, U::Goblin].into_iter().enumerate() {
        let ud = &uw.units[u];
        units = units.push(table_row(
            format!("{u:?}"),
            format!("L{}", ud.level),
            format!(
                "\u{2192} L{}  {} s",
                ud.upgrade_next_lvl,
                human_souls(ud.upgrade_cost.souls)
            ),
            i % 2 == 1,
        ));
    }

    let mut auto = column!().spacing(6).width(Length::Fill);
    let max_lvl = crate::ui::num_field(info.max_level, 9999, move |nv| {
        Message::PlayerSetMaxUndergroundLvl {
            ident: aid,
            lvl: nv,
        }
    });
    auto = auto.push(
        row!(text(t("Max Level:")).size(12), horizontal_space(), max_lvl)
            .align_items(Alignment::Center)
            .width(Length::Fill),
    );
    auto = auto.push(
        checkbox(t("Auto Lure"), info.auto_lure)
            .style(crate::ui::style::check())
            .on_toggle(move |a| Message::AutoLure {
                ident: aid,
                state: a,
            })
            .size(18)
            .text_size(13),
    );
    auto = auto.push(
        checkbox(t("Auto collect gold and souls"), info.auto_collect)
            .style(crate::ui::style::check())
            .on_toggle(move |a| Message::UnderworldAutoCollect {
                ident: aid,
                state: a,
            })
            .size(18)
            .text_size(13),
    );
    auto = auto.push(
        checkbox(t("Auto build"), info.auto_build)
            .style(crate::ui::style::check())
            .on_toggle(move |a| Message::UnderworldAutoBuild {
                ident: aid,
                state: a,
            })
            .size(18)
            .text_size(13),
    );
    // The two phases of the guild leader's build order (see `mechanics`)
    auto = auto.push(
        row!(
            text(t("Build for:")).size(12),
            pick_list(
                crate::mechanics::UnderworldPhase::ALL,
                Some(info.phase),
                move |phase| Message::UnderworldSetPhase { ident: aid, phase }
            )
            .text_size(12)
            .padding(3),
        )
        .spacing(8)
        .align_items(Alignment::Center),
    );
    auto = auto.push(
        text(t(
            "Three buildings and one unit: the Heart of Darkness, the Soul Extractor, the Gladiator Trainer - and a level on the Keeper now and then. The Heart caps the level of every other building, so it only goes up when something has hit that cap. The souls are held for whichever of the three is next; everything else, the Keeper included, only ever spends the surplus above it. The rest of the buildings are a tail for a maxed-out underworld, not a plan.",
        ))
        .size(11)
        .style(iced::theme::Text::Color(style::MUTED)),
    );
    // The one line that answers "why is nothing being built": what the
    // souls are for, how far off it is and how long that takes. Bigger
    // and in colour rather than grey - grey is for furniture, and this is
    // the page's answer
    if let Some(status) = &info.maintain_status {
        let saving = status.starts_with("Saving");
        auto =
            auto.push(text(status).size(13).style(iced::theme::Text::Color(
                if saving { style::WARN } else { style::MUTED },
            )));
    }
    auto = auto.push(
        button(text(t("Copy Targets")).size(12))
            .padding([5, 10])
            .style(style::brass(false))
            .on_press(Message::CopyBestLures { ident: aid }),
    );

    let mut lure_log = None;
    if !info.attack_log.is_empty() {
        let mut log = column!().spacing(3).width(Length::Fill);
        for (time, target, won) in info.attack_log.iter().rev() {
            log = log.push(
                button(
                    row!(
                        text(target).size(12),
                        horizontal_space(),
                        text(time.time().format("%H:%M").to_string()).size(12)
                    )
                    .width(Length::Fill),
                )
                .style(if *won {
                    style::primary()
                } else {
                    style::danger()
                })
                .padding([3, 7])
                .width(Length::Fill),
            );
        }
        lure_log = Some(style::panel(
            t("Lure log"),
            scrollable(log).height(Length::Fixed(160.0)),
        ));
    }

    // One framed panel per block, stacked - the page reads as four things
    // rather than as one column with rules across it
    let mut left = column!(
        style::panel(t("Buildings"), buildings),
        style::panel(t("Units"), units),
        style::panel(t("Automation"), auto),
    )
    .spacing(10)
    .width(Length::Fill);
    if let Some(log) = lure_log {
        left = left.push(log);
    }
    // Right padding keeps the last column out from under the scrollbar,
    // which iced draws over the content
    let left_card = container(
        scrollable(container(left).padding([0, 12, 0, 0])).height(Length::Fill),
    )
    .width(Length::FillPortion(5))
    .height(Length::Fill);

    // -------------------------------------------------------- right card
    let mut right = column!()
        .spacing(8)
        .width(Length::Fill)
        .height(Length::Fill);
    right = right.push(view_crawling(server, config));
    let can_lure = uw.lured_today < lures_max;
    if info.best.is_empty() {
        right = right.push(
            text(t("No targets yet - the Hall of Fame is still being read"))
                .size(12)
                .style(muted),
        );
    } else {
        let head = row!(
            text("").width(Length::FillPortion(1)),
            text(t("Level"))
                .size(12)
                .style(muted)
                .width(Length::FillPortion(1))
                .horizontal_alignment(Horizontal::Center),
            text(t("Items"))
                .size(12)
                .style(muted)
                .width(Length::FillPortion(1))
                .horizontal_alignment(Horizontal::Center),
            text(t("Name"))
                .size(12)
                .style(muted)
                .width(Length::FillPortion(4)),
        )
        .spacing(6)
        .width(Length::Fill);
        right = right.push(head);

        let mut list = column!().spacing(4).width(Length::Fill);
        for (i, v) in info.best.iter().enumerate() {
            let mut who = row!()
                .spacing(6)
                .align_items(Alignment::Center)
                .width(Length::FillPortion(4));
            if let Some(class) = v.class
                && config.show_class_icons
            {
                who = who.push(
                    Image::new(images.get_handle(class))
                        .width(Length::Fixed(22.0))
                        .content_fit(iced::ContentFit::ScaleDown),
                );
            }
            who = who.push(text(&v.name).size(12));
            let lure = button(text(t("Lure")).size(11))
                .padding([3, 8])
                .on_press_maybe(if can_lure {
                    Some(Message::PlayerLure {
                        ident: aid,
                        target: LureTarget {
                            uid: v.uid,
                            name: v.name.clone(),
                        },
                    })
                } else {
                    None
                });
            let r = row!(
                container(lure).width(Length::FillPortion(1)).center_x(),
                text(v.level)
                    .size(12)
                    .width(Length::FillPortion(1))
                    .horizontal_alignment(Horizontal::Center),
                text(v.equipment.len())
                    .size(12)
                    .width(Length::FillPortion(1))
                    .horizontal_alignment(Horizontal::Center),
                who,
            )
            .spacing(6)
            .align_items(Alignment::Center)
            .width(Length::Fill);
            let c = container(r).padding([3, 4]).width(Length::Fill);
            list = list.push(if i % 2 == 1 {
                c.style(style::stripe())
            } else {
                c
            });
        }
        right = right.push(scrollable(list).height(Length::Fill));
    }
    let right_card = style::panel(t("Lure targets"), right)
        .width(Length::FillPortion(6))
        .height(Length::Fill);

    let panels = row!(left_card, right_card)
        .spacing(10)
        .width(Length::Fill)
        .height(Length::Fill);
    container(
        column!(tiles, panels)
            .spacing(10)
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .padding([12, 14])
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

#[derive(Debug, Clone)]
pub struct LureTarget {
    pub uid: u32,
    pub name: String,
}

/// A compact soul count: 1.2 K, 3.4 M, 5.6 B. Kept here rather than reaching
/// into message.rs's money formatter, so the pets/underworld page has no
/// cross-module dependency for one label
#[must_use]
fn human_souls(v: u64) -> String {
    const UNITS: [(&str, u64); 3] =
        [("B", 1_000_000_000), ("M", 1_000_000), ("K", 1_000)];
    for (name, size) in UNITS {
        if v >= size {
            return format!("{:.1} {name}", v as f64 / size as f64);
        }
    }
    v.to_string()
}
