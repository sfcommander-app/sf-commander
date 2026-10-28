use crate::i18n::t;
use iced::{
    Alignment, Color, Element, Length,
    alignment::Horizontal,
    theme,
    widget::{
        self, button, column, container, horizontal_rule,
        horizontal_space, pick_list, progress_bar, row, scrollable, text_input,
    },
};
use super::glyphs::ticked as checkbox;
// His lettering stands in for the toolkit's `text`, so every line on
// this screen is set in the face the purse card is
use super::glyphs::lettered as text;
use sf_api::gamestate::guild::BattlesJoined;

use crate::{
    guild_history::GuildHistory,
    guild_report::{Verdict, member_stats},
    message::Message,
    player::{AccountInfo, AccountStatus, MemberSort},
    server::ServerInfo,
    util::LockExt,
};

/// The Hall of Knights level the guild leader wants everybody at. The
/// summary counts how many are there - "how many are at our current best"
/// flatters a guild where the best is low
pub const HOK_TARGET: u16 = 10;

fn pct_color(p: f32) -> Color {
    match p {
        p if p >= 75.0 => Color::from_rgb(0.36, 0.78, 0.45),
        p if p >= 40.0 => Color::from_rgb(0.85, 0.72, 0.20),
        _ => Color::from_rgb(0.83, 0.33, 0.33),
    }
}

/// The two guild-wide bonuses to gold and XP from quests, and how far along
/// they are.
///
/// Both are filled from two sides, worth +100 % each:
/// * the Treasure / Instructor level the members pay for, up to level 500
/// * the guild raids - each of the first 50 raid dungeons adds +2 %, so
///   dungeon 50 is the last one that counts towards this
///
/// (Playa's own help pages; raids past the 50th still give XP and album
/// pictures, just no more bonus here.)
const GUILD_SKILL_MAX: u16 = 500;
const BONUS_RAIDS_MAX: u16 = 50;

/// Percent of the maximum, 0-100, for one of the two bonuses
fn bonus_progress(skill: u16, raids: u16) -> f32 {
    let from_members =
        f32::from(skill.min(GUILD_SKILL_MAX)) / f32::from(GUILD_SKILL_MAX);
    let from_raids =
        f32::from(raids.min(BONUS_RAIDS_MAX)) / f32::from(BONUS_RAIDS_MAX);
    (from_members + from_raids) / 2.0 * 100.0
}

/// One labelled progress bar for the guild block. `detail` carries the raw
/// numbers, because a bar alone never says whether 80 % is 400 of 500 or
/// 4 of 5
fn bonus_bar<'a>(
    label: &'a str,
    pct: f32,
    detail: String,
) -> Element<'a, Message> {
    row!(
        text(label).size(11).width(Length::FillPortion(7)),
        progress_bar(0.0..=100.0, pct)
            .height(Length::Fixed(9.0))
            .width(Length::FillPortion(16)),
        text(format!("{pct:.0}%"))
            .size(11)
            .style(theme::Text::Color(pct_color(pct)))
            .width(Length::FillPortion(3))
            .horizontal_alignment(Horizontal::Right),
        text(detail)
            .size(11)
            .style(theme::Text::Color(Color::from_rgb(0.62, 0.65, 0.70)))
            .width(Length::FillPortion(9)),
    )
    .spacing(8)
    .align_items(Alignment::Center)
    .into()
}

/// A guild level as a plain number. Green once it is level with the best in
/// the guild, so a finished guild is a column of green and needs no reading
fn level_cell(level: u16, max: u16, fs: u16) -> Element<'static, Message> {
    let done = max > 0 && level >= max;
    // Shrink, not a share: three of these sit in one lane now, and a share
    // of a lane whose own width is "as much as the contents need" is a
    // circular question the layout answers by running off the page
    text(level)
        .size(fs)
        .style(theme::Text::Color(if done {
            crate::ui::style::GOOD
        } else {
            Color::from_rgb(0.90, 0.70, 0.35)
        }))
        .width(Length::Shrink)
        .into()
}

fn pct_cell(
    p: f32,
    portion: u16,
    compact: bool,
    fs: u16,
) -> Element<'static, Message> {
    if compact {
        return text(format!("{p:.0}%"))
            .size(fs)
            .style(theme::Text::Color(pct_color(p)))
            .width(Length::FillPortion(portion))
            .into();
    }
    // The bar used to be a fixed 8 px with size-11 lettering whatever the
    // rest of the row did, so on a guild of fifty it - not the type - was
    // what set the row height, and the table ran off the screen anyway
    row!(
        progress_bar(0.0..=100.0, p)
            .height(Length::Fixed(f32::from(fs).mul_add(0.6, 1.0)))
            .style(crate::ui::style::bar(pct_color(p)))
            .width(Length::FillPortion(3)),
        text(format!("{p:.0}%"))
            .size(fs)
            .style(theme::Text::Color(pct_color(p)))
            .width(Length::FillPortion(2))
            .horizontal_alignment(Horizontal::Right),
    )
    .spacing(4)
    .width(Length::FillPortion(portion))
    .align_items(Alignment::Center)
    .into()
}

fn online_text(hours: Option<i64>) -> String {
    match hours {
        None => "?".to_string(),
        Some(h) if h < 1 => "now".to_string(),
        Some(h) if h < 24 => format!("{h} h"),
        Some(h) => format!("{} d", h / 24),
    }
}

/// The highest treasure / instructor / knights level anyone in the guild
/// has been raised to. That is the ceiling the leader has actually paid
/// for, so it is the only honest thing to measure the rest against
fn guild_ceilings(guild: &sf_api::gamestate::guild::Guild) -> (u16, u16, u16) {
    let max_treasure = guild
        .members
        .iter()
        .map(|m| m.treasure_skill)
        .max()
        .unwrap_or(0);
    let max_instructor = guild
        .members
        .iter()
        .map(|m| m.instructor_skill)
        .max()
        .unwrap_or(0);
    let max_knights = guild
        .members
        .iter()
        .map(|m| u16::from(m.knights))
        .max()
        .unwrap_or(0);
    (max_treasure, max_instructor, max_knights)
}

/// The portal column: today's swing, and behind it how many of the last
/// days we watched the member took it - "yes 9/14". The share colours the
/// cell once the diary has three days; before that today does. The game
/// only keeps the last swing, so the share is the diary's (see
/// `guild_history`)
/// Battles: how many the member signed up for, and how many they let
/// pass. Two plain numbers, green over red - the same shape as the portal
/// column next to it.
///
/// It used to read "7W 5L (58 %)", which was the raids the *guild* won
/// while this member was in it: not what the column is called, not what
/// anybody reads it as, and nothing the member did or failed to do. Those
/// two numbers are still in the tooltip, where they are context rather
/// than a verdict
fn battles_cell<'a>(
    s: &crate::guild_report::MemberStats<'_>,
    fs: u16,
) -> Element<'a, Message> {
    let (signed, seen) = s.progress.battles;
    let missed = seen.saturating_sub(signed);
    if seen == 0 {
        return text("\u{2013}")
            .size(fs)
            .style(theme::Text::Color(crate::ui::style::MUTED))
            .width(Length::FillPortion(5))
            .into();
    }
    let (a_in, a_seen) = s.progress.attacks;
    let (d_in, d_seen) = s.progress.defenses;
    let raids = s.raid_wins + s.raid_losses;
    let mut tip = format!(
        "{}: {signed} / {missed}\n{}: {a_in}/{a_seen}\n{}: {d_in}/{d_seen}",
        t("Battles signed up for / missed"),
        t("attacks"),
        t("defenses"),
    );
    if raids > 0 {
        tip.push_str(&format!(
            "\n{}: {}W {}L",
            t("guild raids in their time"),
            s.raid_wins,
            s.raid_losses
        ));
    }
    widget::tooltip(
        row!(
            text(signed.to_string())
                .size(fs)
                .style(theme::Text::Color(Color::from_rgb(0.36, 0.78, 0.45))),
            text("/")
                .size(fs)
                .style(theme::Text::Color(crate::ui::style::MUTED)),
            text(missed.to_string())
                .size(fs)
                .style(theme::Text::Color(Color::from_rgb(0.83, 0.33, 0.33))),
        )
        .spacing(2)
        .width(Length::FillPortion(5)),
        text(tip).size(fs),
        widget::tooltip::Position::Left,
    )
    .gap(4)
    .style(theme::Container::Box)
    .into()
}

fn portal_cell<'a>(
    s: &crate::guild_report::MemberStats<'_>,
    fs: u16,
) -> Element<'a, Message> {
    let (fought, watched) = s.progress.portal;
    let didn_t = watched - fought;
    let col = if didn_t == 0 {
        Color::from_rgb(0.36, 0.78, 0.45)
    } else {
        Color::from_rgb(0.83, 0.33, 0.33)
    };
    widget::tooltip(
        text(didn_t.to_string())
            .size(fs)
            .style(theme::Text::Color(col))
            .width(Length::FillPortion(8)),
        text(format!(
            "Portals: {fought} {} / {didn_t} {} (last 14 days)",
            t("participated"),
            t("didn't participate")
        ))
        .size(fs),
        widget::tooltip::Position::Left,
    )
    .gap(4)
    .style(theme::Container::Box)
    .into()
}

