//! Weapon shop + magic shop: what is on offer, how it compares to what is
//! worn, and the automatic buying / attribute settings

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
use sf_api::gamestate::items::{EquipmentSlot, Item};

use crate::{
    message::{Message, ShopNumber, ShopOption},
    player::{AccountInfo, AccountStatus},
    shopping::item_score,
    util::LockExt,
};

/// The colour each wearer's column is set in. His own is the plain
/// figure - green or red by what it means. The companions are told apart
/// by colour: Bert blue, Mark green, Kunigunde gold, in his words
fn wearer_colour(index: usize) -> Color {
    match index {
        1 => Color::from_rgb(0.45, 0.68, 0.95),
        2 => crate::ui::style::GOOD,
        3 => crate::ui::style::NAME_GOLD,
        _ => crate::ui::style::GOLD_DIM,
    }
}

fn slot_name(slot: EquipmentSlot) -> &'static str {
    match slot {
        EquipmentSlot::Hat => "Hat",
        EquipmentSlot::BreastPlate => "Armor",
        EquipmentSlot::Gloves => "Gloves",
        EquipmentSlot::FootWear => "Boots",
        EquipmentSlot::Amulet => "Amulet",
        EquipmentSlot::Belt => "Belt",
        EquipmentSlot::Ring => "Ring",
        EquipmentSlot::Talisman => "Talisman",
        EquipmentSlot::Weapon => "Weapon",
        EquipmentSlot::Shield => "Shield",
    }
}

fn item_line(
    item: &Item,
    class: sf_api::gamestate::character::Class,
) -> String {
    // Three letters, not "Intelligence". The line carries up to three
    // attributes, a damage range, a rune and a price, and spelled out in
    // full it was long enough to wrap - which on a shelf of twelve offers
    // turned each one into a paragraph
    let short = |a: sf_api::command::AttributeType| match a {
        sf_api::command::AttributeType::Strength => "Str",
        sf_api::command::AttributeType::Dexterity => "Dex",
        sf_api::command::AttributeType::Intelligence => "Int",
        sf_api::command::AttributeType::Constitution => "Con",
        sf_api::command::AttributeType::Luck => "Lck",
    };
    let attrs: Vec<String> = item
        .attributes
        .iter()
        .filter(|(_, v)| **v > 0)
        .map(|(a, v)| format!("{} {v}", short(a)))
        .collect();
    let extra = match item.typ {
        sf_api::gamestate::items::ItemType::Weapon { min_dmg, max_dmg } => {
            format!(" dmg {min_dmg}-{max_dmg}")
        }
        sf_api::gamestate::items::ItemType::Shield { block_chance } => {
            format!(" block {block_chance}%")
        }
        _ => String::new(),
    };
    let _ = class;
    // The item level the game prints next to the name (253), the rarity,
    // and the rune - the things a player compares by eye
    let rarity = if item.is_legendary() {
        " LEGENDARY"
    } else if item.is_epic() {
        " epic"
    } else {
        ""
    };
    let rune = item
        .rune
        .as_ref()
        .map(|r| format!(" · rune {:?} +{}", r.typ, r.value))
        .unwrap_or_default();
    format!(
        "({}){} {}{}{} · {}",
        item.item_quality,
        rarity,
        attrs.join(", "),
        extra,
        rune,
        if item.mushroom_price > 0 {
            format!("{} mushrooms", item.mushroom_price)
        } else {
            format!("{} gold", item.price / 100)
        },
    )
}

