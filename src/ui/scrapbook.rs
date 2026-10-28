use crate::i18n::t;
use chrono::Local;
use iced::{
    Alignment, Color, Element, Length,
    alignment::Horizontal,
    theme,
    widget::{
        Image, button, column, horizontal_rule, horizontal_space,
        pick_list, row, scrollable, text_input, vertical_space,
    },
};
use super::glyphs::ticked as checkbox;
// His lettering stands in for the toolkit's `text`, so every line on
// this screen is set in the face the purse card is
use super::glyphs::lettered as text;
use num_format::ToFormattedString;

use super::{remaining_minutes, view_crawling};
use crate::{
    AttackTarget, ClassImages,
    config::{Config, TargetOrder},
    message::Message,
    player::{AccountInfo, AccountStatus},
    server::ServerInfo,
    util::LockExt,
};

const LEFT_WIDTH: f32 = 240.0;

/// Green -> red, depending on how strong the opponent is compared to us
fn risk_color(ratio: f32) -> Color {
    match ratio {
        r if r <= 0.6 => Color::from_rgb(0.29, 0.69, 0.36),
        r if r <= 0.9 => Color::from_rgb(0.85, 0.72, 0.20),
        _ => Color::from_rgb(0.83, 0.33, 0.33),
    }
}

fn info_row(name: &str, value: String) -> Element<'_, Message> {
    row!(
        text(name).size(12).width(Length::FillPortion(1)),
        text(value)
            .size(12)
            .width(Length::FillPortion(1))
            .horizontal_alignment(Horizontal::Right)
    )
    .into()
}