/// The guild-wide summary that lives in the drawer above the table:
/// today's duties in numbers, the four bonus bars, the kick advice and
/// who joined in the last day
fn member_summary<'a>(
    guild: &sf_api::gamestate::guild::Guild,
    history: &GuildHistory,
    history_key: &str,
    strictness: u8,
    seats: u8,
    ident: crate::AccountIdent,
    tools: bool,
) -> Element<'a, Message> {
    let stats = member_stats(
        &guild.members,
        guild.attacking.is_some(),
        guild.defending.is_some(),
        history,
        history_key,
        strictness,
        seats,
    );
    let (max_treasure, max_instructor, max_knights) = guild_ceilings(guild);
    let all_raised = guild.members.iter().all(|m| {
        m.treasure_skill >= max_treasure && m.instructor_skill >= max_instructor
    });

    let n = stats.len().max(1);
    let portal_done = stats.iter().filter(|s| s.portal_today).count();
    let potions_full = stats.iter().filter(|s| s.potions == 3).count();
    let online_day = stats
        .iter()
        .filter(|s| s.online_hours.is_some_and(|h| h < 24))
        .count();
    let avg_level =
        stats.iter().map(|s| u32::from(s.member.level)).sum::<u32>() / n as u32;
    let avg_activity = stats.iter().map(|s| s.activity).sum::<f32>() / n as f32;

    let kicks: Vec<String> = stats
        .iter()
        .filter(|s| s.verdict == Verdict::Kick)
        .map(|s| {
            format!(
                "{} ({})",
                titlecase::titlecase(&s.member.name),
                s.reasons.join(", ")
            )
        })
        .collect();
    let watch = stats.iter().filter(|s| s.verdict == Verdict::Watch).count();
    // The officer bench: who should take one of the nine seats and who
    // should give theirs back, with the count as it stands now
    let named = |v: Verdict| -> Vec<String> {
        stats
            .iter()
            .filter(|s| s.verdict == v)
            .map(|s| {
                format!(
                    "{} {:.0}",
                    titlecase::titlecase(&s.member.name),
                    s.contribution.total
                )
            })
            .collect()
    };
    let promote = named(Verdict::Promote);
    let demote = named(Verdict::Demote);
    let officers_now = stats
        .iter()
        .filter(|s| {
            matches!(
                s.member.guild_rank,
                sf_api::gamestate::guild::GuildRank::Officer
            )
        })
        .count();
    // How many seats the leadership is built around, with a step either
    // way. The advice is only ever as good as this number, so it sits on
    // the line it decides rather than three pages away in Settings
    let seat_step = |label: &'static str, to: u8, on: bool| {
        button(text(label).size(11))
            .padding([1, 5])
            .style(crate::ui::style::quiet())
            .on_press_maybe(
                on.then_some(Message::SetOfficerSeats { ident, n: to }),
            )
    };
    let advice: Element<Message> = if promote.is_empty() && demote.is_empty() {
        text(t("the right people hold the seats"))
            .size(12)
            .style(theme::Text::Color(crate::ui::style::MUTED))
            .into()
    } else {
        // The two words carry opposite meanings and were set in the same
        // colour, in one run of text, with the names of both groups
        // between them. Green for the ones going up, red for the ones
        // coming down - which is the app's own palette everywhere else,
        // and is read before the words are
        let mut parts: Vec<Element<Message>> = Vec::new();
        let names = Color::from_rgb(0.45, 0.68, 0.95);
        let mut group = |label: &'static str, colour, who: &[String]| {
            if who.is_empty() {
                return;
            }
            if !parts.is_empty() {
                parts.push(
                    text("\u{00B7}")
                        .size(12)
                        .style(theme::Text::Color(crate::ui::style::MUTED))
                        .into(),
                );
            }
            parts.push(
                text(t(label))
                    .size(12)
                    .style(theme::Text::Color(colour))
                    .into(),
            );
            parts.push(
                text(who.join(", "))
                    .size(12)
                    .style(theme::Text::Color(names))
                    .into(),
            );
        };
        group("promote:", crate::ui::style::GOOD, &promote);
        group("demote:", crate::ui::style::BAD, &demote);
        // Wrapped rather than one long row: a guild with eight people to
        // promote must push the line onto a second one, not off the page
        iced_aw::Wrap::with_elements(parts)
            .spacing(5.0)
            .line_spacing(2.0)
            .into()
    };
    // "33/25" read like a fraction and said nothing about what the
    // buttons change. In words: how many officers there are now, and how
    // many the advice is built for - the leader is on top of that number,
    // which is why it says so
    let now_col = match officers_now.cmp(&usize::from(seats)) {
        std::cmp::Ordering::Greater => Color::from_rgb(0.80, 0.55, 0.25),
        std::cmp::Ordering::Equal => Color::from_rgb(0.36, 0.78, 0.45),
        std::cmp::Ordering::Less => crate::ui::style::MUTED,
    };
    let promote_line: Element<Message> = row!(
        text(format!("{}:", t("Officers"))).size(12),
        text(officers_now.to_string())
            .size(12)
            .style(theme::Text::Color(now_col)),
        text(t("now, plan for")).size(12),
        seat_step("\u{2013}", seats.saturating_sub(1).max(1), seats > 1),
        text(seats.to_string()).size(12),
        seat_step(
            "+",
            (seats + 1).min(crate::guild_report::OFFICER_SEATS_MAX),
            seats < crate::guild_report::OFFICER_SEATS_MAX
        ),
        text(t("+ the leader")).size(12),
        text("\u{00B7}")
            .size(12)
            .style(theme::Text::Color(crate::ui::style::MUTED)),
        advice,
    )
    .spacing(5)
    .align_items(Alignment::Center)
    .into();

    let advice_line: Element<Message> = if kicks.is_empty() {
        text(format!(
            "Kick advice: nobody right now, {watch} to keep an eye on"
        ))
        .size(12)
        .style(theme::Text::Color(Color::from_rgb(0.36, 0.78, 0.45)))
        .into()
    } else {
        text(format!(
            "Kick advice ({}): {} \u{00B7} {watch} more to keep an eye on",
            kicks.len(),
            kicks.join("; ")
        ))
        .size(12)
        .style(theme::Text::Color(Color::from_rgb(0.90, 0.45, 0.45)))
        .into()
    };

    // Who genuinely turned up in the last day: seen on a single day AND that
    // day is later than when our diary itself started (see member_stats), so a
    // fresh diary does not brand the whole guild as new. They are the ones on
    // the 24 h grace, who can not raid yet and must not be judged for it
    let new_members: Vec<String> = stats
        .iter()
        .filter(|s| s.just_joined)
        .map(|s| titlecase::titlecase(&s.member.name))
        .collect();
    let new_line: Element<Message> = if new_members.is_empty() {
        column!().into()
    } else {
        text(format!(
            "{} ({}): {}",
            t("Joined in the last day - 24h grace, can't raid yet"),
            new_members.len(),
            new_members.join(", ")
        ))
        .size(12)
        .style(theme::Text::Color(Color::from_rgb(0.45, 0.68, 0.95)))
        .into()
    };

    let gold = bonus_progress(guild.total_treasure_skill, guild.finished_raids);
    let xp = bonus_progress(guild.total_instructor_skill, guild.finished_raids);
    let raids = guild.finished_raids.min(BONUS_RAIDS_MAX);
    // The Hall of Knights is what the guild raids on, so the number that
    // matters is not "how many sit at whatever our best happens to be" -
    // a guild where everybody is at 4 would read 100 % - but how many
    // have reached the level worth reaching
    let hok_target = guild
        .members
        .iter()
        .filter(|m| u16::from(m.knights) >= HOK_TARGET)
        .count();
    let hok_top = guild
        .members
        .iter()
        .filter(|m| u16::from(m.knights) >= max_knights && max_knights > 0)
        .count();
    column!(
        new_line,
        text(format!(
            "Portal today {portal_done}/{n} \u{00B7} 3 potions {potions_full}/{n} \u{00B7} \
             online <24h {online_day}/{n} \u{00B7} avg level {avg_level} \u{00B7} \
             avg activity {avg_activity:.0}% \u{00B7} {}",
            if all_raised {
                t("everyone raised to the guild maximum").to_string()
            } else {
                format!(
                    "{} {}",
                    guild
                        .members
                        .iter()
                        .filter(|m| m.treasure_skill < max_treasure
                            || m.instructor_skill < max_instructor)
                        .count(),
                    t("still to raise")
                )
            }
        ))
        .size(12),
        // What the whole guild gets out of quests, drawn rather than
        // written - four bars answer "how far are we" at a glance, and the
        // raw numbers sit next to each one so the bar can be checked
        bonus_bar(
            t("Guild bonus"),
            (gold + xp) / 2.0,
            format!("{raids}/{BONUS_RAIDS_MAX} {}", t("raids")),
        ),
        bonus_bar(
            t("Treasure (gold)"),
            gold,
            format!("{}/{GUILD_SKILL_MAX}", guild.total_treasure_skill),
        ),
        bonus_bar(
            t("Instructor (XP)"),
            xp,
            format!("{}/{GUILD_SKILL_MAX}", guild.total_instructor_skill),
        ),
        bonus_bar(
            t("Hall of Knights"),
            if n == 0 {
                0.0
            } else {
                hok_target as f32 / n as f32 * 100.0
            },
            format!(
                "{hok_target}/{n} {} {HOK_TARGET}+ · {hok_top} {} {max_knights}",
                t("at"),
                t("at the guild best")
            ),
        ),
    )
    .spacing(2)
    // The kick advice and the officer seats are the leadership's business
    // (6.1.2) - an ordinary member does not need them on the screen
    .push_maybe(tools.then_some(advice_line))
    .push_maybe(tools.then_some(promote_line))
    .into()
}

/// One of the "mark the best / worst N" switches: none, five, ten
fn mark_switch<'a>(top: bool, current: u8) -> Element<'a, Message> {
    let mut r = row!().spacing(2);
    for (label, n) in [("\u{2013}", 0u8), ("5", 5), ("10", 10)] {
        r = r.push(
            button(text(label).size(11))
                .padding([2, 6])
                .style(if current == n {
                    crate::ui::style::primary()
                } else {
                    crate::ui::style::quiet()
                })
                .on_press(Message::GuildMark { top, n }),
        );
    }
    r.into()
}