pub fn view_shops(player: &AccountInfo) -> Element<'_, Message> {
    let lock = player.status.lock_safe();
    let gs = match &*lock {
        AccountStatus::Idle(_, gs) | AccountStatus::Busy(gs, _) => gs,
        AccountStatus::FatalError(err) => {
            return text(format!("Error: {err}")).size(20).into();
        }
        _ => return text(t("Logging in")).size(20).into(),
    };
    let si = &player.shop_info;
    let class = gs.character.class;
    let prot = si.protection();

    let mut col = column!().spacing(7).width(Length::Fill);
    col = col.push(crate::ui::style::note(t(
        "Per piece: what it does to your main attribute, constitution and \
         luck together, for you and for each companion, against what each of \
         you wears in that slot",
    )));

    // ---------------------------------------------------------- offers
    let good = crate::ui::style::GOOD;
    let dim = crate::ui::style::MUTED;
    // The four wearers' figures stand in four lanes, right-aligned, with
    // the names in a header once per shop. They used to be "Bert -9.5 %"
    // glued into one cell per wearer, so the figures never lined up down
    // the table and the widest name pushed its figure into the next lane
    // - "Kunigunde -16.9" with the "%" cut off, and "Mark -" for somebody
    // who cannot wear the piece, which reads as a minus beside all the
    // real ones
    const LANE_SLOT: f32 = 60.0;
    const LANE_PCT: f32 = 66.0;
    const LANE_NOTE: f32 = 190.0;
    let lane = |body: String, colour: Color| {
        text(body)
            .size(11)
            .style(theme::Text::Color(colour))
            .width(Length::Fixed(LANE_PCT))
            .horizontal_alignment(iced::alignment::Horizontal::Right)
    };
    for shop in gs.shops.values() {
        col = col.push(
            text(match shop.typ {
                sf_api::command::ShopType::Weapon => t("Weapon shop"),
                sf_api::command::ShopType::Magic => t("Magic shop"),
            })
            .size(13)
            .style(theme::Text::Color(crate::ui::style::GOLD)),
        );
        let mut head = row!(
            text("").width(Length::Fixed(LANE_SLOT)),
            text("").width(Length::Fill),
        )
        .spacing(6);
        for (i, who) in crate::shopping::WEARERS.iter().enumerate() {
            head = head.push(lane(t(who).to_string(), wearer_colour(i)));
        }
        head = head.push(text("").width(Length::Fixed(LANE_NOTE)));
        col = col.push(head);
        for (_, item) in shop.iter() {
            let Some(slot) = item.typ.equipment_slot() else {
                // Consumables: a readable line, not the raw Debug dump
                use sf_api::gamestate::items::ItemType;
                let (name, note) = match &item.typ {
                    ItemType::QuickSandGlass => (
                        t("Hourglass").to_string(),
                        t("bought automatically").to_string(),
                    ),
                    ItemType::Potion(p) => (
                        format!("{} {:?} ({:?})", t("Potion"), p.typ, p.size),
                        String::new(),
                    ),
                    other => (format!("{other:?}"), String::new()),
                };
                let price = if item.mushroom_price > 0 {
                    format!("{} {}", item.mushroom_price, t("mushrooms"))
                } else {
                    format!("{} {}", item.price / 100, t("gold"))
                };
                // The name gets the item lane, not the slot lane: "Potion
                // Intelligence (Small)" in sixty pixels was three lines
                col = col.push(
                    row!(
                        text("").width(Length::Fixed(LANE_SLOT)),
                        text(format!("{name} \u{00B7} {price}"))
                            .size(11)
                            .width(Length::Fill),
                        text(note)
                            .size(10)
                            .style(theme::Text::Color(dim))
                            .width(Length::Fixed(LANE_NOTE)),
                    )
                    .spacing(6),
                );
                continue;
            };
            let worn = gs.character.equipment.0[slot].as_ref();
            // What the buy does to each possible wearer - the character and
            // the three companions - in the three attributes that matter
            // (main, constitution, luck), against what that wearer has on
            // in this slot. A nerf shows as a minus, "-" means they can not
            // wear it. The buying logic keeps its own score ratio; this is
            // what a person reads
            let gains = crate::shopping::gains_for_everyone(gs, item);
            let bad = Color::from_rgb(0.90, 0.45, 0.45);
            let worn_score = worn.map(|w| item_score(w, class)).unwrap_or(0.0);
            let new_score = item_score(item, class);
            let buys = worn_score <= 0.0
                || (new_score / worn_score - 1.0) * 100.0
                    >= f64::from(si.min_gain);
            // Whether the worn piece actually stops this buy - not merely
            // whether it carries a protection.
            //
            // `protected_reason` answers "does this piece have a guard on
            // it", and the page was printing that as "worn item protected
            // (epic)" against every epic in the slot. But an epic, an
            // enchantment, blacksmith levels and a gem are all *soft*: they
            // yield to a piece that is `EPIC_OVERRIDE_PCT` better and of a
            // higher item level, which is what the settings above this table
            // say in words. So the page was calling a buy blocked that the
            // shop step was about to make. `may_replace` is the rule the
            // automation actually follows, and it is the one to ask
            let gain_pct = (new_score > 0.0 && worn_score > 0.0)
                .then(|| (new_score / worn_score - 1.0) * 100.0)
                .unwrap_or(999.0);
            let protected =
                crate::shopping::blocks_buy(worn, item, gain_pct, &prot);
            let mut cells = row!().spacing(6);
            for (i, g) in gains.iter().enumerate() {
                let (txt, colour) = match g {
                    // His own column keeps green for a buy and red for a
                    // loss - it is the one the decision hangs on. Each
                    // companion's column is that companion's colour, so
                    // the three read as three columns and not as one
                    // field of red: full for a gain, faded for a loss
                    Some(g) if i == 0 => {
                        let ok = g.pct > 0.0 && protected.is_none() && buys;
                        (
                            format!("{:+.1} %", g.pct),
                            if ok {
                                good
                            } else if g.pct < 0.0 {
                                bad
                            } else {
                                dim
                            },
                        )
                    }
                    Some(g) => {
                        let own = wearer_colour(i);
                        (
                            format!("{:+.1} %", g.pct),
                            if g.pct > 0.0 {
                                own
                            } else {
                                Color { a: 0.5, ..own }
                            },
                        )
                    }
                    // Cannot wear it: a dot, not a dash. A dash in a
                    // column of minus figures is one more minus
                    None => ("\u{00B7}".to_string(), dim),
                };
                cells = cells.push(lane(txt, colour));
            }
            let note = if let Some(why) = protected {
                if why == "legendary" {
                    format!("{} \u{2013} {}", t(why), t("never replaced"))
                } else {
                    // Say what it would take, or the line is a dead end:
                    // the guard is not "never", it is "not by this piece"
                    format!(
                        "{} \u{2192} +{:.0} % {}",
                        t(why),
                        crate::shopping::EPIC_OVERRIDE_PCT,
                        t("and higher lvl")
                    )
                }
            } else if gains[0].is_some_and(|g| g.pct > 0.0) && !buys {
                t("below threshold").to_string()
            } else {
                String::new()
            };
            col = col.push(
                row!(
                    text(t(slot_name(slot)))
                        .size(11)
                        .width(Length::Fixed(LANE_SLOT)),
                    text(item_line(item, class)).size(11).width(Length::Fill),
                    cells,
                    text(note)
                        .size(10)
                        .style(theme::Text::Color(dim))
                        .width(Length::Fixed(LANE_NOTE)),
                )
                .spacing(6)
                .align_items(iced::Alignment::Center),
            );
        }
    }

    // ---------------------------------------------------------- settings
    // Stacked, not side by side. An offer row carries a slot, a long item
    // line and four wearers' gains - about a thousand pixels - and the
    // rules below carry button rows of their own that want six hundred.
    // Sharing 1460 between them wrapped every item line into ten, which is
    // what the shelves looked like: a column of loose words
    let mut page = column!(crate::ui::style::panel(t("Shops"), col))
        .spacing(10)
        .width(Length::Fill);
    if !si.log.is_empty() {
        let mut log = column!().spacing(3);
        for (time, msg, ok) in si.log.iter().rev().take(40) {
            log = log.push(
                row!(
                    text(time.format("%H:%M").to_string()).size(11),
                    text(msg).size(11).style(theme::Text::Color(if *ok {
                        crate::ui::style::GOOD
                    } else {
                        crate::ui::style::BAD
                    })),
                    horizontal_space(),
                )
                .spacing(8),
            );
        }
        page = page.push(crate::ui::style::panel(
            t("Shop log"),
            scrollable(log).height(Length::Fixed(140.0)),
        ));
    }
    page = page.push(crate::ui::style::panel(
        t("Buying rules"),
        maxxing_settings(player, gs),
    ));

    crate::ui::style::page(page)
}

