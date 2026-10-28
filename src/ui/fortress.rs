use crate::i18n::t;
use iced::{
    Alignment, Element, Length,
    widget::{
        button, column, horizontal_space, pick_list, row, scrollable, },
};
use super::glyphs::ticked as checkbox;
// His lettering stands in for the toolkit's `text`, so every line on
// this screen is set in the face the purse card is
use super::glyphs::lettered as text;
use sf_api::gamestate::fortress::FortressUnitType;

use crate::{
    message::{FortressOption, Message},
    player::{AccountInfo, AccountStatus},
    ui::style,
    util::LockExt,
};

/// A preset for the wait ceiling, in minutes, shown the way a person
/// says it: "15 min", "2 h", "24 h"
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Wait(u32);

const WAITS: [Wait; 9] = [
    Wait(15),
    Wait(30),
    Wait(60),
    Wait(120),
    Wait(240),
    Wait(480),
    Wait(720),
    Wait(1440),
    Wait(2880),
];

impl std::fmt::Display for Wait {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0 < 60 {
            write!(f, "{} min", self.0)
        } else {
            write!(f, "{} h", self.0 / 60)
        }
    }
}

pub fn view_fortress(player: &AccountInfo) -> Element<'_, Message> {
    let lock = player.status.lock_safe();

    // The guard is taken exactly once - locking the same mutex again further
    // down would deadlock the whole UI
    let (gs, busy_reason) = match &*lock {
        AccountStatus::Idle(_, gs) => (gs, None),
        AccountStatus::Busy(gs, reason) => (gs, Some(reason.to_string())),
        AccountStatus::FatalError(err) => {
            return text(format!("Error: {err}")).size(20).into();
        }
        _ => return text(t("Logging in")).size(20).into(),
    };

    let Some(fortress) = gs.fortress.as_ref() else {
        return text(t("This character does not have a fortress"))
            .size(20)
            .into();
    };

    let Some(fi) = &player.fortress_info else {
        return text(t("No fortress data")).size(20).into();
    };

    let ident = player.ident;
    let unit = |u: FortressUnitType| &fortress.units[u];
    let soldiers = unit(FortressUnitType::Soldier).count;
    let mages = unit(FortressUnitType::Magician).count;
    let archers = unit(FortressUnitType::Archer).count;

    // ------------------------------------------------- the army, at a glance
    // Six of the game's own number boxes across the top. A garrison is a set
    // of counts against their limits; six `label: value` rows made that a
    // paragraph to read instead of a barracks to look at
    let full = |have: u16, limit: u16| {
        if have >= limit {
            style::GOOD
        } else {
            style::NAME_GOLD
        }
    };
    let army = row!(
        style::tile_sub(
            t("Soldiers"),
            soldiers.to_string(),
            full(soldiers, unit(FortressUnitType::Soldier).limit),
            Some(format!("/ {}", unit(FortressUnitType::Soldier).limit)),
        ),
        style::tile_sub(
            t("Archers"),
            archers.to_string(),
            full(archers, unit(FortressUnitType::Archer).limit),
            Some(format!("/ {}", unit(FortressUnitType::Archer).limit)),
        ),
        style::tile_sub(
            t("Mages"),
            mages.to_string(),
            full(mages, unit(FortressUnitType::Magician).limit),
            Some(format!("/ {}", unit(FortressUnitType::Magician).limit)),
        ),
        style::tile_sub(
            t("In training"),
            unit(FortressUnitType::Soldier).in_training.to_string(),
            if unit(FortressUnitType::Soldier).in_training > 0 {
                style::ACCENT
            } else {
                style::MUTED
            },
            Some(t("soldiers")),
        ),
        style::tile(t("Honor"), fortress.honor.to_string(), style::NAME_GOLD),
        style::tile(
            t("Wall level"),
            fortress.wall_combat_lvl.to_string(),
            style::NAME_GOLD,
        ),
    )
    .spacing(8)
    .width(Length::Fill);

    // --------------------------------------------------------------- attacks
    let free_reroll = fortress
        .attack_free_reroll
        .map(|a| a <= chrono::Local::now())
        .unwrap_or(false);

    // The ceiling on the wait after the barracks fill - up to a week, in
    // minutes, because "longer than an hour" was asked for by name. Any
    // number can be typed; the presets beside it are the usual ones
    let notice_max =
        crate::ui::num_field(fi.notice_max_min, 10_080u32, move |nv| {
            Message::FortressSetNoticeMax {
                ident,
                minutes: nv,
            }
        });
    let notice_presets = pick_list(
        WAITS,
        WAITS.iter().copied().find(|w| w.0 == fi.notice_max_min),
        move |w: Wait| Message::FortressSetNoticeMax {
            ident,
            minutes: w.0,
        },
    )
    .placeholder(t("presets"))
    .text_size(12)
    .padding([2, 6])
    .width(Length::Fixed(96.0));
    let margin = crate::ui::num_field(fi.soldier_margin, 50u16, move |nv| {
        Message::FortressSetSoldierMargin { ident, margin: nv }
    });
    let soldiers_per_attack =
        crate::ui::num_field(fi.attack_soldiers, 500u16, move |nv| {
            Message::FortressSetAttackSoldiers { ident, count: nv }
        });

    // Without this it is impossible to tell whether anything is running.
    // The cycle has three states and each says what it is waiting for
    use crate::player::RaidPhase;
    let (activity, activity_colour) = match (&busy_reason, fi.raid) {
        (Some(reason), _) => {
            (format!("{} {reason}", t("Running:")), style::GOOD)
        }
        _ if !fi.auto_attack => {
            (t("Automatic raids are off").to_string(), style::BAD)
        }
        (_, RaidPhase::Raiding { attacks, .. }) => (
            format!("{} {attacks}", t("Raiding - attacks this run:")),
            style::GOOD,
        ),
        (_, RaidPhase::Noticed) => match fi.next_attack() {
            Some(next) if next > chrono::Local::now() => (
                format!(
                    "{} {}",
                    t("Barracks full - raiding in"),
                    super::remaining_minutes(next)
                ),
                style::NAME_GOLD,
            ),
            _ => (t("Raiding any moment now...").to_string(), style::GOOD),
        },
        (_, RaidPhase::Filling) => (
            fi.status
                .clone()
                .unwrap_or_else(|| t("Filling the barracks").to_string()),
            style::MUTED,
        ),
    };

    let mut attacks = column!()
        .spacing(7)
        .width(Length::Fill)
        .push(style::line_in(
            t("Free new opponent:"),
            if free_reroll { t("yes") } else { t("not yet") },
            Some(if free_reroll {
                style::GOOD
            } else {
                style::MUTED
            }),
        ))
        .push(style::line_in(t("Status"), activity, Some(activity_colour)))
        .push(
            checkbox(t("Raid automatically"), fi.auto_attack)
                .style(crate::ui::style::check())
                .on_toggle(move |state| Message::AutoFortress { ident, state })
                .size(17)
                .text_size(13),
        )
        .push(
            row!(
                text(t("After the barracks fill, wait at most (min):"))
                    .size(12),
                horizontal_space(),
                notice_presets,
                notice_max
            )
            .spacing(6)
            .align_items(Alignment::Center),
        )
        .push(style::note(t(
            "No clock. The barracks fill at the game's own training pace; \
             some drawn while after the last soldier is trained - three \
             minutes up to this ceiling, usually early, sometimes the whole \
             way - the raid starts: one opponent after another, each with \
             just the soldiers the game advises, until they run out. An \
             opponent not worth the soldiers is passed over: a free new one, \
             a paid one once if the price is small change, otherwise one \
             soldier is thrown at them. Then archers and mages are topped \
             up, the soldiers are queued back to full, and it waits for \
             them. Only inside a session.",
        )))
        .push(
            checkbox(
                t("Recruit soldiers, archers and mages until full"),
                fi.recruit_all,
            )
            .style(crate::ui::style::check())
            .on_toggle(move |state| Message::FortressSetOption {
                ident,
                option: FortressOption::RecruitAll,
                state,
            })
            .size(17)
            .text_size(13),
        )
        .push(
            checkbox(
                t("Send only as many soldiers as the game recommends"),
                fi.smart_soldiers,
            )
            .style(crate::ui::style::check())
            .on_toggle(move |state| Message::FortressSetOption {
                ident,
                option: FortressOption::SmartSoldiers,
                state,
            })
            .size(17)
            .text_size(13),
        )
        .push(style::note(t(
            "Before every attack the opponent is looked up and the game's own \
             'attack with N soldiers' value is used - the green part of the \
             bar in the game. Archers and mages never attack, they only \
             defend.",
        )))
        .push(
            checkbox(
                t("Send one soldier at opponents that are out of reach"),
                fi.cheap_skip,
            )
            .style(crate::ui::style::check())
            .on_toggle(move |state| Message::FortressSetOption {
                ident,
                option: FortressOption::CheapSkip,
                state,
            })
            .size(17)
            .text_size(13),
        )
        .push(style::note(t(
            "When the game wants more soldiers than the barracks holds, the \
             attack is lost on purpose with a single soldier. That costs one \
             soldier and puts a new opponent in front of you instead of \
             letting the attack timer run out on one you cannot beat.",
        )))
        .push(
            row!(
                text(t("Extra soldiers on top of the recommendation:"))
                    .size(12),
                horizontal_space(),
                margin
            )
            .align_items(Alignment::Center),
        )
        .push(
            row!(
                text(t(
                    "Manual soldiers per attack, when the above is off (0 = \
                     all):"
                ))
                .size(12),
                horizontal_space(),
                soldiers_per_attack
            )
            .align_items(Alignment::Center),
        )
        .push(
            row!(
                button(text(t("Attack now")).size(13))
                    .padding([4, 9])
                    .on_press(Message::FortressAttack { ident }),
                button(text(t("New opponent")).size(13))
                    .padding([4, 9])
                    .style(style::quiet())
                    .on_press(Message::FortressReroll { ident }),
                button(text(t("Recruit soldier")).size(13))
                    .padding([4, 9])
                    .style(style::quiet())
                    .on_press(Message::FortressTrain { ident }),
            )
            .spacing(8),
        );

    if let Some(status) = &fi.status {
        attacks = attacks.push(text(status).size(11));
    }
    attacks = attacks.push(style::note(t(
        "The game decides who your fortress opponent is - you can only take a \
         different one, not pick a specific player.",
    )));

    // ----------------------------------------------------------- housekeeping
    let opt = |label: &'static str, option: FortressOption, on: bool| {
        checkbox(label, on)
            .style(crate::ui::style::check())
            .on_toggle(move |state| Message::FortressSetOption {
                ident,
                option,
                state,
            })
            .size(17)
            .text_size(13)
    };

    let mut keeping = column!()
        .spacing(7)
        .width(Length::Fill)
        .push(opt(
            t("Collect wood / stone / XP when the building is full"),
            FortressOption::Collect,
            fi.auto_collect,
        ))
        .push(opt(
            t("Upgrade buildings (economy > fortress > military)"),
            FortressOption::Build,
            fi.auto_build,
        ));

    // What the fortress is sitting on right now, as boxes rather than as a
    // sentence: a store that has filled up is a store that has stopped
    // earning, and that has to be visible without reading
    let mut stores = row!().spacing(8).width(Length::Fill);
    for (typ, res) in &fortress.resources {
        let est = {
            let since_h = fortress
                .last_collectable_updated
                .map(|t| {
                    (chrono::Local::now() - t).num_seconds().max(0) as f64
                        / 3600.0
                })
                .unwrap_or(0.0);
            (res.production.last_collectable as f64
                + res.production.per_hour as f64 * since_h)
                .min(res.production.limit as f64) as u64
        };
        let limit = res.production.limit as u64;
        // The resource's name reaches us as a Rust type name, which no
        // translation table can reach. Naming the three here is what puts
        // them in the same language as the rest of the screen
        let caption = match format!("{typ:?}").as_str() {
            "Wood" => t("Wood"),
            "Stone" => t("Stone"),
            "Experience" | "Xp" | "XP" => t("Experience"),
            _ => "",
        };
        let caption = if caption.is_empty() {
            format!("{typ:?}")
        } else {
            caption.to_string()
        };
        stores = stores.push(style::tile_sub(
            caption,
            crate::ui::thousands(est),
            if limit > 0 && est >= limit {
                style::BAD
            } else if est > 0 {
                style::GOOD
            } else {
                style::MUTED
            },
            Some(format!("/ {}", crate::ui::thousands(limit))),
        ));
    }
    keeping = keeping.push(stores);

    if let Some(target) = fortress.building_upgrade.target {
        let when = fortress
            .building_upgrade
            .finish
            .map(|f| {
                if f <= chrono::Local::now() {
                    t("done").to_string()
                } else {
                    super::remaining_minutes(f)
                }
            })
            .unwrap_or_default();
        keeping = keeping.push(style::line_in(
            t("Building:"),
            format!("{target:?} ({when})"),
            Some(style::NAME_GOLD),
        ));
    }

    keeping = keeping.push(opt(
        t("Keep searching for gems"),
        FortressOption::Gems,
        fi.auto_gems,
    ));

    // The mine can not be steered - the game only shows the gem once the
    // search is over. So the choice is what happens to it afterwards
    if fi.auto_gems {
        let (mine, mine_colour) = match fortress.gem_search.finish {
            Some(finish) if finish > chrono::Local::now() => (
                format!("{} {}", t("digging until"), finish.format("%H:%M")),
                style::NAME_GOLD,
            ),
            Some(_) => (t("ready to collect").to_string(), style::GOOD),
            None => (t("not digging").to_string(), style::BAD),
        };
        keeping = keeping.push(style::line_in(
            t("Gem mine:"),
            mine,
            Some(mine_colour),
        ));
        // Socketing is not a choice: found gems go into empty slots and
        // better ones replace weaker ones of the same colour, always - blue
        // to Bert, green to Mark, yellow to Kunigunde, purple and red to
        // everybody. What the player decides is which colours are off limits
        keeping = keeping.push(style::note(t(
            "Found gems go into empty slots on their own: blue to Bert, green \
             to Mark, yellow to Kunigunde, purple and red to all three and to \
             you. A stronger gem replaces a weaker one of the same colour at \
             +100.",
        )));
        keeping = keeping.push(
            text(t("Gem protector - never sold, never replaced:")).size(12),
        );
        // Seven colours wrap onto a second line when the panel is too
        // narrow for them. They used to sit in a sideways scroller, which
        // hid the last two behind a bar nobody looks for
        let keep: Vec<iced::Element<Message>> =
            crate::config::GemWanted::every()
                .into_iter()
                .map(|g| {
                    let on = fi.gem_wanted.contains(&g);
                    button(text(t(g.label())).size(11))
                        .padding([3, 6])
                        .style(style::segment(on, style::ACCENT))
                        .on_press(Message::GemToggleWanted { ident, gem: g })
                        .into()
                })
                .collect();
        keeping = keeping.push(
            iced_aw::Wrap::with_elements(keep).spacing(4.0).line_spacing(4.0),
        );
        keeping = keeping.push(style::note(t(
            "A protected colour is never sold and a worn gem of it is never \
             replaced. With nothing protected every colour is weighed alike. \
             All-attribute and legendary gems come from events and are never \
             sold, whatever is set here.",
        )));
    }

    keeping = keeping.push(
        row!(
            button(text(t("Run housekeeping now")).size(12))
                .padding([4, 9])
                .style(style::quiet())
                .on_press(Message::FortressMaintainTick { ident }),
            text(fi.maintain_status.clone().unwrap_or_default()).size(11),
        )
        .spacing(10)
        .align_items(Alignment::Center),
    );

    // ---------------------------------------------------------------- the log
    let mut page = column!(
        army,
        row!(
            style::panel(t("Attacks"), attacks).width(Length::FillPortion(1)),
            style::panel(t("Housekeeping (every ~3 minutes)"), keeping)
                .width(Length::FillPortion(1)),
        )
        .spacing(10)
        .align_items(Alignment::Start),
    )
    .spacing(10)
    .width(Length::Fill);

    if !fi.log.is_empty() {
        let mut log = column!().spacing(3);
        for (time, msg, ok) in fi.log.iter().rev().take(50) {
            log = log.push(
                button(
                    row!(
                        text(msg).size(11),
                        horizontal_space(),
                        text(format!("{}", time.time().format("%H:%M")))
                            .size(11)
                    )
                    .align_items(Alignment::Center),
                )
                .padding([3, 7])
                .style(match ok {
                    true => style::primary(),
                    false => style::danger(),
                }),
            );
        }
        page = page.push(style::panel(
            t("Fortress log"),
            scrollable(log).height(Length::Fixed(150.0)),
        ));
    }

    // The page grew past a 700px window, so it scrolls as a whole
    style::page(page)
}