/// The member table: every member on one screen, no scrolling for a full
/// guild. The rows carry the verdict in colour - red for a kick, orange
/// for a watch - and the best / worst N by grade are washed green / red
/// so an officer sees at a glance where to look
fn view_members<'a>(
    player: &'a AccountInfo,
    // How big the window is, so the table can work out its own type size
    // and row height instead of guessing at them
    window_w: f32,
    window_h: f32,
    // Not tied to 'a: the guild lives behind a mutex guard, that is dropped
    // when the view returns. Every widget copies what it shows
    guild: &sf_api::gamestate::guild::Guild,
    history: &GuildHistory,
    history_key: &str,
    strictness: u8,
    officer_seats: u8,
    top_mark: u8,
    bottom_mark: u8,
    two_columns: bool,
    tools: bool,
) -> Element<'a, Message> {
    let ident = player.ident;
    let compact = player.social.compact_members;
    let one_col = !two_columns;
    // **Everybody has to fit under the header without scrolling.** This
    // table is read as a whole - the leader looks down the marks column
    // and sees who is falling behind - and a table you have to scroll is
    // a table you read in halves.
    //
    // A fixed size could not do that: at 13 px a row is ~17 px, which is
    // fine for thirty members and eight hundred pixels for fifty. So the
    // type shrinks with the guild instead, and the manual tick takes it
    // one step smaller again for a shorter screen
    let head_count = guild.members.len();
    // The roster is dealt into two columns side by side, so what decides
    // the type size is half the guild, not all of it. Fifty members read at
    // the size twenty-five used to, which is the whole point of splitting
    // How wide the table is allowed to be. The two framing columns are
    // fixed - the nav plates on the left, the feeds on the right - so what
    // is left for the page is arithmetic, not a guess
    const NAV_W: f32 = 190.0;
    const FEED_W: f32 = 0.0;
    // The card's own padding, the page inset and the scroll gutter.
    // Measured off a screenshot at 1920 rather than added up from the
    // widgets: every time this was reasoned about it came out twenty or
    // thirty pixels short, and thirty pixels is a black strip down the
    // side of the table. Erring high is the safe direction - what is
    // left over goes to the activity lane below, and what is over-spent
    // has nowhere to go but off the edge
    const CHROME: f32 = 48.0;
    const COLUMN_GAP: f32 = 18.0;
    let page_w = (window_w - NAV_W - FEED_W - CHROME).max(600.0);

    // How tall a row is allowed to get. Not how tall it *is*: the rows
    // divide the real height between them (see the bottom of this
    // function), because every attempt to work that height out from a
    // constant for the strip, the heading and the sorting row above has
    // been out by twenty or thirty pixels - which is a scrollbar. The cap
    // only stops a guild of eight from getting rows an inch tall
    let per_column = if one_col {
        head_count.max(1)
    } else {
        head_count.div_ceil(2).max(1)
    };
    let row_cap = (window_h / per_column as f32).clamp(18.0, 34.0);

    let raid_on = guild.attacking.is_some() || guild.defending.is_some();

    // ------------------------------------------------------- lane widths
    //
    // Each lane is measured from the strings that will actually stand in
    // it, not from a count of characters times a factor. A drawn line's
    // width scales exactly with its height, so measuring once at a canvas
    // of one pixel answers for every type size - and the type size is then
    // solved for rather than searched for, which is what makes the two
    // halves meet the edges of the page instead of stopping short.
    //
    // The name lane in particular used to be given room for twenty
    // characters whether or not anybody in the guild had a name that long.
    // Twenty characters of allowance against ten of name is a finger's
    // width of black in every row, and it came out of the type size
    // Measured in the face that is actually switched on, at a text size of
    // one, so one measurement answers for every size the solver tries
    let unit = |samples: &[&str]| -> f32 {
        samples
            .iter()
            .map(|s| super::glyphs::width_at(s, 1.0))
            .fold(0.0_f32, f32::max)
    };
    let top_level = guild
        .members
        .iter()
        .map(|m| m.level)
        .max()
        .unwrap_or(999)
        .to_string();
    // One joker with a forty-character name must not shrink the type for
    // the other forty-nine, so a name is measured to a sane bound
    let u_name = guild
        .members
        .iter()
        .map(|m| {
            let cut: String = m.name.chars().take(18).collect();
            super::glyphs::width_at(&cut, 1.0)
        })
        .fold(unit(&[t("Name")]), f32::max);

    let u_grade = unit(&["5", t("Gr.")]);
    let u_advice = unit(&[
        t("Advice"),
        t("keep"),
        t("OFFICER"),
        t("demote"),
        t("watch"),
        t("KICK"),
        t("Guild rank"),
        t("leader"),
        t("officer"),
        t("invited"),
        t("member"),
    ]);
    let u_level = unit(&[&top_level, t("Level")]);
    let u_gain = unit(&["+999", t("+7d")]);
    let u_battles = unit(&["999 / 99", t("Btl.")]);
    let u_flag = unit(&[t("Atk"), t("Def"), "out"]);
    // The bar shares this lane with the figure and stretches to whatever it
    // is given, so the lane is the figure with room for a bar beside it.
    //
    // This multiplier was 2.4, and a measurement of where a row's width
    // actually went said that one lane was taking a quarter of it - 136 px
    // to show a short bar and "33%" - which came straight out of the type
    // size for every other column. A bar reads perfectly well at sixty
    let u_activity = unit(&["100%", t("Activity")]) * 1.6;
    let u_skills = unit(&["99 / 99 / 99", t("Tr/In/HoK")]);
    let u_small = unit(&["9", t("Prt.")]);
    let u_online = unit(&["999+ d", t("Online")]);
    // How long they have been in the guild. It was living in the online
    // cell's tooltip, which is to say it was not on the table at all
    let u_tenure = unit(&["999+ d", t("In guild")]);

    let mut units = u_grade
        + u_advice
        + u_name
        + u_level
        + u_gain
        + u_battles
        + u_activity
        + u_skills
        + u_small
        + u_online
        + u_tenure;
    let mut lane_count = 11.0_f32;
    if raid_on {
        units += u_flag * 2.0;
        lane_count += 2.0;
    }

    // What the lanes cannot pay for: the padding inside each cell and the
    // gaps between them
    const CELL_PAD: f32 = 6.0;
    const LANE_GAP: f32 = 4.0;
    let chrome = CELL_PAD * lane_count + LANE_GAP * (lane_count - 1.0) + 14.0;
    let half_w = if one_col {
        // One column has the whole page; the scroll gutter comes off it
        (page_w - 16.0).max(480.0)
    } else {
        ((page_w - COLUMN_GAP) / 2.0).max(240.0)
    };

    // Solve for the type size that fills the half exactly, then round down
    // to a whole point. The remainder after rounding is shared out over the
    // lanes in proportion, so there is no dead strip at the right edge and
    // no one lane carrying all the slack
    // `units` are per point of type, so the size that fills the half is a
    // division and not a search
    let ideal = ((half_w - chrome) / units).max(1.0);
    let fs = if one_col {
        // Readable first: 13 to 16 px, never squeezed under 13
        (ideal.floor() as u16).clamp(13, 16)
    } else {
        (ideal.floor() as u16).clamp(9, 22)
    };
    let fs = fs.saturating_sub(u16::from(compact)).max(8);
    // A row in the one-column table has a height of its own - it sits in
    // a scrollable, which has no height to share out
    let one_row_h = f32::from(fs) * if compact { 1.7 } else { 2.2 };
    let stretch = if units * f32::from(fs) > 0.0 {
        ((half_w - chrome) / (units * f32::from(fs))).clamp(1.0, 1.6)
    } else {
        1.0
    };
    let lane = |u: f32| Length::Fixed(u * f32::from(fs) * stretch + CELL_PAD);
    let c_grade = lane(u_grade);
    let c_advice = lane(u_advice);
    let c_name = lane(u_name);
    let c_level = lane(u_level);
    let c_gain = lane(u_gain);
    let c_battles = lane(u_battles);
    let c_flag = lane(u_flag);
    // The one lane that takes what is left. The type size is solved from
    // whole points, so there is always a remainder, and a remainder has
    // to land somewhere: either in a dead strip at the right edge, or in
    // the one lane that is a bar and reads better for being longer
    let c_activity = Length::Fill;
    let c_skills = lane(u_skills);
    let c_small = lane(u_small);
    let c_online = lane(u_online);
    let c_tenure = lane(u_tenure);
    let mut stats = member_stats(
        &guild.members,
        guild.attacking.is_some(),
        guild.defending.is_some(),
        history,
        history_key,
        strictness,
        officer_seats,
    );
    let (max_treasure, max_instructor, max_knights) = guild_ceilings(guild);

    // The best and the worst N by grade, whatever the table is sorted by
    let mut by_grade: Vec<(u8, f32, String)> = stats
        .iter()
        .map(|s| (s.grade, s.kick_score, s.member.name.to_lowercase()))
        .collect();
    by_grade.sort_by(|a, b| {
        a.0.cmp(&b.0).then_with(|| {
            a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)
        })
    });
    let top: std::collections::HashSet<String> = by_grade
        .iter()
        .take(usize::from(top_mark))
        .map(|(_, _, n)| n.clone())
        .collect();
    let bottom: std::collections::HashSet<String> = by_grade
        .iter()
        .rev()
        .take(usize::from(bottom_mark))
        .map(|(_, _, n)| n.clone())
        .filter(|n| !top.contains(n))
        .collect();

    match player.social.member_sort {
        // `stats` is built in the order the game sent the members - left
        // as it is
        MemberSort::Game => {}
        MemberSort::Advice => stats.sort_by(|a, b| {
            b.grade.cmp(&a.grade).then_with(|| {
                b.kick_score
                    .partial_cmp(&a.kick_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        }),
        // Most contribution first: the nine seats are the top nine rows
        // below the leader, so the cut line is where the promotions and
        // the demotions meet
        MemberSort::Officers => {
            stats.sort_by(|a, b| a.contribution_rank.cmp(&b.contribution_rank))
        }
        MemberSort::Activity => stats.sort_by(|a, b| {
            b.activity
                .partial_cmp(&a.activity)
                .unwrap_or(std::cmp::Ordering::Equal)
        }),
        // Lowest first: whoever is still waiting to be raised is at the top,
        // and a guild that has everyone maxed shows a flat list
        MemberSort::ToRaise => stats.sort_by_key(|s| {
            u32::from(s.member.treasure_skill)
                + u32::from(s.member.instructor_skill)
        }),
        MemberSort::Level => {
            stats.sort_by_key(|s| std::cmp::Reverse(s.member.level))
        }
        MemberSort::Name => stats.sort_by(|a, b| {
            a.member
                .name
                .to_lowercase()
                .cmp(&b.member.name.to_lowercase())
        }),
        MemberSort::LastOnline => stats.sort_by(|a, b| {
            b.online_hours
                .unwrap_or(-1)
                .cmp(&a.online_hours.unwrap_or(-1))
        }),
    }

    if two_columns {
        return roster_overview(player, &stats, window_w, window_h);
    }

    // Is a raid actually declared right now? The Atk/Def columns only carry
    // meaning while one is - otherwise there is nobody to be in or out of
    let atk_planned = guild.attacking.is_some();
    let def_planned = guild.defending.is_some();
    // Both halves get one, so it is built twice
    let build_header = || {
        let head = |label: &'static str, width: Length| {
            text(t(label))
                .size(fs)
                .style(theme::Text::Color(crate::ui::style::GOLD_DIM))
                .width(width)
        };
        // Ten lanes, not eighteen. Half a page cannot hold eighteen, and
        // most of the eight that went are still on the row - the rank is the
        // colour of the name, the three skills share one cell, and the
        // month's levels sit behind the week's in the tooltip. The two
        // sign-up lanes come back on their own the moment a battle is
        // actually declared, because that is the only time they say anything
        let mut header = row!(
            head("Gr.", c_grade),
            head(if tools { "Advice" } else { "Guild rank" }, c_advice),
            head("Name", c_name),
            head("Level", c_level),
            head("+7d", c_gain),
            head("Btl.", c_battles),
        );
        if atk_planned || def_planned {
            header = header.push(head("Atk", c_flag));
            header = header.push(head("Def", c_flag));
        }
        header
            .push(head("Activity", c_activity))
            .push(head("Tr/In/HoK", c_skills))
            .push(head("Prt.", c_small))
            .push(head("Online", c_online))
            .push(head("In guild", c_tenure))
            .spacing(4)
    };
    // The header lines up with the rows, which are padded to make room for
    // the wash behind them
    let header_left: Element<Message> =
        container(build_header()).padding([1, 6]).into();
    let header_right: Element<Message> =
        container(build_header()).padding([1, 6]).into();

    // Days in the guild come from our own diary. The earliest day we ever saw
    // anyone is the day tracking began: members first seen on that day were
    // already here, so their true tenure can be older - shown with a "="
    // (at least). Anyone first seen later joined then, so theirs is exact
    let today = chrono::Local::now().date_naive();
    let track_start = stats.iter().filter_map(|s| s.progress.first_seen).min();
    // What the leader typed in the box next to the member count. Only his
    // own row is built from it; for everybody else it is the guild's age
    // and nothing more
    let founded = parse_date(&player.social.guild_founded);

    let mut rows: Vec<Element<Message>> = Vec::with_capacity(head_count);
    for (row_no, s) in stats.iter().enumerate() {
        let m = s.member;
        use sf_api::gamestate::guild::GuildRank;
        let in_attack = matches!(
            m.battles_joined,
            Some(BattlesJoined::Attack | BattlesJoined::Both)
        );
        let in_defense = matches!(
            m.battles_joined,
            Some(BattlesJoined::Defense | BattlesJoined::Both)
        );
        let is_me = m.name.eq_ignore_ascii_case(&player.name);
        // The name as the game spells it - titlecase was turning
        // "GussOlean" into "Gussolean". A bit bigger and brighter than the
        // numbers around it, because it is what the eye looks for first
        // The name carries the rank now, in its colour: the leader in the
        // app's gold, officers in blue, an invitation not yet taken in
        // grey. That is the whole of what the rank lane used to say, and it
        // says it in the one place the eye already goes
        let name_txt = text(&m.name).size(fs + 1).width(c_name).style(
            theme::Text::Color(match m.guild_rank {
                GuildRank::Leader => crate::ui::style::NAME_GOLD,
                GuildRank::Officer => Color::from_rgb(0.45, 0.68, 0.95),
                GuildRank::Invited => crate::ui::style::MUTED,
                GuildRank::Member if is_me => {
                    Color::from_rgb(0.55, 0.78, 0.98)
                }
                GuildRank::Member => Color::from_rgb(0.93, 0.94, 0.96),
            }),
        );

        let (v_txt, v_col) = if !tools {
            // Without the leadership tools the lane says the rank, not what
            // to do with the member
            match m.guild_rank {
                GuildRank::Leader => (t("leader"), crate::ui::style::NAME_GOLD),
                GuildRank::Officer => {
                    (t("officer"), Color::from_rgb(0.45, 0.68, 0.95))
                }
                GuildRank::Invited => (t("invited"), crate::ui::style::MUTED),
                GuildRank::Member => (t("member"), crate::ui::style::MUTED),
            }
        } else {
            match s.verdict {
            // The app's own palette. Blue stays what it has always meant
            // here and nowhere else: an officer
            Verdict::Keep => (t("keep"), crate::ui::style::GOOD),
            Verdict::Promote => {
                (t("OFFICER"), Color::from_rgb(0.45, 0.68, 0.95))
            }
            Verdict::Demote => (t("demote"), Color::from_rgb(0.80, 0.55, 0.25)),
            Verdict::Watch => (t("watch"), crate::ui::style::WARN),
            Verdict::Kick => (t("KICK"), crate::ui::style::BAD),
            }
        };
        let grade_col = match s.grade {
            1 => Color::from_rgb(0.36, 0.78, 0.45),
            2 => Color::from_rgb(0.55, 0.75, 0.35),
            3 => Color::from_rgb(0.85, 0.72, 0.20),
            4 => Color::from_rgb(0.90, 0.55, 0.30),
            _ => Color::from_rgb(0.90, 0.40, 0.40),
        };
        let grade_cell = widget::tooltip(
            text(s.grade.to_string())
                .size(fs + 1)
                .style(theme::Text::Color(grade_col))
                .width(c_grade),
            text(s.reasons.join(", ")).size(fs),
            widget::tooltip::Position::Right,
        )
        .gap(4)
        .style(theme::Container::Box);
        let verdict_cell = text(v_txt)
            .size(fs)
            .style(theme::Text::Color(v_col))
            .width(c_advice);
        let gain = |g: Option<u16>| match g {
            Some(v) => format!("+{v}"),
            None => "?".to_string(),
        };
        // in = green, out = red, and a quiet dash when there is no raid of
        // that kind to be in or out of
        let signup_cell = |planned: bool, joined: bool| -> Element<Message> {
            let (txt, col) = if !planned {
                ("\u{2013}", crate::ui::style::MUTED)
            } else if joined {
                ("in", Color::from_rgb(0.36, 0.78, 0.45))
            } else {
                ("out", Color::from_rgb(0.90, 0.40, 0.40))
            };
            text(txt)
                .size(fs)
                .style(theme::Text::Color(col))
                .width(c_flag)
                .into()
        };
        let atk_cell = signup_cell(atk_planned, in_attack);
        let def_cell = signup_cell(def_planned, in_defense);

        // Time in the guild, out of the app's own diary - the game does
        // not remember it. Exact for anybody who turned up while the app
        // was watching; "25+ d" for the ones who were already here on the
        // diary's first day, because that is a floor and not a figure.
        // It used to print "\u{2265}25 d", which is the same thing said in
        // maths instead of Czech
        let tenure: Element<Message> = {
            let (txt, tip) = match s.progress.first_seen {
                Some(fs) => {
                    let days = (today - fs).num_days().max(0);
                    if track_start == Some(fs) {
                        // Already a member on the diary's first day, so
                        // the diary has nothing to say about how long
                        // they have really been here. It used to print
                        // "in the guild since at least <the day I started
                        // looking>", which is a fact about the app and
                        // not about the person - and the leader,
                        // reading his own table, could not tell which of
                        // the people above him had been here for years
                        // and which had turned up the week before.
                        //
                        // The one member who does know is whoever founded
                        // the guild, about himself. So: his own row is
                        // counted from the founding date he typed in, and
                        // everybody else in that cohort gets an honest
                        // question mark
                        match founded.filter(|_| is_me) {
                            Some(f) => (
                                format!("{} d", (today - f).num_days().max(0)),
                                format!(
                                    "{} {}",
                                    t("In the guild since"),
                                    f.format("%-d. %-m. %Y")
                                ),
                            ),
                            // Everybody else keeps the floor the diary can
                            // actually vouch for. Turning theirs into a
                            // question mark took away a number that was
                            // being read every day and put nothing in its
                            // place - the founding date was only ever
                            // meant to fix the leader's own row
                            None => (
                                format!("{days}+ d"),
                                format!(
                                    "{} {} \u{2013} {}",
                                    t("In the guild since at least"),
                                    fs.format("%-d. %-m. %Y"),
                                    t("already a member when the app started \
                                       watching, so it may be much longer")
                                ),
                            ),
                        }
                    } else {
                        (
                            format!("{days} d"),
                            format!(
                                "{} {}",
                                t("Joined"),
                                fs.format("%-d. %-m. %Y")
                            ),
                        )
                    }
                }
                None => (
                    "?".to_string(),
                    t("Not in the app's diary yet").to_string(),
                ),
            };
            // Two different facts, and they were sharing one cell: the
            // cell showed when somebody was last online and kept how long
            // they have been in the guild in the tooltip, where nobody
            // reads it. A leader looking down his own table for who is
            // new and who has been here a year was being asked to hover
            // over fifty rows one at a time
            widget::tooltip(
                text(txt).size(fs).width(c_tenure),
                text(tip).size(fs),
                widget::tooltip::Position::Left,
            )
            .gap(4)
            .style(theme::Container::Box)
            .into()
        };

        let online_cell: Element<Message> = text(online_text(s.online_hours))
            .size(fs)
            .width(c_online)
            .into();

        // Treasure, instructor and the Hall of Knights in one lane, each
        // still coloured against the guild's best. Three lanes of two
        // digits was a third of the row's width spent on numbers nobody
        // reads one at a time.
        //
        // In the game's own order: treasure, instructor, then the hall.
        // This had the hall first, which is not the order anybody who
        // plays it reads them in
        let skills_cell: Element<Message> = row!(
            level_cell(m.treasure_skill, max_treasure, fs),
            text("/").size(fs).style(theme::Text::Color(
                crate::ui::style::GOLD_DIM
            )),
            level_cell(m.instructor_skill, max_instructor, fs),
            text("/").size(fs).style(theme::Text::Color(
                crate::ui::style::GOLD_DIM
            )),
            level_cell(u16::from(m.knights), max_knights, fs),
        )
        .spacing(2)
        .align_items(Alignment::Center)
        .width(Length::Shrink)
        .into();

        let striped = row_no % 2 == 1;
        // The colour of the row says what to do: green for the best N,
        // red for the worst N and for every kick, orange for a watch - the
        // rest keeps the quiet stripe
        let lower = m.name.to_lowercase();
        let wash = if !tools {
            None
        } else if s.verdict == Verdict::Promote {
            Some(Color::from_rgb(0.45, 0.68, 0.95))
        } else if top.contains(&lower) {
            Some(crate::ui::style::GOOD)
        } else if bottom.contains(&lower) || s.verdict == Verdict::Kick {
            Some(crate::ui::style::BAD)
        } else if s.verdict == Verdict::Watch {
            Some(crate::ui::style::WARN)
        } else {
            None
        };
        let mut line = row!(
            grade_cell,
            verdict_cell,
            name_txt,
            text(m.level).size(fs).width(c_level),
            // The month sits behind the week in the tooltip: on a half-width
            // table the week is what is read and the month is what is asked
            // for
            widget::tooltip(
                text(gain(s.progress.levels_7d)).size(fs).width(c_gain),
                text(format!(
                    "{} {}",
                    t("in the last 30 days"),
                    gain(s.progress.levels_30d)
                ))
                .size(fs),
                widget::tooltip::Position::Top,
            )
            .gap(4)
            .style(theme::Container::Box),
            container(battles_cell(s, fs)).width(c_battles),
        );
        if atk_planned || def_planned {
            line = line.push(atk_cell);
            line = line.push(def_cell);
        }
        let line = line
            .push(container(pct_cell(s.activity, 11, compact, fs)).width(
                c_activity,
            ))
            .push(container(skills_cell).width(c_skills))
            .push(container(portal_cell(s, fs)).width(c_small))
            .push(online_cell)
            .push(tenure)
            .spacing(4)
            .align_items(Alignment::Center);
        rows.push(
            container(line)
            .padding([0, 7])
            // An equal share of whatever is left below the header, so the
            // table reaches the bottom of the window and the rows are as
            // far apart as they can be. Air between them is what keeps the
            // eye on one line across fifteen small numbers
            .height(if one_col {
                Length::Fixed(one_row_h)
            } else {
                Length::FillPortion(1)
            })
            .max_height(if one_col { one_row_h } else { row_cap })
            .center_y()
            .style(match wash {
                Some(c) => crate::ui::style::tint(c),
                None if striped => crate::ui::style::stripe(),
                None => iced::theme::Container::Transparent,
            })
            .into(),
        );
    }

    // The rating text itself is gone from the page - it was a wall of
    // small print under the table. The button still puts it on the
    // clipboard: the whole roster ranked by contribution, twenty-five,
    // twenty and the last five (see `guild_report::build_report`)
    let preview = crate::guild_report::build_report(
        guild, history, history_key, strictness, officer_seats,
    );
    let mut sort = row!(
        text(t("Sort by")).size(12),
        pick_list(
            MemberSort::ALL,
            Some(player.social.member_sort),
            move |sort| { Message::GuildMemberSort { ident, sort } }
        )
        .text_size(12)
        .padding(3),
        checkbox(t("Compact rows"), compact)
            .style(crate::ui::style::check())
            .on_toggle(move |state| Message::GuildCompact { ident, state })
            .size(14)
            .text_size(12),
        checkbox(t("Entire guild"), two_columns)
            .style(crate::ui::style::check())
            .on_toggle(Message::GuildTwoColumns)
            .size(14)
            .text_size(12),
    )
    .spacing(6)
    .align_items(Alignment::Center);
    if tools {
        sort = sort
            .push(text(t("Mark best")).size(12))
            .push(mark_switch(true, top_mark))
            .push(text(t("worst")).size(12))
            .push(mark_switch(false, bottom_mark))
            .push(
                button(text(t("Copy the ladder")).size(11))
                    .padding(3)
                    .style(crate::ui::style::quiet())
                    .on_press(Message::GuildReportCopy {
                        ident,
                        text: preview,
                    }),
            )
            .push(
                text(player.social.report_status.clone().unwrap_or_default())
                    .size(10)
                    .style(theme::Text::Color(Color::from_rgb(0.95, 0.80, 0.40))),
            )
            .push(horizontal_space())
            .push(text(t("Grading:")).size(12))
            .push(text(t("lenient")).size(11))
            .push(
                iced::widget::slider(
                    0..=100u8,
                    strictness,
                    Message::SetGradeStrictness,
                )
                .style(crate::ui::style::slide())
                .width(Length::Fixed(120.0)),
            )
            .push(text(t("strict")).size(11))
            .push(text(format!("{strictness}")).size(11));
    }
    let sort = sort.spacing(6).align_items(Alignment::Center);

    // Dealt into two columns, the first half on the left. Fifty rows down
    // one side of a 1080p screen could only be made to fit by shrinking the
    // type to the point where the leader needed a magnifying glass; twenty
    // five down each side fit at a size that can be read, which is the
    // whole reason the table exists
    if one_col {
        // 6.1: one table, the header once, the rows in a scrollable at a
        // size that can be read. Forty-odd rows of 14 px are a scroll of
        // a screen and a bit - against ten-pixel type in two halves
        let body = rows.into_iter().fold(
            column!().spacing(0).width(Length::Fill),
            |c, r| c.push(r),
        );
        return column!(
            sort,
            header_left,
            horizontal_rule(1),
            iced::widget::scrollable(body).height(Length::Fill),
        )
        .spacing(4)
        .height(Length::Fill)
        .into();
    }
    let half = head_count.div_ceil(2);
    let mut right = rows.split_off(half.min(rows.len()));
    // Both halves are given the same width, worked out from the lanes. Left
    // to size themselves they took the width of their own longest row, so
    // the half with the longer names came out visibly fatter than the other
    // Half of whatever the page really is, not half of what the
    // arithmetic above thinks it is. The lanes are sized from that
    // arithmetic and the activity lane swallows the difference, so the
    // two halves meet the edges of the card whether the estimate was
    // exactly right or a little short
    let stack = |items: Vec<Element<'a, Message>>| {
        items.into_iter().fold(
            column!()
                .spacing(0)
                .width(Length::Fill)
                .height(Length::Fill),
            |c, r| c.push(r),
        )
    };
    let side = |head: Element<'a, Message>, body: Vec<Element<'a, Message>>| {
        column!(head, horizontal_rule(1), stack(body))
            .spacing(2)
            .width(Length::FillPortion(1))
            .height(Length::Fill)
    };
    // Both halves carry the header: a column of numbers with its heading on
    // the other side of the page is a column you have to count across to
    let left_side = side(header_left, std::mem::take(&mut rows));
    let right_side = side(header_right, std::mem::take(&mut right));

    // No scrollable around it any more. A scrollable hands its content an
    // unbounded height, which is exactly what stops the rows from being
    // able to divide the page between them - and a scrollbar here is the
    // one thing this table must never grow
    column!(
        sort,
        row!(left_side, right_side)
            .spacing(18)
            .height(Length::Fill)
            .align_items(Alignment::Start),
    )
    .spacing(4)
    .height(Length::Fill)
    .into()
}

