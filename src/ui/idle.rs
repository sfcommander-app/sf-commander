use crate::i18n::t;
use iced::{
    Alignment, Element, Length,
    widget::{
        button, column, container, pick_list, row, scrollable,
    },
};
use super::glyphs::ticked as checkbox;
// His lettering stands in for the toolkit's `text`, so every line on
// this screen is set in the face the purse card is
use super::glyphs::lettered as text;
use num_traits::ToPrimitive;

use crate::{
    message::Message,
    player::{AccountInfo, AccountStatus},
    ui::style,
    util::LockExt,
};

fn money(v: f64) -> String {
    const UNITS: [(&str, f64); 5] =
        [("Q", 1e15), ("T", 1e12), ("B", 1e9), ("M", 1e6), ("K", 1e3)];
    for (name, size) in UNITS {
        if v >= size {
            return format!("{:.2} {name}", v / size);
        }
    }
    format!("{v:.0}")
}

/// The arena's buildings by the names the game gives them, rather than
/// the `{:?}` of the library's enum - "StrayingMonsters" is a Rust
/// identifier, not something a player reads
fn building_name(
    typ: sf_api::gamestate::idle::IdleBuildingType,
) -> &'static str {
    use sf_api::gamestate::idle::IdleBuildingType as B;
    match typ {
        B::Seat => t("Seats"),
        B::PopcornStand => t("Popcorn stand"),
        B::ParkingLot => t("Parking lot"),
        B::Trap => t("Trap"),
        B::Drinks => t("Drinks"),
        B::DeadlyTrap => t("Deadly trap"),
        B::VIPSeat => t("VIP seats"),
        B::Snacks => t("Snacks"),
        B::StrayingMonsters => t("Straying monsters"),
        B::Toilet => t("Toilet"),
    }
}