pub fn view_scrapbook<'a>(
    server: &'a ServerInfo,
    player: &'a AccountInfo,
    config: &'a Config,
    images: &'a ClassImages,
) -> Element<'a, Message> {
    let lock = player.status.lock_safe();

    let gs = match &*lock {
        AccountStatus::LoggingIn => {
            return text(t("Logging in")).size(20).into();
        }
        AccountStatus::Idle(_, gs) => gs,
        AccountStatus::Busy(gs, _) => gs,
        AccountStatus::FatalError(err) => {
            return text(format!("Error: {err}")).size(20).into();
        }
        AccountStatus::LoggingInAgain => {
            return text(t("Logging in again").to_string()).size(20).into();
        }
    };

    let Some(si) = &player.scrapbook_info else {
        return text(t("Player does not have a scrapbook")).size(20).into();
    };

    let aid = player.ident;
    let own_stats = si.own_stats.max(1) as f32;

    // ---------------------------------------------------------------- left
    let mut left_col = column!().align_items(Alignment::Center).spacing(8);

    left_col = left_col.push(text(t("Character")).size(15).width(Length::Fill));
    left_col = left_col.push(info_row(
        t("Mushrooms:"),
        gs.character.mushrooms.to_string(),
    ));
    left_col =
        left_col.push(info_row(t("Level:"), gs.character.level.to_string()));
    left_col = left_col.push(info_row(
        t("Items found:"),
        si.scrapbook
            .items
            .len()
            .to_formatted_string(&config.num_format),
    ));
    left_col = left_col.push(info_row(
        t("Attributes:"),
        si.own_stats.to_formatted_string(&config.num_format),
    ));

    match &gs.arena.next_free_fight {
        Some(x) if *x >= Local::now() => {
            left_col = left_col
                .push(info_row(t("Next free fight:"), remaining_minutes(*x)));
        }
        _ => {
            left_col = left_col
                .push(info_row(t("Next free fight:"), t("now").to_string()))
        }
    };
    // Only the first ten wins of a day give XP - the number the player
    // actually plans around
    let xp_left = 10u8.saturating_sub(gs.arena.fights_for_xp);
    left_col = left_col.push(info_row(
        t("XP fights today:"),
        format!("{}/10 ({xp_left} {})", gs.arena.fights_for_xp, t("left")),
    ));

    left_col = left_col.push(horizontal_rule(1));
    left_col =
        left_col.push(text(t("Target limits")).size(15).width(Length::Fill));

    let max_lvl = crate::ui::num_field(si.max_level, 9999, move |nv| {
        Message::PlayerSetMaxLvl {
            ident: aid,
            max: nv,
        }
    });
    left_col = left_col.push(
        row!(text(t("Max level:")).size(12), horizontal_space(), max_lvl)
            .align_items(Alignment::Center),
    );

    let max_attributes =
        crate::ui::num_field(si.max_attributes, 9_999_999, move |nv| {
            Message::PlayerSetMaxAttributes {
                ident: aid,
                max: nv,
            }
        });
    left_col = left_col.push(
        row!(
            text(t("Max attributes:")).size(12),
            horizontal_space(),
            max_attributes
        )
        .align_items(Alignment::Center),
    );

    // How much stronger than us we are willing to fight
    let limit_ratio = si.max_attributes as f32 / own_stats;
    left_col = left_col.push(
        text(format!("= {:.0}% of your attributes", limit_ratio * 100.0))
            .size(11)
            .style(theme::Text::Color(risk_color(limit_ratio)))
            .width(Length::Fill),
    );

    left_col = left_col.push(horizontal_rule(1));

    left_col = left_col.push(
        checkbox(t("Auto battle"), si.auto_battle)
            .style(crate::ui::style::check())
            .on_toggle(move |a| Message::AutoBattle {
                ident: aid,
                state: a,
            })
            .size(18)
            .text_size(13),
    );
    left_col = left_col.push(
        checkbox(t("Spend mushrooms on extra arena fights"), si.mushroom)
            .style(crate::ui::style::check())
            .on_toggle(move |a| Message::ArenaSetMushroom {
                ident: aid,
                state: a,
            })
            .size(16)
            .text_size(12),
    );

    if let Some(status) = &si.auto_status {
        left_col = left_col.push(text(status).size(11).width(Length::Fill));
    }

    let (wins, losses, items) = si.session_stats();
    if wins + losses > 0 {
        left_col = left_col.push(
            text(format!("Session: {wins}W / {losses}L / +{items} items"))
                .size(11)
                .width(Length::Fill),
        );
    }

    left_col = left_col.push(
        button(text(t("Copy battle order")).size(12))
            .padding(4)
            .on_press(Message::CopyBattleOrder { ident: aid }),
    );

    if !si.attack_log.is_empty() {
        let mut log = column!().padding(5).spacing(4);

        for (time, target, won) in si.attack_log.iter().rev().take(50) {
            let time =
                text(format!("{}", time.time().format("%H:%M"))).size(11);
            let mut info = titlecase::titlecase(&target.info.name);
            if *won {
                info.push_str(&format!(" (+{})", target.missing));
            }
            let entry = button(
                row!(text(info).size(11), horizontal_space(), time)
                    .align_items(Alignment::Center),
            )
            .style(match won {
                true => crate::ui::style::primary(),
                false => crate::ui::style::danger(),
            });
            log = log.push(entry.padding(4));
        }

        left_col = left_col.push(scrollable(log).height(Length::Fixed(160.0)));
    }

    if !si.blacklist.is_empty() {
        let blocked = si
            .blacklist
            .values()
            .filter(|(_, lost)| *lost >= config.blacklist_threshold.max(1))
            .count();
        left_col = left_col.push(
            row!(
                text(format!("Skipped targets: {blocked}")).size(11),
                horizontal_space(),
                button(text(t("Reset")).size(11))
                    .padding(3)
                    .style(crate::ui::style::danger())
                    .on_press(Message::PlayerClearBlacklist { ident: aid })
            )
            .align_items(Alignment::Center),
        );
    }

    left_col = left_col.push(vertical_space());
    left_col = left_col.push(horizontal_rule(1));
    left_col = left_col.push(view_crawling(server, config));

    // --------------------------------------------------------------- right
    let filter_input = text_input(t("Search name..."), &si.filter).font(crate::ui::style::FAT)
        .on_input(move |nv| Message::PlayerSetFilter {
            ident: aid,
            filter: nv,
        })
        .size(13)
        .padding(4)
        .width(Length::Fixed(180.0));

    let order_picker = pick_list(TargetOrder::ALL, Some(si.order), move |nv| {
        Message::PlayerSetOrder {
            ident: aid,
            order: nv,
        }
    })
    .text_size(13)
    .padding(4);

    let plan_toggle = checkbox(t("Battle plan"), si.show_plan)
        .style(crate::ui::style::check())
        .on_toggle(move |nv| Message::PlayerShowPlan {
            ident: aid,
            show: nv,
        })
        .size(16)
        .text_size(13);

    let mut toolbar = row!(filter_input, order_picker, plan_toggle)
        .spacing(10)
        .align_items(Alignment::Center);

    if si.show_plan {
        toolbar = toolbar.push(
            button(text(t("Recalculate")).size(13))
                .padding(4)
                .on_press(Message::PlayerComputePlan { ident: aid }),
        );
    }

    let header = row!(
        text(t("#"))
            .size(12)
            .width(Length::Fixed(30.0))
            .horizontal_alignment(Horizontal::Center),
        text("").width(Length::Fixed(70.0)),
        text(t("Items"))
            .size(12)
            .width(Length::FillPortion(4))
            .horizontal_alignment(Horizontal::Center),
        text(t("Level"))
            .size(12)
            .width(Length::FillPortion(4))
            .horizontal_alignment(Horizontal::Center),
        text(t("Attributes"))
            .size(12)
            .width(Length::FillPortion(5))
            .horizontal_alignment(Horizontal::Center),
        text(t("Risk"))
            .size(12)
            .width(Length::FillPortion(4))
            .horizontal_alignment(Horizontal::Center),
        text(t("Name")).size(12).width(Length::FillPortion(14)),
        text("").width(Length::Fixed(30.0)),
    )
    .spacing(4)
    .align_items(Alignment::Center);

    let source: &Vec<AttackTarget> =
        if si.show_plan { &si.plan } else { &si.best };

    let filter = si.filter.trim().to_lowercase();
    let mut shown = 0;
    let mut cumulative = 0;
    let mut target_list = column!().spacing(6);

    for (pos, v) in source.iter().enumerate() {
        cumulative += v.missing;
        if !filter.is_empty() && !v.info.name.to_lowercase().contains(&filter) {
            continue;
        }
        shown += 1;

        let mut name_part = row!()
            .align_items(Alignment::Center)
            .spacing(5)
            .width(Length::FillPortion(14));

        if let Some(class) = v.info.class
            && config.show_class_icons
        {
            name_part = name_part.push(
                Image::new(images.get_handle(class))
                    .width(Length::Fixed(18.0))
                    .content_fit(iced::ContentFit::ScaleDown),
            );
        }
        name_part = name_part.push(
            text(titlecase::titlecase(&v.info.name))
                .width(Length::Fill)
                .horizontal_alignment(Horizontal::Left),
        );

        let stats = v.info.stats;
        let ratio = stats.map(|s| s as f32 / own_stats);

        let risk: Element<Message> = match ratio {
            Some(r) => text(format!("{:.0}%", r * 100.0))
                .style(theme::Text::Color(risk_color(r)))
                .width(Length::FillPortion(4))
                .horizontal_alignment(Horizontal::Center)
                .into(),
            None => text(t("???"))
                .width(Length::FillPortion(4))
                .horizontal_alignment(Horizontal::Center)
                .into(),
        };

        // In the plan, every entry only lists the items, that the fights
        // before it did not already provide, so the running total is what
        // actually matters
        let items_label = if si.show_plan {
            format!("+{} ({})", v.missing, cumulative)
        } else {
            v.missing.to_string()
        };

        let attack = button(text(t("Attack")).size(12)).padding(4).on_press(
            Message::PlayerAttack {
                ident: aid,
                target: v.to_owned(),
            },
        );

        let skip = button(text(t("x")).size(12))
            .padding(4)
            .style(crate::ui::style::danger())
            .on_press(Message::PlayerSkipTarget {
                ident: aid,
                uid: v.info.uid,
                name: v.info.name.clone(),
            });

        target_list = target_list.push(
            row!(
                text(format!("{}", pos + 1))
                    .size(11)
                    .width(Length::Fixed(30.0))
                    .horizontal_alignment(Horizontal::Center),
                row!(attack).width(Length::Fixed(70.0)),
                text(items_label)
                    .width(Length::FillPortion(4))
                    .horizontal_alignment(Horizontal::Center),
                text(v.info.level)
                    .width(Length::FillPortion(4))
                    .horizontal_alignment(Horizontal::Center),
                text(
                    stats
                        .map(|a| a.to_formatted_string(&config.num_format))
                        .unwrap_or("???".to_string())
                )
                .width(Length::FillPortion(5))
                .horizontal_alignment(Horizontal::Center),
                risk,
                name_part,
                row!(skip).width(Length::Fixed(30.0)),
            )
            .spacing(4)
            .align_items(Alignment::Center),
        );
    }

    let list: Element<Message> = if shown == 0 {
        let hint = if !filter.is_empty() {
            "No target matches the search"
        } else if si.show_plan {
            "No plan yet - press 'Recalculate'"
        } else {
            "No targets found. Crawl more of the HoF, or raise the level / \
             attribute limits"
        };
        column!(vertical_space(), text(hint).size(14), vertical_space())
            .align_items(Alignment::Center)
            .width(Length::Fill)
            .into()
    } else {
        scrollable(target_list).height(Length::Fill).into()
    };

    // ------------------------------------------------- the game's offer
    // What the arena screen in the game shows: three opponents, their five
    // attributes, and how they compare to ours. Always there, whether or not
    // the crawler has found anybody
    let offer_panel = {
        use sf_api::command::AttributeType as A;
        let own = [
            A::Strength,
            A::Dexterity,
            A::Intelligence,
            A::Constitution,
            A::Luck,
        ]
        .map(|a| {
            gs.character.attribute_basis[a]
                + gs.character.attribute_additions[a]
        });
        let names = [
            "Strength", "Dexterity", "Intelligence", "Constitution", "Luck",
        ];
        let free_now =
            gs.arena.next_free_fight.is_none_or(|x| x <= Local::now());
        let muted = theme::Text::Color(crate::ui::style::MUTED);
        let good = Color::from_rgb(0.36, 0.78, 0.45);
        let bad = Color::from_rgb(0.90, 0.45, 0.45);

        let status_txt = if si.arena_offer_busy {
            t("fetching...").to_string()
        } else {
            match si.arena_offer_at {
                Some(at) => format!("{} {}", t("as of"), at.format("%H:%M")),
                None => t("not fetched yet").to_string(),
            }
        };
        // The countdown next to the offer, ticking with the UI - it was
        // only in the left column, where nobody looked while picking a fight
        let fight_txt = match gs.arena.next_free_fight {
            Some(x) if x > Local::now() => {
                format!("{} {}", t("next fight in"), remaining_minutes(x))
            }
            _ => t("free fight now").to_string(),
        };
        let head = row!(
            text(t("The game's offer")).size(14),
            text(status_txt).size(11).style(muted),
            text(fight_txt)
                .size(11)
                .style(theme::Text::Color(if free_now {
                    good
                } else {
                    crate::ui::style::MUTED
                })),
            horizontal_space(),
            button(text(t("Refresh")).size(11))
                .padding([3, 8])
                .style(crate::ui::style::quiet())
                .on_press_maybe(if si.arena_offer_busy {
                    None
                } else {
                    Some(Message::ArenaOfferRefresh {
                        ident: aid,
                        then_fight: false,
                    })
                }),
        )
        .spacing(8)
        .align_items(Alignment::Center)
        .width(Length::Fill);

        let mut cards = row!().spacing(10).width(Length::Fill);
        if si.arena_offer.is_empty() {
            cards = cards.push(
                text(t(
                    "Press Refresh - the game always has three opponents ready",
                ))
                .size(12)
                .style(muted),
            );
        }
        for o in &si.arena_offer {
            // The simulated chance decides, not the attribute sum
            let beatable = o.win >= 0.5;
            let chance = format!("{:.0} %", o.win * 100.0);
            let mut card = column!().spacing(3).width(Length::Fill);
            card = card.push(
                row!(
                    text(&o.name).size(13),
                    horizontal_space(),
                    text(format!("LVL {}", o.level)).size(12).style(muted),
                )
                .align_items(Alignment::Center),
            );
            card = card.push(
                text(format!(
                    "{:?}{}",
                    o.class,
                    o.guild
                        .as_deref()
                        .map(|g| format!(" \u{00B7} [{g}]"))
                        .unwrap_or_default()
                ))
                .size(11)
                .style(muted),
            );
            card = card.push(horizontal_rule(1));
            for i in 0..5 {
                let diff = i64::from(own[i]) - i64::from(o.attrs[i]);
                card = card.push(
                    row!(
                        text(t(names[i])).size(11).width(Length::Fill),
                        text(
                            o.attrs[i].to_formatted_string(&config.num_format)
                        )
                        .size(11)
                        .width(Length::Fixed(58.0))
                        .horizontal_alignment(Horizontal::Right),
                        text(format!("{diff:+}"))
                            .size(11)
                            .style(theme::Text::Color(if diff >= 0 {
                                good
                            } else {
                                bad
                            }))
                            .width(Length::Fixed(58.0))
                            .horizontal_alignment(Horizontal::Right),
                    )
                    .spacing(6),
                );
            }
            let tdiff = i64::from(si.own_stats) - i64::from(o.stats);
            card = card.push(
                row!(
                    text(t("Total")).size(11).width(Length::Fill),
                    text(o.stats.to_formatted_string(&config.num_format))
                        .size(11)
                        .width(Length::Fixed(58.0))
                        .horizontal_alignment(Horizontal::Right),
                    text(format!("{tdiff:+}"))
                        .size(11)
                        .style(theme::Text::Color(if tdiff >= 0 {
                            good
                        } else {
                            bad
                        }))
                        .width(Length::Fixed(58.0))
                        .horizontal_alignment(Horizontal::Right),
                )
                .spacing(6),
            );
            card = card.push(
                row!(
                    text(format!("{} {chance}", t("win chance:")))
                        .size(12)
                        .style(theme::Text::Color(if o.win >= 0.7 {
                            good
                        } else if beatable {
                            Color::from_rgb(0.85, 0.72, 0.20)
                        } else {
                            bad
                        })),
                    horizontal_space(),
                    button(text(t("Fight")).size(11))
                        .padding([3, 10])
                        .style(if beatable {
                            crate::ui::style::primary()
                        } else {
                            crate::ui::style::quiet()
                        })
                        .on_press_maybe(if free_now {
                            Some(Message::ArenaFightOffer {
                                ident: aid,
                                pid: o.pid,
                            })
                        } else {
                            None
                        }),
                )
                .align_items(Alignment::Center),
            );
            cards = cards.push(
                iced::widget::container(card)
                    .padding(10)
                    .width(Length::Fill)
                    .style(crate::ui::style::card()),
            );
        }
        column!(head, cards).spacing(6).width(Length::Fill)
    };

    let right_col = column!(
        offer_panel,
        horizontal_rule(1),
        toolbar,
        header,
        horizontal_rule(1),
        list
    )
    .spacing(8)
    .padding([0, 5]);

    row!(
        left_col.width(Length::Fixed(LEFT_WIDTH)),
        right_col.width(Length::Fill)
    )
    .spacing(15)
    .padding(15)
    .height(Length::Fill)
    .align_items(Alignment::Start)
    .into()
}