/// A date the way a person types one: "21.8.2026", "21. 8. 2026",
/// "21/8/2026" or "2026-08-21". Anything else is no date at all, which is
/// what an empty box and a half-typed one both are
fn parse_date(raw: &str) -> Option<chrono::NaiveDate> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if let Ok(d) = chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d") {
        return Some(d);
    }
    let parts: Vec<i32> = raw
        .split(['.', '/', ' ', '-'])
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| p.parse().unwrap_or(-1))
        .collect();
    let [day, month, year] = parts[..] else {
        return None;
    };
    // Two digits means this century - "21.8.26" is 2026, not year 26
    let year = if (0..100).contains(&year) {
        year + 2000
    } else {
        year
    };
    chrono::NaiveDate::from_ymd_opt(
        year,
        u32::try_from(month).ok()?,
        u32::try_from(day).ok()?,
    )
}

/// A guildless player from the server crawl who would fit the guild
#[derive(Debug, Clone)]
pub struct Recruit {
    pub name: String,
    pub level: u16,
    pub class: Option<sf_api::gamestate::character::Class>,
}

/// Up to ten players with no guild whose level sits nearest the guild's
/// average - the crawl reads every player of the server for the arena, and
/// it knows who is unguilded, so recruiting can draw from it. Nobody
/// already invited this session, nobody the crawl found invalid
pub fn recruits(player: &AccountInfo, server: &ServerInfo) -> Vec<Recruit> {
    let avg = match &*player.status.lock_safe() {
        AccountStatus::Idle(_, gs) | AccountStatus::Busy(gs, _) => {
            gs.guild.as_ref().and_then(|g| {
                let n = g.members.len();
                (n > 0).then(|| {
                    g.members.iter().map(|m| u32::from(m.level)).sum::<u32>()
                        / n as u32
                })
            })
        }
        _ => None,
    };
    let Some(avg) = avg else {
        return Vec::new();
    };
    let crate::server::CrawlingStatus::Crawling {
        player_info, que, ..
    } = &server.crawling
    else {
        return Vec::new();
    };
    let invalid: std::collections::HashSet<String> = {
        let lock = que.lock_safe();
        lock.invalid_accounts
            .iter()
            .map(|a| a.to_lowercase())
            .collect()
    };
    let mut out: Vec<(u32, Recruit)> = player_info
        .values()
        .filter(|c| c.no_guild)
        .filter(|c| !invalid.contains(&c.name.to_lowercase()))
        .filter(|c| !player.social.invited.contains(&c.name.to_lowercase()))
        .map(|c| {
            (
                u32::from(c.level).abs_diff(avg),
                Recruit {
                    name: c.name.clone(),
                    level: c.level,
                    class: c.class,
                },
            )
        })
        .filter(|(diff, _)| *diff <= 25)
        .collect();
    out.sort_by_key(|(diff, r)| (*diff, r.name.clone()));
    out.into_iter().take(10).map(|(_, r)| r).collect()
}