pub fn view_arena_manager(player: &AccountInfo) -> Element<'_, Message> {
    let lock = player.status.lock_safe();
    let gs = match &*lock {
        AccountStatus::Idle(_, gs) | AccountStatus::Busy(gs, _) => gs,
        AccountStatus::FatalError(err) => {
            return text(format!("Error: {err}")).size(20).into();
        }
        _ => return text(t("Logging in")).size(20).into(),
    };
    let ident = player.ident;

    let ii = &player.idle_info;
    let mut all = column!().spacing(10).width(Length::Fill);

    // Two different numbers, and telling them apart is the whole point of
    // this screen. `current_runes` is what is already in the bank and it
    // only ever grows; `sacrifice_runes` is what a reset would hand over
    // *now*, which is what the threshold is about and what the game's own
    // button shows. The panel used to do arithmetic on the first one, so
    // with 87 711 runes owned it said "9 runes to go" for ever and the app
    // kept asking for a reset the game would not allow
    let (owned, offer) = match gs.idle_game.as_ref() {
        Some(idle) => (
            crate::mechanics::runes_of(&idle.current_runes),
            crate::mechanics::runes_of(&idle.sacrifice_runes),
        ),
        None => (0, 0),
    };
    // The same exact minimum is used by the display and the reset decision.
    let pct = crate::mechanics::snap_sacrifice_step(ii.sacrifice_gain_pct);
    let bar = ii.sacrifice_target();
    let due = crate::mechanics::should_sacrifice(owned, offer, bar);
    // The same figure the automation uses, not the ladder alone - a
    // threshold on the screen that the bot does not act on is the screen
    // contradicting the code
    let target = crate::mechanics::sacrifice_bar_runes(owned, bar);
    let to_go = target.saturating_sub(offer);
    // What this reset would actually be worth, which is the number the
    // decision turns on and the one the game never shows
    let gain_now = crate::mechanics::sacrifice_gain_pct(owned, offer);

    match gs.idle_game.as_ref() {
        None => {
            all = all.push(style::block(
                text(t(
                    "The game did not send Arena Manager data for this \
                     character",
                ))
                .size(13),
            ));
        }
        Some(idle) => {
            // Laid out the way the game lays it out: the numbers across the
            // top, then the buildings as cards with what the next level
            // costs. The list of grey one-liners it used to be had nothing
            // in common with the screen it is meant to mirror
            // The game's own figure, as of its last answer to an Arena
            // Manager purchase - the buying works from the same number
            let purse = idle.current_money.to_f64().unwrap_or(0.0);
            all = all.push(
                row!(
                    style::tile(
                        t("Arena money"),
                        money(purse),
                        style::NAME_GOLD
                    ),
                    // Purple: the runes in the bank only ever go up, so
                    // this is a running total, not a figure to act on
                    style::tile(
                        t("runes"),
                        crate::ui::thousands(owned),
                        style::NEON
                    ),
                    style::tile_sub(
                        t("A reset now gives"),
                        crate::ui::thousands(offer),
                        if due {
                            style::GOOD
                        } else if offer > 0 {
                            style::NAME_GOLD
                        } else {
                            style::MUTED
                        },
                        Some(if due {
                            t("due now").to_string()
                        } else {
                            format!(
                                "{} {}",
                                crate::ui::thousands(to_go),
                                t("to go")
                            )
                        }),
                    ),
                    style::tile(
                        t("Threshold"),
                        crate::ui::thousands(target),
                        style::NAME_GOLD,
                    ),
                    style::tile(
                        t("resets so far"),
                        idle.resets.to_string(),
                        style::NAME_GOLD
                    ),
                )
                .spacing(8)
                .width(Length::Fill),
            );

            // One card per building. Green price = the money covers it right
            // now, which is the only thing worth looking at on this screen:
            // every coin spent raises both the earnings and what the next
            // sacrifice is worth
            let card = |typ, b: &sf_api::gamestate::idle::IdleBuilding| {
                let cost = b.upgrade_cost.to_f64().unwrap_or(0.0);
                let affordable = cost > 0.0 && cost <= purse;
                let level = if b.level == 0 {
                    t("not built yet").to_string()
                } else {
                    format!("{} {}", t("level"), b.level)
                };
                container(
                    column!(
                        row!(
                            text(building_name(typ))
                                .size(13)
                                .style(iced::theme::Text::Color(style::GOLD)),
                            iced::widget::horizontal_space(),
                            text(level).size(11).style(
                                iced::theme::Text::Color(style::MUTED)
                            ),
                        )
                        .align_items(Alignment::Center),
                        row!(
                            text(t("next upgrade"))
                                .size(11)
                                .style(iced::theme::Text::Color(style::MUTED)),
                            iced::widget::horizontal_space(),
                            text(money(cost)).size(14).style(
                                iced::theme::Text::Color(if affordable {
                                    style::GOOD
                                } else {
                                    style::BAD
                                })
                            ),
                        )
                        .spacing(6)
                        .align_items(Alignment::Center),
                    )
                    .spacing(3),
                )
                .padding([7, 10])
                .width(Length::Fill)
                .style(style::well())
            };
            // Three across at 1920, so the ten buildings are one block the
            // eye takes in rather than five rows to scroll past
            let all_cards: Vec<_> = idle.buildings.iter().collect();
            let mut grid = column!().spacing(7).width(Length::Fill);
            for chunk in all_cards.chunks(3) {
                let mut line = row!().spacing(7).width(Length::Fill);
                for (typ, b) in chunk {
                    line = line.push(card(*typ, b));
                }
                for _ in chunk.len()..3 {
                    line = line.push(iced::widget::horizontal_space());
                }
                grid = grid.push(line);
            }
            all = all.push(style::panel(t("Buildings"), grid));
        }
    }

    // ------------------------------------------------------------ settings
    let mut buying = column!().spacing(6).width(Length::Fill);
    buying = buying.push(
        checkbox(
            t("Upgrade buildings automatically"),
            ii.auto_upgrade,
        )
        .style(style::check())
        .on_toggle(move |nv| Message::IdleSetAuto { ident, state: nv })
        .text_size(13),
    );
    buying = buying.push(text(t("After each burst, buying resumes through the queue without waiting for the timer.")).size(12));
    buying = buying.push(
        button(text(t("Buy upgrades now")).size(12))
            .padding([5, 10])
            .style(style::brass(false))
            .on_press(Message::IdleTimer { ident }),
    );

    let mut resetting = column!().spacing(6).width(Length::Fill);
    resetting = resetting.push(
        checkbox(
            t("Sacrifice (reset) automatically at"),
            ii.auto_sacrifice
        )
        .style(style::check())
        .on_toggle(move |state| Message::IdleSetSacrifice { ident, state })
        .text_size(13),
    );
    resetting = resetting.push(
        row!(
            pick_list(
                crate::mechanics::SACRIFICE_STEPS,
                Some(pct),
                move |step| Message::IdleSetSacrificeStep { ident, step },
            )
            .width(Length::Fixed(70.0)),
            text(t("% more income from runes")).size(12),
        )
        .spacing(8)
        .align_items(Alignment::Center),
    );
    // The offer itself is a tile at the top; what belongs next to the
    // threshold box is how far off it is. `owned` is deliberately not
    // repeated here - the runes already in the bank have nothing to do with
    // whether a reset is worth doing, and printing the two side by side is
    // what made the old panel unreadable
    resetting = resetting.push(style::line_in(
        if due {
            t("Due now, against a threshold of").to_string()
        } else {
            t("Still short of").to_string()
        },
        crate::ui::thousands(target),
        Some(if due { style::GOOD } else { style::MUTED }),
    ));
    resetting = resetting.push(text(t("Fixed threshold: the selected percentage is never randomized.")).size(12));
    // The one number that says whether a reset is worth doing. The game
    // shows an offer of runes, which means nothing on its own: 746 runes is
    // everything to a new account and six hundredths of a per cent to one
    // with a million
    resetting = resetting.push(style::line_in(
        t("Resetting now would add"),
        format!("{gain_now:.2} %"),
        Some(if gain_now >= 1.0 { style::GOOD } else { style::BAD }),
    ));
    resetting = resetting.push(style::note(t(
        "A reset removes all building levels. It waits until the offered runes reach the selected income gain; whole rune blocks are taken into account.",
    )));
    // The Arena Manager has nothing to "collect" - the buildings earn money
    // on their own and you only ever spend it on upgrades - so that toggle
    // is gone
    if let Some(status) = &ii.status {
        resetting = resetting.push(text(status).size(11));
    }

    all = all.push(
        row!(
            style::panel(t("Buying"), buying).width(Length::FillPortion(1)),
            style::panel(t("Sacrifice"), resetting)
                .width(Length::FillPortion(1)),
        )
        .spacing(10),
    );

    if !ii.log.is_empty() {
        let mut log = column!().spacing(3);
        for (time, msg, _) in ii.log.iter().rev().take(40) {
            log = log
                .push(text(format!("{} {msg}", time.format("%H:%M"))).size(11));
        }
        all = all.push(style::panel(
            t("Arena Manager log"),
            scrollable(log).height(Length::Fixed(170.0)),
        ));
    }

    style::page(all)
}