/// Character maxxing: gear buying with its protections, the companions,
/// the backpack and the attributes. Shown on the Shops page and, as the
/// guild leader asked, on the Character page in one place
pub fn maxxing_settings<'a>(
    player: &'a AccountInfo,
    gs: &sf_api::gamestate::GameState,
) -> iced::widget::Column<'a, Message> {
    let ident = player.ident;
    let si = &player.shop_info;
    let class = gs.character.class;
    let mut col = column!().spacing(10).width(Length::Fill);
    let opt = |label: &'static str, option: ShopOption, on: bool| {
        checkbox(t(label), on)
            .style(crate::ui::style::check())
            .on_toggle(move |state| Message::ShopSetOption {
                ident,
                option,
                state,
            })
            .size(16)
            .text_size(13)
    };
    let num =
        |what: ShopNumber, value: u32, max: u32| {
            crate::ui::num_field(value, max, move |value| {
                Message::ShopSetNumber { ident, what, value }
            })
        };
    // The one number a player has to set: the bot's mushrooms for the day.
    // Big and first, because every paid step in the app comes out of it
    let ti = &player.tavern_info;
    let mut budget_row = row!().spacing(3);
    for v in [0u32, 5, 10, 20, 30, 50, 100] {
        budget_row = budget_row.push(
            button(text(v.to_string()).size(11))
                .padding([3, 7])
                .style(if ti.mushroom_budget == v {
                    crate::ui::style::primary()
                } else {
                    crate::ui::style::quiet()
                })
                .on_press(Message::SetMushroomBudget { ident, n: v }),
        );
    }
    col = col.push(
        row!(
            text(t("Mushrooms a day:")).size(15),
            budget_row,
            text(format!(
                "{} {}/{}",
                t("spent today"),
                ti.mushroom_spent_today,
                ti.mushroom_budget
            ))
            .size(13)
            .style(theme::Text::Color(
                if ti.mushroom_spent_today >= ti.mushroom_budget {
                    crate::ui::style::BAD
                } else {
                    crate::ui::style::GOOD
                }
            )),
        )
        .spacing(8)
        .align_items(Alignment::Center),
    );
    col = col.push(
        text(t(
            "The bot's budget for one server day - beer, paid dungeon and \
             arena fights, shop rerolls, epics and skipped waits all come out \
             of it, and it does what it likes with it. Once it is spent the \
             bot pays nothing more until the server's midnight. 0 keeps \
             every mushroom.",
        ))
        .size(11)
        .style(iced::theme::Text::Color(crate::ui::style::MUTED)),
    );
    // No switch for buying: it is on whenever the automation is on. Every
    // piece in both shops is weighed for the character and the three
    // companions, and the best upgrade is bought first
    col = col.push(
        text(t(
            "Gear is kept up on its own: every piece in the weapon and the \
             magic shop is weighed for you and for Bert, Mark and Kunigunde \
             - legendary before epic before plain, runes counted, gems left \
             out (a gem moves with you). The best upgrade is bought first; a \
             better gem of the same colour replaces a worn one at +100.",
        ))
        .size(12),
    );
    // Buttons, one lit at a time - the number boxes could not be typed
    // into and took a click per percent
    let choice =
        |what: ShopNumber, steps: &[u8], current: u8, star: Option<u8>| {
            let mut r = row!().spacing(3);
            for &v in steps {
                // "(rec.)" in words - the star glyph came out as a box
                let label = if star == Some(v) {
                    format!("{v} % ({})", t("rec."))
                } else {
                    format!("{v} %")
                };
                r = r.push(
                    button(text(label).size(11))
                        .padding([3, 7])
                        .style(if current == v {
                            crate::ui::style::primary()
                        } else {
                            crate::ui::style::quiet()
                        })
                        .on_press(Message::ShopSetNumber {
                            ident,
                            what,
                            value: u32::from(v),
                        }),
                );
            }
            r
        };
    col = col.push(
        row!(
            text(t("Only when at least this much better:")).size(12),
            // A box for any number first (1, 2, 3, 4 %...), the steps after
            crate::ui::num_field(
                u32::from(si.min_gain),
                100u32,
                move |value| {
                    Message::ShopSetNumber {
                        ident,
                        what: ShopNumber::MinGain,
                        value,
                    }
                }
            )
            .width(Length::Fixed(48.0)),
            choice(
                ShopNumber::MinGain,
                &crate::shopping::MIN_GAIN_STEPS,
                si.min_gain,
                Some(si.recommended_gain),
            ),
        )
        .spacing(8)
        .align_items(Alignment::Center),
    );
    col = col.push(
        text(format!(
            "{} {} % - {}",
            t("recommended for this character:"),
            si.recommended_gain,
            t("from how fast it levels: the faster the levels, the sooner a piece is outgrown, so the bar is higher")
        ))
        .size(11)
        .style(iced::theme::Text::Color(crate::ui::style::MUTED)),
    );
    col = col.push(
        row!(
            text(t("Keep of the gold untouched:")).size(12),
            choice(
                ShopNumber::GoldReserve,
                &[0, 10, 20, 30, 50],
                si.gold_reserve,
                None
            ),
        )
        .spacing(8)
        .align_items(Alignment::Center),
    );
    // A worn legendary may give way to a much better piece (5.3); the old
    // one goes into the bag and is never sold. 0 = never
    col = col.push(
        row!(
            text(t("Swap a worn legendary for a piece better by:")).size(12),
            {
                let mut r = row!().spacing(3);
                for v in [0u8, 30, 40, 50] {
                    let label = if v == 0 {
                        t("never").to_string()
                    } else {
                        format!("{v} %")
                    };
                    r = r.push(
                        button(text(label).size(11))
                            .padding([3, 7])
                            .style(if si.legendary_swap_pct == v {
                                crate::ui::style::primary()
                            } else {
                                crate::ui::style::quiet()
                            })
                            .on_press(Message::ShopSetNumber {
                                ident,
                                what: ShopNumber::LegendarySwap,
                                value: u32::from(v),
                            }),
                    );
                }
                r
            },
            text(t("(the old one goes into the bag, never sold)"))
                .size(11)
                .style(iced::theme::Text::Color(crate::ui::style::MUTED)),
        )
        .spacing(8)
        .align_items(Alignment::Center),
    );
    // How often the shops may be rerolled for a mushroom when they hold
    // nothing for anybody - it was a fixed three with no way to see it
    col = col.push(
        row!(
            text(t("Reroll a shop up to")).size(12),
            num(ShopNumber::Rolls, u32::from(si.rolls_per_day), 20),
            text(t(
                "times a day (1 mushroom each, only when nothing fits anybody)"
            ))
            .size(12),
        )
        .spacing(6)
        .align_items(Alignment::Center),
    );
    col = col.push(
        text(t(
            "Give way only to a piece 20 % better of a higher item level:",
        ))
        .size(12),
    );
    col = col.push(
        row!(
            opt("epics", ShopOption::ProtectEpics, si.protect_epics),
            opt(
                "enchanted",
                ShopOption::ProtectEnchanted,
                si.protect_enchanted
            ),
            opt(
                "blacksmith-upgraded",
                ShopOption::ProtectUpgraded,
                si.protect_upgraded
            ),
        )
        .spacing(14),
    );
    col = col.push(
        text(t("Replaced legendary equipment stays in the backpack. Equipment useful to a companion is also kept."))
            .size(11)
            .style(iced::theme::Text::Color(crate::ui::style::MUTED)),
    );
    col = col.push(
        column!(
            opt(
                "Recycle replaced equipment automatically",
                ShopOption::SellOld,
                si.sell_old
            ),
            text(t("Extract the gem first. If extraction is unavailable, dismantle at the blacksmith. Sell ordinary pieces without a gem."))
                .size(13)
                .width(Length::Fill)
                .style(iced::theme::Text::Color(crate::ui::style::MUTED)),
            opt(
                "Buy epics for mushrooms",
                ShopOption::Mushrooms,
                si.mushrooms
            ),
        )
        .spacing(14),
    );
    col = col.push(
        text(t("New equipment is equipped first. With automatic recycling off, the old piece stays in the backpack."))
            .size(11)
            .style(iced::theme::Text::Color(crate::ui::style::MUTED)),
    );
    // ---------------------------------------------------------- blacksmith
    col = col.push(crate::ui::style::heading(t("Blacksmith")));
    let smith_line = match &gs.blacksmith {
        Some(b) => format!(
            "{} {} \u{00B7} {} {}",
            crate::ui::thousands(b.metal),
            t("metal"),
            crate::ui::thousands(b.arcane),
            t("arcane splinters")
        ),
        None => t("Not unlocked yet").to_string(),
    };
    col = col.push(
        row!(
            opt(
                "Raise the worn gear at the blacksmith (you and the three companions, up to +20)",
                ShopOption::Smith,
                si.smith_auto
            ),
            text(smith_line).size(12),
        )
        .spacing(14)
        .align_items(Alignment::Center),
    );
    col = col.push(
        text(t(
            "A level is 3 % of the piece for metal and arcane splinters. \
             Never a piece with something better on offer in a shop or in \
             the bag - that one is on its way out. The level that buys the \
             most for its materials goes first, your gear before theirs, \
             so the levels spread over the whole set. No level takes more \
             than 40 % of the metal or arcane in stock - the rest waits \
             for what comes in. When a piece is \
             weighed against a new one its levels count 2 % each: a +10 \
             piece gives way to one 20 % better, which is then raised in \
             its turn. With this on, the \"blacksmith-upgraded\" protection \
             above is off - every piece would soon be upgraded.",
        ))
        .size(11)
        .style(iced::theme::Text::Color(crate::ui::style::MUTED)),
    );
    col = col.push(
        text(si.smith_status.clone().unwrap_or_default())
            .size(11)
            .style(theme::Text::Color(Color::from_rgb(0.95, 0.80, 0.40))),
    );
    // ---------------------------------------------------------- witch
    col = col.push(crate::ui::style::heading(t("Witch")));
    col = col.push(opt(
        "Enchant the worn gear at the witch (unlocked enchantments, gold above the reserve)",
        ShopOption::Witch,
        si.witch_auto,
    ));
    col = col.push(
        text(t(
            "A bought piece is put on, then enchanted at the witch as soon as \
             its enchantment is unlocked and the gold above the reserve covers \
             the price, and gets the next fitting gem from the bag (see the \
             Fortress page). Never a piece with something better in sight.",
        ))
        .size(11)
        .style(iced::theme::Text::Color(crate::ui::style::MUTED)),
    );
    col = col.push(
        text(si.witch_status.clone().unwrap_or_default())
            .size(11)
            .style(theme::Text::Color(Color::from_rgb(0.95, 0.80, 0.40))),
    );
    // The companions dress themselves from the bag - it needs no heading
    // and no paragraph, it just happens
    col = col.push(crate::ui::style::heading(t("Backpack")));
    col = col.push(
        row!(
            opt(
                "Keep room in the backpack",
                ShopOption::ClearBackpack,
                si.clear_backpack
            ),
            num(ShopNumber::KeepFree, u32::from(si.keep_free), 10),
            text(t("slots free")).size(12),
        )
        .spacing(6)
        .align_items(Alignment::Center),
    );
    col = col.push(
        text(t(
            "A full backpack stops the dungeons - the game will not hand out \
             loot it has nowhere to put, and mining gems fills the bag fast. \
             The weakest piece that is not protected and is no upgrade for \
             anybody in the party is sold to make room.",
        ))
        .size(11)
        .style(iced::theme::Text::Color(crate::ui::style::MUTED)),
    );
    col = col.push(
        row!(
            button(text(t("Check the shops now")).size(12))
                .padding(4)
                .style(crate::ui::style::quiet())
                .on_press(Message::ShopTimer { ident }),
            text(si.status.clone().unwrap_or_default())
                .size(11)
                .style(theme::Text::Color(Color::from_rgb(0.95, 0.80, 0.40))),
        )
        .spacing(10)
        .align_items(Alignment::Center),
    );

    // ---------------------------------------------------------- attributes
    col = col.push(crate::ui::style::heading(t("Attributes")));
    let main = class.main_attribute();
    let basis = &gs.character.attribute_basis;
    let bought = &gs.character.attribute_times_bought;
    col = col.push(
        text(format!(
            "{main:?} {} ({} {}), Constitution {} ({}), Luck {} ({})",
            basis[main],
            bought[main],
            t("bought"),
            basis[sf_api::command::AttributeType::Constitution],
            bought[sf_api::command::AttributeType::Constitution],
            basis[sf_api::command::AttributeType::Luck],
            bought[sf_api::command::AttributeType::Luck],
        ))
        .size(11)
        .style(iced::theme::Text::Color(crate::ui::style::MUTED)),
    );
    col = col.push(opt(
        "Buy attributes with the gold above the reserve",
        ShopOption::AttrAuto,
        si.attr_auto,
    ));
    col = col.push(
        row!(
            text(t("Weights:")).size(12),
            text(format!("{main:?}")).size(12),
            num(ShopNumber::WMain, u32::from(si.attr_w_main), 100),
            text(t("Constitution")).size(12),
            num(ShopNumber::WCon, u32::from(si.attr_w_con), 100),
            text(t("Luck")).size(12),
            num(ShopNumber::WLuck, u32::from(si.attr_w_luck), 100),
        )
        .spacing(6)
        .align_items(Alignment::Center),
    );
    col = col.push(
        text(t(
            "The attribute furthest behind its weight is bought next: 10 / 10 / 5 \
             means main and constitution alternate and luck gets every other \
             point. 0 = never. Off-attributes are never bought. The same gold \
             reserve as for the shops applies.",
        ))
        .size(11)
        .style(iced::theme::Text::Color(crate::ui::style::MUTED)),
    );
    col = col.push(
        row!(
            button(text(t("Buy attributes now")).size(12))
                .padding(4)
                .style(crate::ui::style::quiet())
                .on_press(Message::AttrTimer { ident }),
            text(si.attr_status.clone().unwrap_or_default())
                .size(11)
                .style(theme::Text::Color(Color::from_rgb(0.95, 0.80, 0.40))),
        )
        .spacing(10)
        .align_items(Alignment::Center),
    );
    col = col.push(sacrifice_card(player, gs));
    col
}