/// The game's guild finder for a character without a guild: the list the
/// game offers, the bonuses of the ones looked up, a Join button each, and
/// the switch that lets the app pick the best one on its own
fn view_finder<'a>(
    player: &'a AccountInfo,
    ident: crate::AccountIdent,
) -> Element<'a, Message> {
    let social = &player.social;
    let head = row!(
        text(t("This character is not in a guild")).size(16),
        horizontal_space(),
        button(text(t("Read the guild finder")).size(13))
            .padding(4)
            .on_press(Message::GuildFinderRefresh { ident }),
    )
    .spacing(8)
    .align_items(Alignment::Center);

    let auto = checkbox(
        t(
            "Join the best guild on its own (treasure + instructor bonus, \
           every 30 min)",
        ),
        social.finder_auto,
    )
    .style(crate::ui::style::check())
    .on_toggle(move |state| Message::GuildSetFinderAuto { ident, state })
    .size(16)
    .text_size(12);

    let status = text(social.finder_status.clone().unwrap_or_default())
        .size(12)
        .style(theme::Text::Color(crate::ui::style::MUTED));

    let mut list = column!().spacing(2);
    if !social.finder.is_empty() {
        list = list.push(
            row!(
                text(t("Rank")).size(11).width(Length::Fixed(44.0)),
                text(t("Guild")).size(11).width(Length::Fill),
                text(t("Members")).size(11).width(Length::Fixed(60.0)),
                text(t("Levels")).size(11).width(Length::Fixed(110.0)),
                text(t("Treasure")).size(11).width(Length::Fixed(64.0)),
                text(t("Instructor")).size(11).width(Length::Fixed(70.0)),
                text("").size(11).width(Length::Fixed(60.0)),
            )
            .spacing(6),
        );
    }
    let best = crate::guild_finder::best(
        &social.finder, &social.finder_bonus, &social.finder_tried,
    )
    .map(|g| g.name.clone());
    for g in &social.finder {
        let bonus = social.finder_bonus.get(&g.name.to_lowercase());
        let tried = social
            .finder_tried
            .iter()
            .any(|x| x.eq_ignore_ascii_case(&g.name));
        let colour = if best.as_deref() == Some(g.name.as_str()) {
            crate::ui::style::GOOD
        } else if g.members >= 50 || tried {
            crate::ui::style::MUTED
        } else {
            Color::WHITE
        };
        let name = g.name.clone();
        let mut join =
            button(text(t(if tried { "Asked" } else { "Join" })).size(11))
                .padding(2);
        if !tried && g.members < 50 {
            join = join.on_press(Message::GuildFinderJoin { ident, name });
        }
        let num = |v: Option<u16>| {
            v.map(|x| x.to_string()).unwrap_or_else(|| "?".to_string())
        };
        list = list.push(
            row!(
                text(g.rank.to_string())
                    .size(11)
                    .width(Length::Fixed(44.0))
                    .style(theme::Text::Color(colour)),
                text(format!("{} [{}]", g.name, g.language))
                    .size(11)
                    .width(Length::Fill)
                    .style(theme::Text::Color(colour)),
                text(format!("{}/50", g.members))
                    .size(11)
                    .width(Length::Fixed(60.0))
                    .style(theme::Text::Color(colour)),
                text(format!(
                    "{}...{} (~{})",
                    g.min_level, g.max_level, g.avg_level
                ))
                .size(11)
                .width(Length::Fixed(110.0))
                .style(theme::Text::Color(colour)),
                text(num(bonus.map(|b| b.treasure)))
                    .size(11)
                    .width(Length::Fixed(64.0))
                    .style(theme::Text::Color(colour)),
                text(num(bonus.map(|b| b.instructor)))
                    .size(11)
                    .width(Length::Fixed(70.0))
                    .style(theme::Text::Color(colour)),
                container(join).width(Length::Fixed(60.0)),
            )
            .spacing(6)
            .align_items(Alignment::Center),
        );
    }

    column!(
        head,
        auto,
        status,
        scrollable(container(list).padding([0, 12, 0, 0])).height(Length::Fill),
    )
    .spacing(8)
    .padding(12)
    .into()
}

/// Guild overview with the chat
pub fn view_guild<'a>(
    player: &'a AccountInfo,
    // Passed down to the member table, which sizes its type from the width
    // and stretches its rows down to the height
    window_w: f32,
    window_h: f32,
    server_url: &str,
    history: &GuildHistory,
    strictness: u8,
    officer_seats: u8,
    marks: (u8, u8),
    // 6.1: the member table in one readable column (scrolls) or the old
    // two squeezed ones (fits the screen)
    two_columns: bool,
    // 6.1.2: the leadership tools ticked on by hand (a leader or an
    // officer always has them)
    leader_tools: bool,
    recruits: Vec<Recruit>,
    // The chat archive: whether it is on, and what it last had to say
    // (the file written, or why it could not be)
    archive: (bool, Option<&str>),
) -> Element<'a, Message> {
    let lock = player.status.lock_safe();

    let gs = match &*lock {
        AccountStatus::Idle(_, gs) | AccountStatus::Busy(gs, _) => gs,
        AccountStatus::FatalError(err) => {
            return text(format!("Error: {err}")).size(20).into();
        }
        _ => return text(t("Logging in")).size(20).into(),
    };

    let ident = player.ident;

    let Some(guild) = gs.guild.as_ref() else {
        return view_finder(player, ident);
    };

    let members = guild.members.len();

    let show_members = !player.social.show_chat;
    let tab = |label: &'static str, members_tab: bool| {
        button(text(label).size(12))
            .padding(4)
            .style(if show_members == members_tab {
                crate::ui::style::primary()
            } else {
                crate::ui::style::quiet()
            })
            .on_press(Message::GuildShowMembers {
                ident,
                show: members_tab,
            })
    };

    // The drawer: raids, sign-ups, odds, the bonus bars and the kick
    // advice - pulled out with one click and put away with the next, so
    // the table underneath keeps the whole screen
    let drawer_open = player.social.drawer_open;
    let drawer = button(
        text(format!(
            "{} {}",
            if drawer_open { "\u{25B4}" } else { "\u{25BE}" },
            t("Summary")
        ))
        .size(12),
    )
    .padding(4)
    .style(if drawer_open {
        crate::ui::style::primary()
    } else {
        crate::ui::style::quiet()
    })
    .on_press(Message::GuildDrawer { ident });

    // The guild's own age, typed once by whoever founded it. The app's
    // diary starts the day the app did, so this is the only way it can
    // know anything older - and it is deliberately one box, not a column:
    // it is a fact about the guild, not about each member
    let founded_at = parse_date(&player.social.guild_founded);
    let founded_box = widget::tooltip(
        row!(
            text(t("Founded")).size(11).style(
                theme::Text::Color(crate::ui::style::MUTED)
            ),
            text_input("21.8.2026", &player.social.guild_founded).font(crate::ui::style::FAT)
                .on_input(move |text| Message::SetGuildFounded { ident, text })
                .size(11)
                .padding(3)
                .width(Length::Fixed(86.0)),
            text(match founded_at {
                Some(f) => format!(
                    "{} d",
                    (chrono::Local::now().date_naive() - f).num_days().max(0)
                ),
                None => String::new(),
            })
            .size(11)
            .style(theme::Text::Color(crate::ui::style::GOOD)),
        )
        .spacing(4)
        .align_items(Alignment::Center),
        text(t(
            "The day this guild was founded. It sets the guild's age and it counts your own row from the real date - the diary only goes back to the day the app first looked, which for a founder is far too late. Nobody else's row is touched by it: theirs still says what the diary can vouch for.",
        ))
        .size(11),
        widget::tooltip::Position::Bottom,
    )
    .gap(4)
    .style(theme::Container::Box);

    // The leadership tools: kick advice, officer seats, marks, grading,
    // the raid button. On for a leader or an officer; for an ordinary
    // member only when ticked (6.1.2)
    let leading = guild
        .members
        .iter()
        .find(|m| m.name.eq_ignore_ascii_case(&player.name))
        .is_some_and(|m| {
            matches!(
                m.guild_rank,
                sf_api::gamestate::guild::GuildRank::Leader
                    | sf_api::gamestate::guild::GuildRank::Officer
            )
        });
    let tools = leading || leader_tools;
    let mut head = row!(
        text(&guild.name).size(18),
        text(format!("{members} {}", t("members"))).size(12),
        founded_box,
        tab(t("Members"), true),
        tab(t("Chat"), false),
        drawer,
        horizontal_space(),
    )
    .spacing(12)
    .align_items(Alignment::Center);
    if !leading {
        head = head.push(
            checkbox(t("Leadership tools"), leader_tools)
                .style(crate::ui::style::check())
                .on_toggle(Message::GuildLeaderTools)
                .size(14)
                .text_size(12),
        );
    }
    if tools {
        head = head.push(
            button(text(t("Start raid")).size(13))
                .padding(4)
                .style(crate::ui::style::quiet())
                .on_press(Message::GuildStartRaid { ident }),
        );
    }
    let head = head.push(
        button(text(t("Refresh")).size(13))
            .padding(4)
            .on_press(Message::GuildRefresh { ident }),
    );

    // ------------------------------------------------ battles + auto sign
    let own = guild
        .members
        .iter()
        .find(|m| m.name.eq_ignore_ascii_case(&player.name))
        .and_then(|m| m.battles_joined);
    let in_attack =
        matches!(own, Some(BattlesJoined::Attack | BattlesJoined::Both));
    let in_defense =
        matches!(own, Some(BattlesJoined::Defense | BattlesJoined::Both));

    // A marker in front of each battle. Deliberately built out of one glyph
    // that is known to render here plus colour, rather than a tick and a
    // cross: those came out as empty boxes in this font and the console had
    // to have them taken out again
    let battle_mark =
        |battle: &Option<sf_api::gamestate::guild::PlanedBattle>,
         joined: bool|
         -> Element<Message> {
            let (colour, hint) = match (battle.is_some(), joined) {
                (false, _) => (crate::ui::style::MUTED, "nothing planned"),
                (true, true) => (crate::ui::style::GOOD, "you are in"),
                (true, false) => (crate::ui::style::BAD, "not joined"),
            };
            widget::tooltip(
                text("\u{25CF}").size(15).style(theme::Text::Color(colour)),
                text(t(hint)).size(11),
                widget::tooltip::Position::Right,
            )
            .gap(4)
            .style(theme::Container::Box)
            .into()
        };

    let battle_line =
        |label: &str,
         battle: &Option<sf_api::gamestate::guild::PlanedBattle>,
         joined: bool| {
            match battle {
                Some(b) => format!(
                    "{label}: {} {} - {}",
                    if b.is_raid() { "raid" } else { "battle" },
                    b.date.format("%d.%m. %H:%M"),
                    if joined { "you are in" } else { "NOT joined" }
                ),
                None => format!("{label}: none planned"),
            }
        };

    // Switching the sign-up off is asked back once - a forgotten raid hurts
    let confirm_off: Element<Message> = match player.social.confirm_off {
        Some(defense) => container(
            column!(
                text(if defense {
                    t("Really switch off auto-join for defenses? You may miss a defense.")
                } else {
                    t("Really switch off auto-join for attacks? You may miss a raid or attack.")
                })
                .size(12),
                row!(
                    button(text(t("Yes, switch off")).size(12))
                        .padding(4)
                        .style(crate::ui::style::danger())
                        .on_press(Message::GuildSetAuto {
                            ident,
                            defense,
                            state: false
                        }),
                    button(text(t("No, keep it on")).size(12))
                        .padding(4)
                        .on_press(Message::GuildAutoCancelOff { ident }),
                )
                .spacing(8),
            )
            .spacing(4),
        )
        .padding(6)
        .style(theme::Container::Box)
        .into(),
        None => column!().into(),
    };

    // ------------------------------------------------ odds
    let own_power = crate::guild_fight::own_power(guild);
    let history_key = GuildHistory::key(server_url, guild);
    // The chance of winning the battle that is planned - for a raid, the
    // app's own estimate of the next one, learned from the raids it has
    // watched (the game does not show raid monsters).
    //
    // It used to be one grey line of 11 px type, percentage and reasoning
    // alike, which is how the one number anybody reads here disappeared
    // into its own footnote. The number is now big and coloured by what
    // it says; the reasoning stays small beside it
    let odds_line = |battle: &Option<
        sf_api::gamestate::guild::PlanedBattle,
    >|
     -> Option<(String, Option<f32>, String)> {
        let b = battle.as_ref()?;
        if b.is_raid() {
            let number = guild.finished_raids + 1;
            let est = history.raid_estimate(&history_key, number, own_power);
            let label = format!("{} {number}", t("Raid"));
            // Two figures, not one: the whole guild is the ceiling, the
            // members signed up are what actually goes in. The one figure
            // it used to show was the ceiling, unlabelled - "43 %" with
            // ten of forty-nine not signed up was a number for a raid
            // that was not going to happen
            let (signed_power, signed_up) =
                crate::guild_fight::signed_power(guild);
            let members = guild.members.len();
            Some(match est.chance {
                Some(c) => {
                    let as_signed = est
                        .needed
                        .map(|n| crate::guild_fight::win_chance(signed_power, n))
                        .unwrap_or(0.0);
                    let basis = format!(
                        "{} {members} {} \u{00B7} {} {signed_up}/{members} \u{2192} ~{:.0} % \u{00B7} {}",
                        t("if all"),
                        t("fight"),
                        t("signed up"),
                        as_signed * 100.0,
                        est.basis
                    );
                    (label, Some(c as f32 * 100.0), basis)
                }
                None => (
                    label,
                    None,
                    format!(
                        "{} - {}",
                        t("no estimate yet"),
                        t(
                            "the game hides raid monsters; the app learns from every raid it watches"
                        )
                    ),
                ),
            })
        } else {
            let enemy = player.social.enemies.iter().find(|e| e.id == b.other);
            Some(match enemy {
                Some(e) => (
                    format!("vs {}", e.name),
                    Some(
                        crate::guild_fight::win_chance(own_power, e.power)
                            as f32
                            * 100.0,
                    ),
                    e.desc.clone(),
                ),
                None => (
                    String::new(),
                    None,
                    t("looking up the other guild...").to_string(),
                ),
            })
        }
    };
    let odds_text =
        |line: Option<(String, Option<f32>, String)>| -> Element<Message> {
            let Some((label, pct, basis)) = line else {
                return column!().into();
            };
            // The whole line as one piece of text - clicking it puts that
            // on the clipboard, which is what it is for: the odds get
            // pasted into the guild chat before a raid
            let copy = match pct {
                Some(p) if !label.is_empty() => {
                    format!("{label}: ~{p:.0} % ({basis})")
                }
                Some(p) => format!("~{p:.0} % ({basis})"),
                None if !label.is_empty() => format!("{label}: {basis}"),
                None => basis.clone(),
            };
            let mut r = row!().spacing(6).align_items(Alignment::Center);
            if !label.is_empty() {
                r = r.push(
                    text(format!("{label}:"))
                        .size(12)
                        .style(theme::Text::Color(crate::ui::style::MUTED)),
                );
            }
            if let Some(p) = pct {
                r = r.push(
                    text(format!("{p:.0} %"))
                        .size(16)
                        .font(iced::Font {
                            weight: iced::font::Weight::Bold,
                            ..iced::Font::DEFAULT
                        })
                        .style(theme::Text::Color(pct_color(p))),
                );
            }
            let r = r.push(
                text(basis)
                    .size(11)
                    .style(theme::Text::Color(crate::ui::style::MUTED)),
            );
            row!(
                horizontal_space().width(Length::Fixed(12.0)),
                widget::tooltip(
                    button(r)
                        .padding([1, 4])
                        .style(crate::ui::style::quiet())
                        .on_press(Message::GuildChatCopy { ident, text: copy }),
                    text(t("Click to copy")).size(11),
                    widget::tooltip::Position::Right,
                )
                .gap(4)
                .style(theme::Container::Box),
            )
            .align_items(Alignment::Center)
            .into()
        };

    let compare = row!(
        text(format!(
            "{}: {:.0}k ({})",
            t("Our battle strength"),
            own_power / 1000.0,
            crate::guild_fight::describe(guild)
        ))
        .size(11),
        horizontal_space(),
        text_input(t("Compare with guild..."), &player.social.compare_input).font(crate::ui::style::FAT)
            .on_input(move |text| Message::GuildCompareInput { ident, text })
            .on_submit(Message::GuildCompare { ident })
            .size(11)
            .padding(3)
            .width(Length::Fixed(160.0)),
        button(text(t("Odds")).size(11))
            .padding(3)
            .on_press(Message::GuildCompare { ident }),
    )
    .spacing(6)
    .align_items(Alignment::Center);
    let compare_result: Element<Message> = match &player.social.compare_result {
        Some(r) => text(r).size(11).into(),
        None => column!().into(),
    };

    let battles = column!(
        row!(
            battle_mark(&guild.attacking, in_attack),
            text(battle_line(t("Attack"), &guild.attacking, in_attack))
                .size(12)
                .style(theme::Text::Color(if in_attack {
                    crate::ui::style::GOOD
                } else if guild.attacking.is_some() {
                    crate::ui::style::BAD
                } else {
                    crate::ui::style::MUTED
                })),
            horizontal_space(),
            checkbox(t("Auto-join attacks"), player.social.auto_attack)
.style(crate::ui::style::check())
                .on_toggle(move |state| Message::GuildSetAuto {
                    ident,
                    defense: false,
                    state
                })
                .size(16)
                .text_size(12),
        )
        .spacing(8)
        .align_items(Alignment::Center),
        odds_text(odds_line(&guild.attacking)),
        // The raid just fought, with the odds it went in on. This line is
        // the one that stayed after the raid was over: the estimate above
        // is for the next raid and goes blank the moment none is planned,
        // and "what were our odds on the one we lost" was nowhere
        last_raid_line(history.last_raid(&history_key).as_ref()),
        {
            // Only a leader or an officer may declare a raid - hidden with
            // the rest of the leadership tools (6.1.2)
            let raid: Element<Message> = if tools {
                checkbox(
                    t("Start a raid whenever none is running (10 000 gold, leader or officer)"),
                    player.social.raid_auto,
                )
                .style(crate::ui::style::check())
                .on_toggle(move |state| Message::GuildSetRaidAuto { ident, state })
                .size(16)
                .text_size(12)
                .into()
            } else {
                column!().into()
            };
            raid
        },
        row!(
            battle_mark(&guild.defending, in_defense),
            text(battle_line(t("Defense"), &guild.defending, in_defense))
                .size(12)
                .style(theme::Text::Color(if in_defense {
                    crate::ui::style::GOOD
                } else if guild.defending.is_some() {
                    crate::ui::style::BAD
                } else {
                    crate::ui::style::MUTED
                })),
            horizontal_space(),
            checkbox(t("Auto-join defenses"), player.social.auto_defense)
.style(crate::ui::style::check())
                .on_toggle(move |state| Message::GuildSetAuto {
                    ident,
                    defense: true,
                    state
                })
                .size(16)
                .text_size(12),
        )
        .spacing(8)
        .align_items(Alignment::Center),
        odds_text(odds_line(&guild.defending)),
        confirm_off,
        compare,
        compare_result,
        text(format!(
            "Raids done: {} - next guild attack possible: {}",
            guild.finished_raids,
            guild
                .next_attack_possible
                .map(|t| t.format("%d.%m. %H:%M").to_string())
                .unwrap_or_else(|| "now".to_string())
        ))
        .size(11),
    )
    .spacing(4);

    let mut chat = column!().spacing(4).padding(5);

    // The older lines, read back out of the archive file - "kopirovat
    // stary veci z chatu, treba ten spam level upu". Above the live chat,
    // in the same scroll, each a button that copies it. What the game
    // still shows is left out of this part so nothing is listed twice
    if player.social.show_archive {
        let live: std::collections::HashSet<String> = guild
            .chat
            .iter()
            .map(|m| {
                let body = crate::chat::decode(&m.message)
                    .unwrap_or_else(|| m.message.clone());
                if m.user.is_empty() {
                    body
                } else {
                    format!("{}: {body}", m.user)
                }
            })
            .collect();
        let older: Vec<&String> = player
            .social
            .archive_lines
            .iter()
            .filter(|l| {
                !(player.social.archive_people_only
                    && crate::archive::is_games_own(l))
            })
            .filter(|l| {
                let tail = l.find("] ").map_or(l.as_str(), |i| &l[i + 2..]);
                !live.contains(tail)
            })
            .collect();
        chat = chat.push(
            row!(
                text(format!(
                    "{} ({})",
                    t("Older lines from the archive"),
                    older.len()
                ))
                .size(12),
                horizontal_space(),
                checkbox(t("people only"), player.social.archive_people_only)
                    .style(crate::ui::style::check())
                    .on_toggle(move |state| Message::GuildArchivePeopleOnly {
                        ident,
                        state
                    })
                    .size(16)
                    .text_size(12),
            )
            .spacing(8)
            .align_items(Alignment::Center),
        );
        if player.social.archive_lines.is_empty() {
            chat = chat.push(
                text(if archive.0 {
                    t("Nothing archived yet")
                } else {
                    t("Switch the archive on below - the file is what the \
                       older lines are read from")
                })
                .size(12)
                .style(theme::Text::Color(crate::ui::style::MUTED)),
            );
        }
        for l in older {
            let games_own = crate::archive::is_games_own(l);
            chat = chat.push(
                button(
                    text(l).size(12).width(Length::Fill).style(
                        theme::Text::Color(if games_own {
                            crate::ui::style::MUTED
                        } else {
                            Color::WHITE
                        }),
                    ),
                )
                .padding(2)
                .width(Length::Fill)
                .style(crate::ui::style::quiet())
                .on_press(Message::GuildChatCopy {
                    ident,
                    text: l.clone(),
                }),
            );
        }
        chat = chat.push(horizontal_rule(1));
    }

    if guild.chat.is_empty() {
        chat = chat.push(
            text(t(
                "No messages loaded yet. The chat refreshes itself every 15 \
                 seconds while this tab is open.",
            ))
            .size(12),
        );
    }
    // The server lists the newest message first - a chat reads the other way.
    // Every line is a button: this toolkit has no selectable text, so
    // clicking the line is how a message gets copied out
    let mut whole_chat = String::new();
    for msg in guild.chat.iter().rev() {
        // Two kinds of line. Somebody talking has a time and a name; the
        // game talking - a level up, a hit on the portal, a cleared
        // floor, a kick, a promotion - has neither, and used to be thrown
        // away by the parser before it ever got here. Those are the lines
        // that say what the guild has been doing, so they are shown, in
        // the quiet grey of something nobody typed
        let system = msg.user.is_empty();
        let stamp = msg
            .time
            .map(|t| t.format("%H:%M").to_string())
            .unwrap_or_default();
        // The game does not send the sentence, only a code and the parts
        // ("#ra#Petr#2#Mentol"): the game client writes the sentence from
        // its own language file, and we have to write our own
        let body = crate::chat::decode(&msg.message)
            .unwrap_or_else(|| msg.message.clone());
        let line = if system {
            body.clone()
        } else {
            format!("{stamp} {}: {body}", msg.user)
        };
        whole_chat.push_str(&line);
        whole_chat.push('\n');
        chat = chat.push(
            button(
                row!(
                    text(stamp).size(11).width(Length::Fixed(40.0)),
                    // The name as the game spells it, not titlecased
                    text(&msg.user).size(12).width(Length::Fixed(130.0)),
                    text(&body).size(12).width(Length::Fill).style(
                        theme::Text::Color(if system {
                            crate::ui::style::MUTED
                        } else {
                            Color::WHITE
                        }),
                    ),
                )
                .spacing(8)
                .align_items(Alignment::Start),
            )
            .padding(2)
            .width(Length::Fill)
            .style(crate::ui::style::quiet())
            .on_press(Message::GuildChatCopy { ident, text: line }),
        );
    }

    let input =
        text_input(t("Message to the guild..."), &player.social.chat_input).font(crate::ui::style::FAT)
            .on_input(move |text| Message::GuildChatInput { ident, text })
            .on_submit(Message::GuildChatSend { ident })
            .size(13)
            .padding(6);

    let send = button(text(t("Send")).size(13))
        .padding(6)
        .on_press(Message::GuildChatSend { ident });

    let copy_all = button(text(t("Copy chat")).size(13))
        .padding(6)
        .style(crate::ui::style::quiet())
        .on_press(Message::GuildChatCopy {
            ident,
            text: whole_chat,
        });

    // The older lines out of the archive file, shown above the live chat
    let older = button(
        text(if player.social.show_archive {
            t("Hide older")
        } else {
            t("Older lines")
        })
        .size(13),
    )
    .padding(6)
    .style(if player.social.show_archive {
        crate::ui::style::primary()
    } else {
        crate::ui::style::quiet()
    })
    .on_press(Message::GuildArchiveToggle { ident });

    // The archive switch, with where the file is. The game keeps a day
    // or two of chat; this keeps all of it, on the desktop, as it happens
    let (archive_on, archive_note) = archive;
    let archive_row = row!(
        checkbox(t("Archive the chat to a text file"), archive_on)
            .style(crate::ui::style::check())
            .on_toggle(Message::SetChatArchive)
            .size(16)
            .text_size(12),
        text(match (archive_on, archive_note) {
            (false, _) => String::new(),
            (true, None) => t("written on the next refresh").to_string(),
            (true, Some(n)) => n.to_string(),
        })
        .size(11)
        .style(theme::Text::Color(crate::ui::style::MUTED)),
    )
    .spacing(10)
    .align_items(Alignment::Center);

    let mut bottom = column!(
        row!(input, send, copy_all, older)
            .spacing(8)
            .align_items(Alignment::Center),
        text(t("Click any message to copy it")).size(11),
        archive_row,
    )
    .spacing(4);

    if let Some(status) = &player.social.status {
        bottom = bottom.push(text(status).size(11));
    }

    // Recruiting: guildless players from the server crawl whose level is
    // near the guild's average, each with an invite button. Under the chat,
    // where there is room - the member table is full to the last pixel
    if !recruits.is_empty() {
        let mut list = column!(
            text(t(
                "Recruits from the Hall of Fame - no guild, level near ours"
            ))
            .size(12),
        )
        .spacing(2);
        for r in &recruits {
            let name = r.name.clone();
            let class =
                r.class.map(|c| format!(" · {c:?}")).unwrap_or_default();
            list = list.push(
                row!(
                    text(format!("{} (LVL {}{class})", r.name, r.level))
                        .size(11),
                    horizontal_space(),
                    button(text(t("Invite")).size(11))
                        .padding([2, 8])
                        .on_press(Message::GuildInvite { ident, name }),
                )
                .spacing(8)
                .align_items(Alignment::Center),
            );
        }
        bottom = bottom.push(horizontal_rule(1));
        bottom = bottom.push(list);
    }

    let chat = scrollable(chat).height(Length::Fill).direction(
        scrollable::Direction::Vertical(
            scrollable::Properties::new().alignment(scrollable::Alignment::End),
        ),
    );

    // 6.1: the best guilds to attack, from the game's own list
    let targets: Element<Message> = {
        let own_power = crate::guild_fight::own_power(guild);
        let may_declare = guild
            .members
            .iter()
            .find(|m| m.name.eq_ignore_ascii_case(&player.name))
            .is_some_and(|m| {
                matches!(
                    m.guild_rank,
                    sf_api::gamestate::guild::GuildRank::Leader
                        | sf_api::gamestate::guild::GuildRank::Officer
                )
            });
        let list = &guild.fightable_guilds;
        let mut col = column!(row!(
            text(t("Attack targets")).size(14),
            horizontal_space(),
            button(text(t("Find targets")).size(12))
                .padding([3, 8])
                .style(crate::ui::style::quiet())
                .on_press(Message::GuildFindTargets { ident }),
        )
        .align_items(Alignment::Center))
        .spacing(4);
        if list.is_empty() {
            col = col.push(
                text(t("Press 'Find targets' - the game lists the guilds that can be attacked now."))
                    .size(12)
                    .style(theme::Text::Color(crate::ui::style::MUTED)),
            );
        }
        for s in crate::guild_fight::suggest_targets(own_power, list) {
            let g = &list[s.index];
            let pct = (s.chance * 100.0).round();
            let colour = if s.chance >= 0.6 {
                crate::ui::style::GOOD
            } else if s.chance >= 0.45 {
                crate::ui::style::WARN
            } else {
                crate::ui::style::BAD
            };
            let mut r = row!(
                text(format!("{pct:.0} %"))
                    .size(14)
                    .width(Length::Fixed(56.0))
                    .style(theme::Text::Color(colour)),
                text(g.name.clone()).size(13).width(Length::FillPortion(3)),
                text(format!(
                    "#{} · {} {} · {} {}-{} (Ø {}) · {} {}",
                    g.rank,
                    g.number_of_members,
                    t("members"),
                    t("lvl"),
                    g.members_min_level,
                    g.members_max_level,
                    g.members_average_level,
                    g.honor,
                    t("honor"),
                ))
                .size(12)
                .width(Length::FillPortion(5))
                .style(theme::Text::Color(crate::ui::style::MUTED)),
            )
            .spacing(8)
            .align_items(Alignment::Center);
            if may_declare {
                r = r.push(
                    button(text(t("Attack")).size(12))
                        .padding([3, 8])
                        .style(crate::ui::style::primary())
                        .on_press(Message::GuildDeclareAttack {
                            ident,
                            guild: g.name.clone(),
                        }),
                );
            }
            col = col.push(r);
        }
        if !list.is_empty() {
            col = col.push(
                text(t("Estimate from member count and levels; their instructor bonus is not in the list."))
                    .size(11)
                    .style(theme::Text::Color(crate::ui::style::MUTED)),
            );
        }
        col.into()
    };
    let drawer_body: Element<Message> = if drawer_open {
        column!(
            battles,
            horizontal_rule(1),
            targets,
            horizontal_rule(1),
            member_summary(
                guild, history, &history_key, strictness, officer_seats, ident,
                tools,
            ),
            horizontal_rule(1),
        )
        .spacing(6)
        .into()
    } else {
        column!().into()
    };

    if show_members {
        let (inline_drawer, overlay): (Element<Message>, Option<Element<Message>>) =
            if two_columns && drawer_open {
                (column!().into(), Some(container(column!(
                    row!(text(t("Summary")).size(22), horizontal_space(),
                        button(text(t("Close"))).padding([6, 12]).style(crate::ui::style::quiet())
                            .on_press(Message::GuildDrawer { ident })).align_items(Alignment::Center),
                    scrollable(drawer_body).height(Length::Fill),
                ).spacing(16)).padding(20).width(950).height((window_h - 150.0).clamp(400.0, 700.0))
                    .style(crate::ui::style::card()).into()))
            } else { (drawer_body, None) };
        let roster = column!(
            head,
            inline_drawer,
            view_members(
                player,
                window_w,
                window_h,
                guild,
                history,
                &history_key,
                strictness,
                officer_seats, marks.0, marks.1,
                two_columns,
                tools,
            )
        )
        .spacing(6)
        .padding([8, 12])
        .height(Length::Fill)
        .width(Length::Fill);
        return iced_aw::Modal::new(roster, overlay).on_esc(Message::GuildDrawer { ident }).into();
    }

    column!(
        head,
        drawer_body,
        horizontal_rule(1),
        chat,
        horizontal_rule(1),
        bottom
    )
    .spacing(8)
    .padding(15)
    .height(Length::Fill)
    .width(Length::Fill)
    .into()
}