/// The Sacrifice window (6.0): legendaries in the bag that what is worn
/// now beats by 75 % or more. Nothing happens to them on its own - the
/// player presses "Let it go", reads the warning and presses F
fn sacrifice_card<'a>(
    player: &'a AccountInfo,
    gs: &sf_api::gamestate::GameState,
) -> Element<'a, Message> {
    let ident = player.ident;
    let retired = crate::shopping::retired_legendaries(gs, 75.0);
    let mut col = column!(crate::ui::style::heading(t("Sacrifice"))).spacing(8);
    if retired.is_empty() {
        col = col.push(
            text(t("No legendary in the bag has been outgrown by 75 % yet."))
                .size(12)
                .style(theme::Text::Color(crate::ui::style::MUTED)),
        );
        return col.into();
    }
    for r in retired {
        let pos = r.pos.backpack_pos();
        let asked = player.shop_info.sacrifice_ask == Some(pos);
        let line = format!(
            "{} - {} {:.0} %{}",
            t(slot_name(r.slot)),
            t("what you wear now is better by"),
            r.weaker_pct,
            if r.has_gem {
                format!(" · {}", t("has a gem"))
            } else {
                String::new()
            }
        );
        if asked {
            let warn = iced::widget::container(
                column!(
                    text(t("WARNING"))
                        .size(16)
                        .style(theme::Text::Color(crate::ui::style::BAD)),
                    text(t(
                        "This destroys a legendary for good. The gem comes out \
                         first and stays yours, the piece is taken apart at \
                         the blacksmith for its materials. There is no way back.",
                    ))
                    .size(13),
                    text(t("Press F to pay respect."))
                        .size(13)
                        .style(theme::Text::Color(crate::ui::style::WARN)),
                    row!(
                        button(text("F").size(18))
                            .padding([4, 18])
                            .style(crate::ui::style::danger())
                            .on_press(Message::SacrificeConfirm { ident }),
                        button(text(t("Let it live")).size(13))
                            .padding([6, 12])
                            .style(crate::ui::style::quiet())
                            .on_press(Message::SacrificeCancel { ident }),
                    )
                    .spacing(10)
                    .align_items(Alignment::Center),
                )
                .spacing(6),
            )
            .padding(10)
            .style(crate::ui::style::tint(crate::ui::style::BAD));
            col = col.push(text(line).size(13)).push(warn);
        } else {
            col = col.push(
                row!(
                    text(line).size(13).width(Length::Fill),
                    button(text(t("Let it go")).size(12))
                        .padding([4, 10])
                        .style(crate::ui::style::quiet())
                        .on_press(Message::SacrificeAsk { ident, pos }),
                )
                .spacing(10)
                .align_items(Alignment::Center),
            );
        }
    }
    col.into()
}