/// "Raid 52: ~64 % - lost, 2 h ago" - the last raid and what it was given.
/// Nothing at all until a raid has been watched
fn last_raid_line<'a>(
    last: Option<&crate::guild_history::RaidRecord>,
) -> Element<'a, Message> {
    let Some(r) = last else {
        return column!().into();
    };
    let (result, colour) = if r.won {
        (t("won"), crate::ui::style::GOOD)
    } else {
        (t("lost"), crate::ui::style::BAD)
    };
    let odds = match (r.chance, r.signed_chance, r.signed_up) {
        (Some(c), Some(sc), Some(n)) => format!(
            "~{:.0} % ({} {n} {} ~{:.0} %)",
            c * 100.0,
            t("signed up"),
            t("\u{2192}"),
            sc * 100.0
        ),
        (Some(c), _, _) => format!("~{:.0} %", c * 100.0),
        _ => t("no estimate").to_string(),
    };
    let ago = {
        let mins = (chrono::Local::now() - r.when).num_minutes().max(0);
        if mins < 60 {
            format!("{mins} min")
        } else if mins < 48 * 60 {
            format!("{} h", mins / 60)
        } else {
            format!("{} d", mins / (24 * 60))
        }
    };
    row!(
        text(format!("{} {}:", t("Last raid"), r.number))
            .size(12)
            .style(theme::Text::Color(crate::ui::style::MUTED)),
        text(odds).size(13).style(theme::Text::Color(colour)),
        text(format!(
            "\u{00B7} {result}, {}",
            t("{} ago").replace("{}", &ago)
        ))
            .size(12)
            .style(theme::Text::Color(crate::ui::style::MUTED)),
    )
    .spacing(6)
    .align_items(Alignment::Center)
    .into()
}

/// The inbox
pub fn view_mail(player: &AccountInfo) -> Element<'_, Message> {
    let lock = player.status.lock_safe();

    let gs = match &*lock {
        AccountStatus::Idle(_, gs) | AccountStatus::Busy(gs, _) => gs,
        AccountStatus::FatalError(err) => {
            return text(format!("Error: {err}")).size(20).into();
        }
        _ => return text(t("Logging in")).size(20).into(),
    };

    let ident = player.ident;
    let mail = &gs.mail;

    let head = row!(
        text(t("Mail")).size(18),
        text(format!(
            "{}/{} messages",
            mail.inbox.len(),
            mail.inbox_capacity
        ))
        .size(14),
        horizontal_space(),
        button(text(t("Refresh")).size(15))
            .padding(4)
            .on_press(Message::MailRefresh { ident }),
    )
    .spacing(12)
    .align_items(Alignment::Center);

    let mut list = column!().spacing(4).padding(5);
    if mail.inbox.is_empty() {
        list = list.push(text(t("The inbox is empty")).size(14));
    }

    for (pos, entry) in mail.inbox.iter().enumerate() {
        let selected = player.social.open_msg == Some(pos as i32);
        let title = if entry.title.trim().is_empty() {
            format!("{:?}", entry.msg_typ)
        } else {
            entry.title.clone()
        };

        list = list.push(
            button(
                row!(
                    text(if entry.read { " " } else { "*" })
                        .size(14)
                        .width(Length::Fixed(10.0)),
                    text(titlecase::titlecase(&entry.from))
                        .size(14)
                        .width(Length::Fixed(140.0)),
                    text(title).size(14).width(Length::Fill),
                    text(entry.date.format("%d.%m. %H:%M").to_string())
                        .size(11)
                        .width(Length::Fixed(90.0))
                        .horizontal_alignment(Horizontal::Right),
                )
                .spacing(8)
                .align_items(Alignment::Center),
            )
            .padding(4)
            .width(Length::Fill)
            .style(if selected {
                crate::ui::style::primary()
            } else {
                crate::ui::style::quiet()
            })
            .on_press(Message::MailOpen {
                ident,
                pos: pos as i32,
                by_id: true,
            }),
        );
    }

    let mut open = column!().spacing(6);
    match (&player.social.open_msg, &player.social.open_text) {
        (Some(pos), Some(body)) => {
            let header = mail
                .inbox
                .get(*pos as usize)
                .map(|e| {
                    format!(
                        "{} - {} ({})",
                        titlecase::titlecase(&e.from),
                        if e.title.trim().is_empty() {
                            format!("{:?}", e.msg_typ)
                        } else {
                            e.title.clone()
                        },
                        e.date.format("%d.%m.%Y %H:%M")
                    )
                })
                .unwrap_or_else(|| "Message".to_string());
            open = open.push(text(header).size(14));
            open = open.push(
                scrollable(text(body).size(14).width(Length::Fill))
                    .height(Length::Fill),
            );
        }
        (Some(_), None) => {
            open = open.push(text(t("Loading message...")).size(14));
        }
        _ => {
            open = open.push(text(t("Click a message to read it")).size(14));
        }
    }

    // ------------------------------------------------------------ reply
    let to_input = text_input(t("Recipient"), &player.social.reply_to).font(crate::ui::style::FAT)
        .on_input(move |text| Message::MailReplyToInput { ident, text })
        .size(15)
        .padding(6)
        .width(Length::Fixed(180.0));
    let msg_input =
        text_input(t("Write a message..."), &player.social.reply_text).font(crate::ui::style::FAT)
            .on_input(move |text| Message::MailReplyInput { ident, text })
            .on_submit(Message::MailSend { ident })
            .size(15)
            .padding(6);
    let send = button(text(t("Send")).size(15))
        .padding(6)
        .on_press(Message::MailSend { ident });

    let mut reply = column!(
        text(t("Reply / new message")).size(14),
        row!(to_input, msg_input, send)
            .spacing(8)
            .align_items(Alignment::Center),
    )
    .spacing(4);

    if let Some(status) = &player.social.status {
        reply = reply.push(text(status).size(11));
    }

    column!(
        head,
        horizontal_rule(1),
        scrollable(list).height(Length::FillPortion(1)),
        horizontal_rule(1),
        column!(open).height(Length::FillPortion(1)),
        horizontal_rule(1),
        reply
    )
    .spacing(8)
    .padding(15)
    .height(Length::Fill)
    .width(Length::Fill)
    .into()
}

#[cfg(test)]
mod tests {
    use super::{BONUS_RAIDS_MAX, GUILD_SKILL_MAX, bonus_progress};

    #[test]
    fn both_halves_are_worth_the_same() {
        // Nothing done at all
        assert_eq!(bonus_progress(0, 0), 0.0);
        // Members maxed, no raids - half way
        assert_eq!(bonus_progress(GUILD_SKILL_MAX, 0), 50.0);
        // Raids done, no levels - the other half
        assert_eq!(bonus_progress(0, BONUS_RAIDS_MAX), 50.0);
        // Both done
        assert_eq!(bonus_progress(GUILD_SKILL_MAX, BONUS_RAIDS_MAX), 100.0);
    }

    #[test]
    fn raids_past_the_fiftieth_add_nothing() {
        assert_eq!(
            bonus_progress(GUILD_SKILL_MAX, 200),
            bonus_progress(GUILD_SKILL_MAX, BONUS_RAIDS_MAX)
        );
    }
}

#[cfg(test)]
mod date_tests {
    use super::parse_date;
    use chrono::NaiveDate;

    /// However the leader types it. Two digits mean this century - a guild
    /// founded in the year 26 is not something the app has to consider
    #[test]
    fn a_date_is_read_the_way_a_person_writes_one() {
        let d = NaiveDate::from_ymd_opt(2026, 8, 21).unwrap();
        for raw in [
            "21.8.2026", "21. 8. 2026", "21/8/2026", "2026-08-21",
            "  21.08.26  ", "21.8.26",
        ] {
            assert_eq!(parse_date(raw), Some(d), "{raw}");
        }
    }

    /// An empty box and a half-typed one are both "no date", not a wrong
    /// one - the field is edited a character at a time
    #[test]
    fn nonsense_is_no_date_rather_than_the_wrong_one() {
        for raw in ["", "   ", "21", "21.8", "31.2.2026", "tomorrow", "0.0.0"] {
            assert_eq!(parse_date(raw), None, "{raw}");
        }
    }
}

#[cfg(test)]
mod width_budget {
    #[test]
    #[ignore = "prints where a row's width goes"]
    fn where_does_the_width_go() {
        let m = |s: &str| crate::ui::glyphs::measure(s, 1.0);
        let lanes: [(&str, f32); 11] = [
            ("grade", m("5").max(m("Zn."))),
            ("advice", m("VYHODIT").max(m("Rada"))),
            ("name", m("Pepa z depa")),
            ("level", m("260").max(m("Level"))),
            ("+7d", m("+999").max(m("+7d"))),
            ("battles", m("999 / 99").max(m("Bit."))),
            ("activity", m("100%").max(m("Aktivita")) * 1.6),
            ("skills", m("99 / 99 / 99").max(m("Tr/In/HoK"))),
            ("in guild", m("999+ d").max(m("In guild"))),
            ("portal", m("9").max(m("Por."))),
            ("online", m("999+ d").max(m("Online"))),
        ];
        let units: f32 = lanes.iter().map(|(_, u)| u).sum();
        let chrome = 6.0 * 10.0 + 4.0 * 9.0 + 14.0;
        let half = (1920.0 - 210.0 - 340.0 - 64.0 - 18.0) / 2.0;
        let canvas = (half - chrome) / units;
        println!("half {half:.0}px, chrome {chrome:.0}, units {units:.2}");
        println!("canvas {canvas:.1}px -> font size {:.1}", canvas / 1.42);
        for (name, u) in lanes {
            println!(
                "  {name:9} {u:6.2} units = {:5.1} px  ({:4.1}% of the row)",
                u * canvas,
                u / units * 100.0
            );
        }
    }
}

/// Comparable member performance, kept on screen for the entire guild.
fn roster_overview<'a>(
    player: &'a AccountInfo,
    stats: &[crate::guild_report::MemberStats<'_>],
    window_w: f32,
    window_h: f32,
) -> Element<'a, Message> {
    use crate::ui::style;
    let ident = player.ident;
    let extended = window_w >= 1750.0;
    let narrow = window_w < 1360.0;
    let level_w = if narrow { 28 } else { 30 };
    let growth_w = if narrow { 32 } else { 36 };
    let activity_w = if narrow { 48 } else { 52 };
    let skills_w = if narrow { 72 } else { 86 };
    let contribution_w = if narrow { 48 } else { 56 };
    let online_w = if narrow { 42 } else { 48 };
    let controls = row!(
        text(format!("{} · {}", t("Entire guild"), stats.len())).size(14),
        horizontal_space(),
        button(text(t("Inactive first")).size(12)).padding([5, 10])
            .style(style::brass(player.social.member_sort == MemberSort::LastOnline))
            .on_press(Message::GuildMemberSort { ident, sort: MemberSort::LastOnline }),
        button(text(t("At risk first")).size(12)).padding([5, 10])
            .style(style::brass(player.social.member_sort == MemberSort::Advice))
            .on_press(Message::GuildMemberSort { ident, sort: MemberSort::Advice }),
        pick_list(MemberSort::ALL, Some(player.social.member_sort), move |sort| Message::GuildMemberSort { ident, sort })
            .text_size(12).padding(4),
        button(text(t("Detailed table")).size(12)).padding([5, 10])
            .style(style::quiet()).on_press(Message::GuildTwoColumns(false)),
    ).spacing(8).align_items(Alignment::Center);
    let head = || -> Element<'a, Message> {
        let mut cells = row!(
            text(t("Name")).size(11).width(Length::Fill),
            text("Lv").size(11).width(level_w),
            text("+7d").size(11).width(growth_w),
            text(t("Activity")).size(11).width(activity_w),
        );
        if extended {
            cells = cells.push(text(t("Atk")).size(11).width(38))
                .push(text(t("Def")).size(11).width(38))
                .push(text(t("Portal")).size(11).width(42));
        }
        cells = cells.push(text(t("Tr/In/HoK")).size(11).width(skills_w))
            .push(text(t("Contribution")).size(11).width(contribution_w))
            .push(text(t("Online")).size(11).width(online_w))
            .push(text(t("Advice")).size(11).width(64));
        container(cells.spacing(4)).padding([6, 6]).style(style::head()).into()
    };
    let share = |pair: (usize, usize)| if pair.1 == 0 { "—".to_string() } else { format!("{}/{}", pair.0, pair.1) };
    let split = stats.len().div_ceil(2);
    let make_half = |start: usize, end: usize| -> Element<'a, Message> {
        let mut lines = column!().height(Length::Fill).width(Length::Fill);
        for (index, stat) in stats.iter().enumerate().take(end).skip(start) {
            let member = stat.member;
            let chosen = player.social.selected_member.as_deref() == Some(member.name.as_str());
            let name_color = match member.guild_rank {
                sf_api::gamestate::guild::GuildRank::Leader => style::NAME_GOLD,
                sf_api::gamestate::guild::GuildRank::Officer => style::ACCENT,
                _ => style::GOLD,
            };
            let name_limit = if window_w < 1360.0 { 14 } else { 18 };
            let short: String = member.name.chars().take(name_limit).collect();
            let short = if member.name.chars().count() > name_limit { format!("{short}…") } else { short };
            let (advice, tone) = match stat.verdict {
                Verdict::Kick => (t("KICK"), style::BAD),
                Verdict::Watch => (t("watch"), style::WARN),
                Verdict::Demote => (t("demote"), style::WARN),
                Verdict::Promote => (t("OFFICER"), style::ACCENT),
                Verdict::Keep => (t("keep"), style::GOOD),
            };
            let mut cells = row!(
                text(short).size(if window_h < 820.0 { 12 } else { 13 }).style(theme::Text::Color(name_color)).width(Length::Fill),
                text(member.level).size(12).width(level_w),
                text(stat.progress.levels_7d.map_or_else(|| "—".to_string(), |v| format!("+{v}"))).size(12).width(growth_w),
                text(format!("{:.0}%", stat.activity)).size(12).style(theme::Text::Color(pct_color(stat.activity))).width(activity_w),
            );
            if extended {
                cells = cells.push(text(share(stat.progress.attacks)).size(12).width(38))
                    .push(text(share(stat.progress.defenses)).size(12).width(38))
                    .push(text(share(stat.progress.portal)).size(12).width(42));
            }
            cells = cells
                .push(text(format!("{}/{}/{}", member.treasure_skill, member.instructor_skill, member.knights)).size(12).width(skills_w))
                .push(text(if stat.contribution.known_weight > 0.0 { format!("{:.0}%", stat.contribution.total) } else { "—".to_string() })
                    .size(12).style(theme::Text::Color(pct_color(stat.contribution.total))).width(contribution_w))
                .push(text(online_text(stat.online_hours)).size(12).style(theme::Text::Color(
                    if stat.online_hours.is_some_and(|h| h >= 24 * 7) { style::BAD } else { style::MUTED })).width(online_w))
                .push(text(advice).size(11).style(theme::Text::Color(tone)).width(64));
            lines = lines.push(container(
                button(container(cells.spacing(4).align_items(Alignment::Center)).width(Length::Fill).height(Length::Fill).center_y()).padding([1, 6]).width(Length::Fill).height(Length::Fill)
                    .style(style::roster_button(chosen, index % 2 == 1))
                    .on_press(Message::GuildMemberDetail { ident, name: Some(member.name.clone()) }),
            ).height(Length::FillPortion(1)).max_height(32));
        }
        for _ in (end - start)..split {
            lines = lines.push(widget::Space::new(Length::Fill, Length::FillPortion(1)));
        }
        column!(head(), lines).spacing(2).width(Length::FillPortion(1)).height(Length::Fill).into()
    };
    let roster = column!(
        controls,
        row!(make_half(0, split), make_half(split, stats.len())).spacing(20)
            .height((window_h - 200.0).max(490.0).min(split as f32 * 32.0 + 28.0)),
        iced::widget::text(t("Tr/In/HoK = treasure / instructor / knights. +7d = level growth. — = not enough history."))
            .size(11).style(theme::Text::Color(style::MUTED)),
    ).spacing(8).height(Length::Fill).width(Length::Fill);
    let detail: Option<Element<'a, Message>> = stats.iter()
        .find(|s| player.social.selected_member.as_deref() == Some(s.member.name.as_str()))
        .map(|s| {
            let member = s.member;
            container(column!(
                text(member.name.clone()).size(24),
                text(format!("{} {} · {} {:.0}%", t("Level"), member.level, t("Activity"), s.activity)).size(14),
                horizontal_rule(1),
                text(format!("{}: {} / {} / {}", t("Tr/In/HoK"), member.treasure_skill, member.instructor_skill, member.knights)).size(14),
                text(format!("{}: {}", t("Online"), online_text(s.online_hours))).size(14),
                text(format!("{}: {}", t("Gr."), s.grade)).size(14),
                text(s.reasons.join(" · ")).size(13),
                button(text(t("Close")).size(13)).padding([8, 18]).style(style::primary())
                    .on_press(Message::GuildMemberDetail { ident, name: None }),
            ).spacing(16)).padding(24).width(520).style(style::card()).into()
        });
    iced_aw::Modal::new(roster, detail)
        .on_esc(Message::GuildMemberDetail { ident, name: None })
        .into()
}
