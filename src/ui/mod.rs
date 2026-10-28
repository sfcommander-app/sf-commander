use std::collections::HashSet;

use self::glyphs::ticked as checkbox;
use crate::i18n::t;
use chrono::{DateTime, Local};
use iced::{
    Alignment, Color, Element, Length,
    alignment::Horizontal,
    theme,
    widget::{
        self, Button, button, column, container, horizontal_space, pick_list,
        progress_bar, row, scrollable,
    },
};
// His lettering stands in for the toolkit's `text`, so every line on
// this screen is set in the face the purse card is
use self::glyphs::lettered as text;
use iced_aw::widgets::DropDown;
use num_format::ToFormattedString;
use options::view_options;

use self::{
    dungeons::view_dungeons,
    fortress::view_fortress,
    guard::view_guard,
    idle::view_arena_manager,
    pets::view_pets,
    scrapbook::view_scrapbook,
    shops::view_shops,
    social::{view_guild, view_mail},
    tavern::view_tavern,
    underworld::view_underworld,
};
use crate::util::LockExt;
use crate::{
    AccountIdent, AccountPage, ActionSelection, Helper, View,
    config::{AvailableTheme, Config},
    crawler::CrawlingOrder,
    get_server_code,
    message::Message,
    player::{AccountInfo, AccountStatus},
    server::{CrawlingStatus, ServerInfo},
    top_bar,
};

mod dungeons;
mod fortress;
pub mod glyphs;
mod guard;
mod idle;
mod options;
mod pets;
mod scrapbook;
mod shops;
mod social;
pub mod style;
mod tavern;
pub use tavern::TavernTab;
pub mod underworld;

impl Helper {
    pub fn view_current_page(&self) -> Element<'_, Message> {
        // An unreadable helper.toml gets its own page, nothing else - the
        // login page would look like a wiped installation
        if let Some(problem) = &self.config_problem {
            return self.view_recovery(problem);
        }
        let view: Element<Message> = match &self.current_view {
            View::Account { ident, page } => self.view_account(*ident, *page),
            View::Login => self
                .login_state
                .view(&self.config.accounts, self.has_accounts()),
            View::Overview { selected, action } => {
                self.view_overview(selected, action)
            }
            View::Settings => self.view_settings(),
        };
        let main_part = container(view).width(Length::Fill).center_x();
        let mut res = column!();

        if self.should_update {
            let dl_button =  button(t("Download")).on_press(
                Message::OpenLink("https://github.com/the-marenga/sf-scrapbook-helper/releases/latest".to_string())
            );

            let ignore_button = button(t("Ignore"))
                .on_press(Message::UpdateResult(false))
                .style(crate::ui::style::danger());

            let update_msg = row!(
                horizontal_space(),
                text(t("A new Version is available!")).size(20),
                dl_button,
                horizontal_space(),
                ignore_button,
            )
            .align_items(Alignment::Center)
            .spacing(10)
            .width(Length::Fill)
            .padding(15);

            res = res.push(update_msg);
        }
        // Something on disk went wrong: the config came from its backup,
        // or a save keeps failing. Said at the top, in red, until it is
        // dealt with - a lost file must never be a silent event
        let disk_notice =
            crate::storage::last_error().or_else(|| self.config_notice.clone());
        if let Some(notice) = disk_notice {
            res = res.push(
                container(
                    text(format!("{}: {notice}", t("Disk")))
                        .size(13)
                        .style(theme::Text::Color(style::BAD)),
                )
                .width(Length::Fill)
                .padding([4, 12]),
            );
        }
        res.push(main_part).into()
    }

    /// `helper.toml` is there but does not read, and neither does its
    /// backup. Nothing is written while this page is up
    fn view_recovery(&self, problem: &str) -> Element<'_, Message> {
        let path = crate::config::Config::path();
        column!(
            text(t("The settings file could not be read")).size(22),
            text(path.display().to_string()).size(12),
            text(problem).size(12).style(theme::Text::Color(style::BAD)),
            text(t(
                "Nothing has been overwritten. The file stays as it is until \
                 you choose: fix it by hand and read it again, or set it \
                 aside (kept as helper.toml.broken-...) and start without \
                 saved accounts."
            ))
            .size(13),
            row!(
                button(text(t("Read it again")).size(13))
                    .padding(6)
                    .on_press(Message::ConfigRetry),
                button(text(t("Open the folder")).size(13))
                    .padding(6)
                    .on_press(Message::ConfigOpenFolder),
                button(
                    text(t("Set it aside and start without saved accounts"))
                        .size(13)
                )
                .padding(6)
                .style(style::danger())
                .on_press(Message::ConfigStartFresh),
            )
            .spacing(10),
        )
        .spacing(12)
        .padding(30)
        .max_width(760)
        .into()
    }

    fn view_account(
        &self,
        ident: AccountIdent,
        page: AccountPage,
    ) -> Element<'_, Message> {
        let Some((server, player)) = self.servers.get_ident(&ident) else {
            return self
                .login_state
                .view(&self.config.accounts, self.has_accounts());
        };

        // Named game-style tabs remain visible at every supported window size.
        let compact = self.window_h < 850.0;

        let unread_mail = match &*player.status.lock_safe() {
            AccountStatus::Idle(_, gs) | AccountStatus::Busy(gs, _) => {
                gs.mail.inbox.iter().filter(|a| !a.read).count()
            }
            _ => 0,
        };
        let mail_label = if unread_mail > 0 {
            format!("{} ({unread_mail})", t("Mail"))
        } else {
            t("Mail").to_string()
        };

        // FULL BOT: one switch for every farming automation. "On" means all
        // the big ones are running
        let full_on = player
            .scrapbook_info
            .as_ref()
            .is_some_and(|a| a.auto_battle)
            && player.dungeon_info.auto
            && player.expedition_info.auto
            && player.fortress_info.as_ref().is_none_or(|f| f.auto_attack)
            && player.pet_info.auto_feed
            && player.idle_info.auto_upgrade
            && player.guard_info.auto;
        let running = running_jobs(player);
        let full_bot = row!(
            widget::text(format!(
                "{} · {} {}",
                t("Automation"),
                running.len(),
                t("active")
            ))
            .size(12)
            .style(theme::Text::Color(style::MUTED)),
            button(widget::text(t("ON")).size(12))
                .padding([6, 10])
                .style(style::segment(full_on, style::ACCENT))
                .on_press(Message::FullBot {
                    ident: player.ident,
                    on: true
                }),
            button(widget::text(t("OFF")).size(12))
                .padding([6, 10])
                .style(style::segment(running.is_empty(), style::ACCENT))
                .on_press(Message::FullBot {
                    ident: player.ident,
                    on: false
                }),
        )
        .spacing(5)
        .align_items(Alignment::Center);

        // Accounts live in a tab row across the top, so any number of them
        // fits and the sidebar stays for the character itself
        let mut tabs = row!().spacing(4).align_items(Alignment::Center);
        for srv in self.servers.0.values() {
            let mut accs: Vec<&AccountInfo> = srv.accounts.values().collect();
            accs.sort_by(|a, b| a.name.cmp(&b.name));
            for acc in accs {
                let active = acc.ident == player.ident;
                tabs = tabs.push(
                    button(
                        text(format!(
                            "{} ({})",
                            titlecase::titlecase(&acc.name),
                            get_server_code(&srv.ident.url)
                        ))
                        .size(12),
                    )
                    .padding([4, 8])
                    .style(if active {
                        crate::ui::style::primary()
                    } else {
                        crate::ui::style::quiet()
                    })
                    .on_press(Message::ShowPlayer { ident: acc.ident }),
                );
            }
        }
        tabs = tabs.push(
            button(text("+").size(12))
                .padding([4, 8])
                .style(crate::ui::style::quiet())
                .on_press(Message::ViewLogin),
        );
        // Both switches keep a width that does not move as their label
        // flips between STOP and RESUME - a control that changes size when
        // you press it is a control you then have to find again. The width
        // is measured from the wider of the two labels rather than written
        // down, because a hard number is a number that goes stale the day
        // the lettering changes: SLEDOVANI ZAP came out a whole syllable
        // wider than the 104 pixels it had been given
        let switch_w = |a: &str, b: &str| {
            let h = 12.0 * glyphs::INK;
            Length::Fixed(
                glyphs::measure(a, h).max(glyphs::measure(b, h)) + 20.0,
            )
        };
        // Lit, these sit on gold with near-black lettering, so they are
        // set for a coloured plate - see `style::on_plate`
        let switch_face = |label: String, lit: bool| {
            container(style::on_plate(
                label,
                if lit { style::GOLD } else { style::MUTED },
                12.0,
            ))
            .width(Length::Fill)
            .center_x()
        };
        let stop = button(switch_face(
            if self.paused { t("RESUME") } else { t("STOP") }.to_string(),
            self.paused,
        ))
        .padding([4, 8])
        .width(switch_w(t("RESUME"), t("STOP")))
        // Lit while the app is standing still and RESUME is the thing to
        // press; grey while it is working and STOP is merely available.
        // The red is gone from here: when the app is stopped a red band
        // already runs the width of the window saying so, and a second
        // alarm on a button that is not the alarm read as one more colour
        .style(style::lamp(self.paused))
        .on_press(Message::TogglePause);
        // The window follows the bot's jobs - page and character - while
        // this is lit. Next to STOP, because it is the other thing a player
        // reaches for when watching the bot work
        // Green and "ON" while it follows, grey and "OFF" while it does
        // not - the blue accent said nothing about which it was
        let follow = button(switch_face(
            if self.config.follow_bot {
                t("FOLLOW ON")
            } else {
                t("FOLLOW OFF")
            }
            .to_string(),
            self.config.follow_bot,
        ))
        .padding([4, 8])
        .width(switch_w(t("FOLLOW ON"), t("FOLLOW OFF")))
        .style(style::lamp(self.config.follow_bot))
        .on_press(Message::ToggleFollow);

        // The one control over how the app plays, always in sight and
        // wide enough to aim at: eleven stops, OFF at the left and a
        // short visit every half hour at the right. There is no "full
        // power" end any more - the fast end of the old slider sent with
        // no pauses at all, which is the one pattern no player has, so
        // the scale now runs from "does nothing" to "plays as much as the
        // game's own cooldowns allow"
        // The slider shows where the slider is, always. It used to be
        // forced to zero while "do it now" ran, so the handle jumped to
        // OFF and the gold tick moved to 0 whatever the player had set -
        // the scale stopped describing the setting it is the control
        // for. Which of the two is running is what the line above it is
        // for, and the run is the button's business, not the slider's
        // 6.0: one row of four buttons instead of the eleven-stop slider and
        // its "do it now" button, which pulled against each other. While
        // the bot is on it plays continuously; the profile only says how
        // carefully - the request ceiling and nothing else
        let safety = self.config.safety;
        let profile_btn = |label: &'static str, level: u8, on: bool| {
            let colour = if level == 0 {
                style::BAD
            } else {
                style::ACCENT
            };
            button(
                text(t(label))
                    .size(12)
                    .horizontal_alignment(Horizontal::Center),
            )
            .padding([5, 12])
            .style(style::segment(on, colour))
            .on_press(Message::SetSafety(level))
        };
        let current = crate::stealth::Profile::label(safety);
        let safety_box = container(
            row!(
                profile_btn("OFF", 0, safety == 0),
                profile_btn("STEALTH", 3, current == "STEALTH"),
                profile_btn("BALANCED", 6, current == "BALANCED"),
                profile_btn("NO SHAME", 9, current == "NO SHAME"),
            )
            .spacing(2)
            .align_items(Alignment::Center),
        )
        .padding(3)
        .style(style::well());
        // The developer's word of advice, behind a question mark - hover
        // it and it is there, in the app's language
        // 6.1.4: the "?" is also the switch for every explanation in the
        // app - lit while they are shown
        let advice = widget::tooltip(
            button(text("?").size(12))
                .padding([4, 9])
                .style(crate::ui::style::brass(self.config.show_help))
                .on_press(Message::ToggleHelp),
            container(
                column!(
                    text(t(
                        "OFF stops everything. Otherwise the bot plays all the \
                         time, at a human rhythm, and only rests during the night \
                         rest (Settings). STEALTH is the slowest and sends the \
                         fewest requests a minute; NO SHAME halves the pauses \
                         between clicks and sends twice as many. The game's \
                         cooldowns are kept in every mode.",
                    ))
                    .size(12),
                    text(t("Click: show or hide the explanations on every page"))
                        .size(12)
                        .style(theme::Text::Color(style::ACCENT)),
                )
                .spacing(6),
            )
            .padding(8)
            .max_width(340.0)
            .style(theme::Container::Box),
            widget::tooltip::Position::Bottom,
        )
        .gap(6);

        let top = row!(
            stop,
            follow,
            scrollable(tabs)
                .direction(widget::scrollable::Direction::Horizontal(
                    widget::scrollable::Properties::new()
                        .width(3)
                        .scroller_width(3),
                ))
                // The tabs take one share of the free space and the
                // spacer below takes the other, so the slider and its
                // frame sit in the middle of the bar instead of being
                // pushed up against the buttons on the right
                .width(Length::FillPortion(1)),
            safety_box,
            advice,
            horizontal_space().width(Length::FillPortion(1)),
            // The two faces, each written in itself, so the button shows
            // what you get rather than describing it
            widget::tooltip(
                button(if glyphs::artwork() {
                    style::on_plate(t("Aa"), style::GOLD, 13.0)
                } else {
                    text(t("Aa")).size(13).into()
                })
                .padding([4, 9])
                .style(crate::ui::style::brass(glyphs::artwork()))
                .on_press(Message::ToggleDrawnFont),
                container(
                    text(t("The app's own lettering, or the machine's. His \
                         alphabet has capitals only, so a whole screen of it \
                         is not to everybody's taste - this puts the plain \
                         type back.",))
                    .size(12),
                )
                .padding(8)
                .style(style::card()),
                widget::tooltip::Position::Bottom,
            )
            .gap(4),
            button(text(t("Console")).size(12))
                .padding([4, 9])
                .style(crate::ui::style::brass(self.console_open))
                .on_press(Message::ToggleConsole),
            self.update_button(),
            button(text(t("Overview")).size(12))
                .padding([4, 9])
                .style(crate::ui::style::brass(false))
                .on_press(Message::ViewOverview),
            button(text(t("Settings")).size(12))
                .padding([4, 9])
                .style(crate::ui::style::brass(false))
                .on_press(Message::ViewSettings),
            coffee_icon(),
        )
        .spacing(6)
        .padding([6, 10])
        .align_items(Alignment::Center);
        let top = container(
            scrollable(top.width(self.window_w.max(1220.0))).direction(
                widget::scrollable::Direction::Horizontal(
                    widget::scrollable::Properties::new()
                        .width(3)
                        .scroller_width(3),
                ),
            ),
        )
        .width(Length::Fill)
        .style(style::frame());

        // Gold and mushrooms, refreshed with every poll. Two lines, so the
        // mushroom count sits under the gold instead of running off the edge
        let (wallet, shrooms, glasses, lucky, level, xp_done) =
            match &*player.status.lock_safe() {
                AccountStatus::Idle(_, gs) | AccountStatus::Busy(gs, _) => {
                    let c = &gs.character;
                    // `next_level_xp` is the whole bar, `experience` how far
                    // along it we are
                    let frac = if c.next_level_xp > 0 {
                        (c.experience as f64 / c.next_level_xp as f64)
                            .clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    // The gold in full, the way the game writes it - no
                    // "72.4K", and no word "gold" after it, everybody knows
                    (
                        thousands(c.silver / 100),
                        // Mushrooms and hourglasses without the thousands
                        // gap - "7 026" looked like two numbers to the
                        // guild leader; the gold keeps its groups
                        c.mushrooms.to_string(),
                        gs.tavern.quicksand_glasses.to_string(),
                        gs.specials.wheel.lucky_coins.to_string(),
                        Some(c.level),
                        frac,
                    )
                }
                _ => (
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    None,
                    0.0,
                ),
            };

        let resource = |icon: widget::image::Handle, value: String, label: &'static str| -> Element<Message> {
            widget::tooltip(
                row!(widget::image(icon).width(22).height(22),
                    widget::text(value).size(14).style(theme::Text::Color(style::GOLD)))
                    .spacing(7).align_items(Alignment::Center),
                container(widget::text(t(label)).size(12)).padding(8).style(style::card()),
                widget::tooltip::Position::Bottom,
            ).into()
        };
        let wallet = row!(
            resource(COIN.clone(), wallet, "Gold"),
            resource(MUSHROOM.clone(), shrooms, "Mushrooms"),
            resource(HOURGLASS.clone(), glasses, "Hourglasses"),
            resource(LUCKY.clone(), lucky, "Lucky coins"),
        ).spacing(18).align_items(Alignment::Center);
        let identity = column!(
            widget::text(titlecase::titlecase(&player.name))
                .size(24).font(style::DISPLAY_FONT)
                .style(theme::Text::Color(style::GOLD))
                .width(Length::Fill).horizontal_alignment(Horizontal::Center),
            row!(
                widget::text(format!("Level {}", level.unwrap_or_default()))
                    .size(12).style(theme::Text::Color(style::MUTED)),
                horizontal_space(),
                widget::text(format!("{} % XP", (xp_done * 100.0).round() as u16))
                    .size(12).style(theme::Text::Color(style::ACCENT)),
            ).align_items(Alignment::Center),
            progress_bar(0.0..=1.0, xp_done as f32)
                .height(4).style(style::bar(style::ACCENT)),
        ).spacing(5).width(260);
        // Equal side columns keep the character centered even when resource values change.
        let header_width = self.window_w - 190.0 - 48.0
            - if self.window_w >= 1560.0 { 300.0 } else { 0.0 };
        let summary: Element<Message> = if header_width < 1080.0 {
            column!(
                container(identity).width(Length::Fill).center_x(),
                row!(wallet, horizontal_space(), full_bot).align_items(Alignment::Center),
            ).spacing(16).into()
        } else {
            row!(
                container(wallet).width(Length::FillPortion(1)),
                identity,
                container(full_bot).width(Length::FillPortion(1)).align_x(Horizontal::Right),
            ).spacing(20).align_items(Alignment::Center).into()
        };
        let header = container(summary).padding([18, 24]).width(Length::Fill);

        // The pages. They share the whole height down to the logout, each
        // plate the same size; only a window too short for them at their
        // normal size scrolls, so the switch above and the logout below
        // never move
        let b = page_badges(player, self.idle_tail().as_deref());
        let entries: Vec<(AccountPage, String, Option<Badge>)> = vec![
            (AccountPage::Tavern, t("Tavern").to_string(), b.tavern),
            (AccountPage::Scrapbook, t("Arena").to_string(), b.arena),
            (AccountPage::Guard, t("City guard").to_string(), b.guard),
            (AccountPage::Shops, t("Shops").to_string(), None),
            (AccountPage::ArenaManager, t("Arena Manager").to_string(), b.idle),
            (AccountPage::Pets, t("Pets").to_string(), b.pets),
            (AccountPage::Dungeons, t("Dungeons").to_string(), b.dungeon),
            (AccountPage::Fortress, t("Fortress").to_string(), b.fortress),
            (AccountPage::Underworld, t("Underworld").to_string(), b.underworld),
            (AccountPage::Guild, t("Guild").to_string(), b.guild),
            (AccountPage::Mail, mail_label, None),
            (AccountPage::Log, t("Activity log").to_string(), None),
            (AccountPage::Options, t("Character").to_string(), None),
        ];
        let viewer = player.ident;
        let pages = widget::responsive(move |size| {
            const GAP: f32 = 3.0;
            let normal = if compact { 36.0 } else { 48.0 };
            let n = entries.len() as f32;
            let fits = (size.height - GAP * (n - 1.0)) / n >= normal;
            let height = if fits { Length::Fill } else { Length::Fixed(normal) };
            let mut list = column!().spacing(GAP).width(Length::Fill);
            for (p, label, badge) in &entries {
                list = list.push(nav_plate(
                    viewer,
                    *p,
                    *p == page,
                    compact,
                    label.clone(),
                    badge.clone(),
                    height,
                ));
            }
            if fits {
                list.height(Length::Fill).into()
            } else {
                scrollable(list)
                    .height(Length::Fill)
                    .direction(widget::scrollable::Direction::Vertical(
                        widget::scrollable::Properties::new()
                            .width(0)
                            .scroller_width(0),
                    ))
                    .into()
            }
        });

        let sidebar = container(
            column!(
                container(
                    widget::text("SFCommander")
                        .font(style::DISPLAY_FONT)
                        .size(20)
                        .style(theme::Text::Color(style::ACCENT))
                )
                .padding([6, 0])
                .width(Length::Fill)
                .center_x(),
                pages,
                widget::tooltip(
                    button(
                        iced_aw::core::icons::bootstrap::icon_to_text(
                            iced_aw::Bootstrap::BoxArrowRight
                        )
                        .size(17)
                    )
                    .padding(7)
                    .style(style::quiet())
                    .on_press(Message::RemoveAccount {
                        ident: player.ident
                    }),
                    container(widget::text(t("Logout")))
                        .padding(8)
                        .style(style::card()),
                    widget::tooltip::Position::Right,
                ),
                widget::text(format!("v{}", crate::display_version()))
                    .size(10)
                    .style(theme::Text::Color(style::MUTED)),
            )
            .spacing(8)
            .align_items(Alignment::Center)
            .height(Length::Fill),
        )
        .padding([0, 9, 14, 9])
        .width(190)
        .height(Length::Fill)
        .style(style::side());

        let middle = match page {
            AccountPage::Scrapbook => {
                view_scrapbook(server, player, &self.config, &self.class_images)
            }
            AccountPage::Underworld => view_underworld(
                server, player, &self.config, &self.class_images,
            ),
            AccountPage::Fortress => view_fortress(player),
            AccountPage::Dungeons => view_dungeons(player),
            AccountPage::Tavern => view_tavern(player, self.tavern_tab),
            AccountPage::Guard => view_guard(player),
            AccountPage::Shops => view_shops(player),
            AccountPage::ArenaManager => view_arena_manager(player),
            AccountPage::Pets => view_pets(player),
            AccountPage::Guild => view_guild(
                player,
                self.window_w,
                self.window_h,
                &server.ident.url,
                &self.guild_history,
                self.config.grade_strictness,
                self.config.officer_seats,
                (self.config.guild_top_mark, self.config.guild_bottom_mark),
                self.config.guild_two_columns,
                self.config.guild_leader_tools,
                crate::ui::social::recruits(player, server),
                (self.config.chat_archive, self.chat_archive_note.as_deref()),
            ),
            AccountPage::Mail => view_mail(player),
            AccountPage::Log => self.view_log(player),
            AccountPage::Options => view_options(player, server, &self.config),
        };

        let (page_title, page_hint) = match page {
            AccountPage::Tavern => (
                t("Tavern"),
                t("Expeditions, daily rewards and remaining energy."),
            ),
            AccountPage::Scrapbook => {
                (t("Arena"), t("Opponents, battles and your scrapbook."))
            }
            AccountPage::Guard => {
                (t("City guard"), t("Your guard duty and earned gold."))
            }
            AccountPage::Shops => (
                t("Shops"),
                t("Equipment, purchases and character upgrades."),
            ),
            AccountPage::ArenaManager => (
                t("Arena Manager"),
                t("Buildings, income and your next steps."),
            ),
            AccountPage::Pets => {
                (t("Pets"), t("Your pets, feeding and battles."))
            }
            AccountPage::Dungeons => {
                (t("Dungeons"), t("Dungeon progress and available battles."))
            }
            AccountPage::Fortress => (
                t("Fortress"),
                t("Resources, troops and fortress development."),
            ),
            AccountPage::Underworld => (
                t("Underworld"),
                t("Souls, heroes and underworld development."),
            ),
            AccountPage::Guild => (
                t("Guild"),
                t("Members, shared progress and guild activity."),
            ),
            AccountPage::Mail => {
                (t("Mail"), t("Messages and rewards in one place."))
            }
            AccountPage::Log => {
                (t("Activity log"), t("Recent activity and achievements."))
            }
            AccountPage::Options => (
                t("Character"),
                t("Settings and automation for this character."),
            ),
        };
        let title = container(
            row!(
                column!(
                    widget::text(page_title)
                        .size(32)
                        .font(style::DISPLAY_FONT)
                        .style(theme::Text::Color(style::GOLD)),
                    widget::text(page_hint)
                        .size(13)
                        .style(theme::Text::Color(style::MUTED)),
                )
                .spacing(4),
                horizontal_space(),
                container(
                    widget::text(if self.preview {
                        "OFFLINE DEMO"
                    } else {
                        "SFCommander"
                    })
                    .size(10)
                    .style(theme::Text::Color(style::MUTED))
                )
                .padding([6, 10])
                .style(style::well()),
            )
            .align_items(Alignment::Center),
        )
        .padding([8, 24, 14, 24]);
        // The guild needs the entire work area: its own heading already
        // names the guild, and the roster must show all fifty members.
        let col_container = if page == AccountPage::Guild {
            column!(middle)
        } else {
            column!(header, title, middle)
        }.width(Length::Fill).height(Length::Fill);
        // Keep the work area readable on laptop screens. Both feeds remain
        // available in the Activity log, and chat on the Guild page.
        let mut workspace = row!(sidebar, col_container)
            .height(Length::Fill)
            .width(Length::Fill);
        if self.window_w >= 1560.0 && page != AccountPage::Guild {
            workspace = workspace.push(self.right_panel(player));
        }

        let page_body = column!(top, self.mode_banner(), workspace)
            .height(Length::Fill)
            .width(Length::Fill);
        let page_body = container(page_body)
            .height(Length::Fill)
            .width(Length::Fill)
            .style(style::aurora());

        // One box per level, clicked away one at a time. Coming back in the
        // morning and pressing OK four times is how you find out you made
        // four levels overnight
        // One overlay slot, two kinds of news. An epic outranks a level up:
        // a level comes back tomorrow, an epic might not
        let epic_first = !self.epic_popups.is_empty();
        let overlay = if epic_first {
            self.epic_box()
        } else {
            self.level_up_box()
        };
        let esc = if epic_first {
            Message::EpicPopupOk
        } else {
            Message::LevelPopupOk
        };
        iced_aw::Modal::new(page_body, overlay).on_esc(esc).into()
    }

    /// The epic find box. Deliberately loud and in the way - this is the one
    /// thing in the game worth interrupting whatever the user was doing
    fn epic_box(&self) -> Option<Element<'_, Message>> {
        let (who, what) = self.epic_popups.front()?;
        let more = self.epic_popups.len().saturating_sub(1);
        let ok = button(
            text(t("OK"))
                .size(14)
                .width(Length::Fill)
                .horizontal_alignment(Horizontal::Center),
        )
        .padding([6, 20])
        .width(Length::Fixed(120.0))
        .style(crate::ui::style::primary())
        .on_press(Message::EpicPopupOk);

        let mut body = column!(
            text(who.to_string())
                .size(13)
                .style(theme::Text::Color(style::MUTED)),
            text(what.to_string()).size(28),
        )
        .spacing(2)
        .align_items(Alignment::Center)
        .width(Length::Fill);
        if more > 0 {
            body = body.push(
                text(format!("{more} {}", t("more waiting")))
                    .size(12)
                    .style(theme::Text::Color(style::WARN)),
            );
        }
        Some(
            container(
                column!(
                    text(t("EPIC ITEM FOUND!"))
                        .size(20)
                        .style(theme::Text::Color(style::WARN)),
                    widget::horizontal_rule(1),
                    body,
                    ok,
                )
                .spacing(14)
                .align_items(Alignment::Center)
                .width(Length::Fixed(320.0)),
            )
            .padding(24)
            .style(style::card())
            .into(),
        )
    }

    /// The level up box, if one is waiting to be acknowledged
    fn level_up_box(&self) -> Option<Element<'_, Message>> {
        let (who, level) = self.level_popups.front()?;
        let more = self.level_popups.len().saturating_sub(1);
        let ok = button(
            text(t("OK"))
                .size(14)
                .width(Length::Fill)
                .horizontal_alignment(Horizontal::Center),
        )
        .padding([6, 20])
        .width(Length::Fixed(120.0))
        .style(crate::ui::style::primary())
        .on_press(Message::LevelPopupOk);

        let mut body = column!(
            text(who.to_string())
                .size(13)
                .style(theme::Text::Color(style::MUTED)),
            text(format!("{} {level}", t("Level"))).size(34),
        )
        .spacing(2)
        .align_items(Alignment::Center)
        .width(Length::Fill);
        if more > 0 {
            body = body.push(
                text(format!("{more} {}", t("more waiting")))
                    .size(12)
                    .style(theme::Text::Color(style::WARN)),
            );
        }
        Some(
            container(
                column!(
                    text(t("CONGRATULATIONS! LEVEL UP!"))
                        .size(18)
                        .style(theme::Text::Color(Color::WHITE)),
                    widget::horizontal_rule(1),
                    body,
                    ok,
                )
                .spacing(14)
                .align_items(Alignment::Center)
                .width(Length::Fixed(300.0)),
            )
            .padding(24)
            .style(style::card())
            .into(),
        )
    }

    fn view_settings(&self) -> Element<'_, Message> {
        let top_row = top_bar(
            text(t("Settings")).size(20).into(),
            if self.has_accounts() {
                Some(Message::ViewOverview)
            } else {
                Some(Message::ViewLogin)
            },
        );
        use AvailableTheme::*;
        let all_themes = [
            Commander, Light, Dark, Dracula, Nord, SolarizedLight,
            SolarizedDark, GruvboxLight, GruvboxDark, CatppuccinLatte,
            CatppuccinFrappe, CatppuccinMacchiato, CatppuccinMocha, TokyoNight,
            TokyoNightStorm, TokyoNightLight, KanagawaWave, KanagawaDragon,
            KanagawaLotus, Moonfly, Nightfly, Oxocarbon,
        ];

        let theme_picker = pick_list(
            all_themes,
            Some(self.config.theme),
            Message::ChangeTheme,
        )
        .width(Length::Fixed(200.0));

        let theme_row =
            row!(text(t("Theme: ")).width(Length::Fixed(100.0)), theme_picker)
                .width(Length::Fill)
                .align_items(Alignment::Center);

        let lang_picker = pick_list(
            crate::i18n::Lang::ALL,
            Some(self.config.lang),
            Message::SetLanguage,
        )
        .width(Length::Fixed(200.0));
        let lang_row = row!(
            text(t("Language:")).width(Length::Fixed(100.0)),
            lang_picker
        )
        .width(Length::Fill)
        .align_items(Alignment::Center);

        let auto_fetch_hof = checkbox(
            t("Fetch online HoF backup during login"),
            self.config.auto_fetch_newest,
        )
        .style(crate::ui::style::check())
        .on_toggle(Message::SetAutoFetch);

        let crawling_restrict = checkbox(
            t("Show advanced crawling options"),
            self.config.show_crawling_restrict,
        )
        .style(crate::ui::style::check())
        .on_toggle(Message::AdvancedLevelRestrict);

        let show_class_icons =
            checkbox(t("Show class icons"), self.config.show_class_icons)
                .style(crate::ui::style::check())
                .on_toggle(Message::ShowClasses);

        let max_threads = crate::ui::num_field(
            self.config.max_threads,
            50,
            Message::SetMaxThreads,
        );

        let max_threads =
            row!(text(t("Max threads:")), horizontal_space(), max_threads)
                .width(Length::Fill)
                .align_items(Alignment::Center);

        let start_threads = crate::ui::num_field(
            self.config.start_threads,
            50.min(self.config.max_threads),
            Message::SetStartThreads,
        );

        let start_threads = row!(
            text(t("Starting threads:")),
            horizontal_space(),
            start_threads
        )
        .width(Length::Fill)
        .align_items(Alignment::Center);

        let blacklist_threshold = crate::ui::num_field(
            self.config.blacklist_threshold,
            10,
            Message::SetBlacklistThr,
        );

        let blacklist_threshold = row!(
            text(t("Blacklist threshhold:")),
            horizontal_space(),
            blacklist_threshold
        )
        .width(Length::Fill)
        .align_items(Alignment::Center);

        let target_order = pick_list(
            crate::config::TargetOrder::ALL,
            Some(self.config.target_order),
            Message::SetTargetOrder,
        )
        .width(Length::Fixed(150.0));

        let target_order = row!(
            text(t("Default target order:")),
            horizontal_space(),
            target_order
        )
        .width(Length::Fill)
        .align_items(Alignment::Center);

        let auto_battle_losses = crate::ui::num_field(
            self.config.auto_battle_max_losses,
            20,
            Message::SetAutoBattleMaxLosses,
        );

        let auto_battle_losses = row!(
            text(t("Stop auto battle after x losses:")),
            horizontal_space(),
            auto_battle_losses
        )
        .width(Length::Fill)
        .align_items(Alignment::Center);

        // A quiet footer, not a banner. The name on it is the one he
        // plays under, which is the one the guild knows him by - a
        // copyright line does not need the one on his post
        let coffee = column!(
            widget::horizontal_rule(1),
            row!(
                text(format!(
                    "S&F Commander v{} \u{00B7} \u{00A9} {} {}",
                    crate::display_version(),
                    Local::now().format("%Y"),
                    crate::admin::AUTHOR,
                ))
                .size(11),
                horizontal_space(),
                coffee_icon(),
            )
            .align_items(Alignment::Center)
            .width(Length::Fill),
        )
        .spacing(6);

        // Owner corner: hidden behind a password, and the announcement is
        // only ever posted from the hard-coded owner characters
        let admin: Element<Message> = if self.config.admin_unlocked {
            let owner = self
                .servers
                .0
                .values()
                .flat_map(|s| s.accounts.values())
                .find(|a| crate::admin::is_owner(&a.name))
                .map(|a| a.name.clone());
            let posting_as = match owner {
                Some(name) => format!("{}: {name}", t("Posts as")),
                None => {
                    t("No owner character is logged in - nothing gets posted")
                        .to_string()
                }
            };
            let last = match self.config.announce_last {
                Some(at) => format!(
                    "{} {}",
                    t("Last posted:"),
                    at.format("%d.%m. %H:%M")
                ),
                None => t("Not posted yet").to_string(),
            };
            column!(
                widget::horizontal_rule(1),
                row!(
                    text(t("Owner: guild chat announcement")).size(14),
                    horizontal_space(),
                    button(text(t("Lock")).size(12))
                        .padding(4)
                        .style(crate::ui::style::quiet())
                        .on_press(Message::AdminLock),
                )
                .align_items(Alignment::Center),
                row!(
                    checkbox(t("Post every"), self.config.announce_enabled)
                        .style(crate::ui::style::check())
                        .on_toggle(Message::AnnounceSetEnabled)
                        .size(16)
                        .text_size(12),
                    crate::ui::num_field(
                        self.config.announce_hours,
                        48u8,
                        Message::AnnounceSetHours
                    ),
                    text(t("hours")).size(12),
                )
                .spacing(8)
                .align_items(Alignment::Center),
                widget::text_input(t("Message"), &self.config.announce_text)
                    .font(crate::ui::style::FAT)
                    .on_input(Message::AnnounceSetText)
                    .size(12)
                    .padding(5),
                row!(
                    text(posting_as).size(11),
                    horizontal_space(),
                    button(text(t("Post now")).size(12))
                        .padding(4)
                        .on_press(Message::AnnounceNow),
                )
                .align_items(Alignment::Center),
                text(last).size(11),
                widget::horizontal_rule(1),
                checkbox(
                    t("Discord invite once a day, at a random time between \
                       10:00 and midnight, from Petr Macinka only"),
                    self.config.discord_daily
                )
                .style(crate::ui::style::check())
                .on_toggle(Message::DiscordDailySet)
                .size(16)
                .text_size(12),
                text({
                    let today = Local::now().date_naive();
                    match (
                        self.config.discord_daily_last,
                        self.config.discord_daily_at,
                    ) {
                        (Some(d), _) if d == today => {
                            t("Posted today").to_string()
                        }
                        (_, Some(at)) if at.date() == today => {
                            format!("{} {}", t("Today at"), at.format("%H:%M"))
                        }
                        _ => t("Time not drawn yet").to_string(),
                    }
                })
                .size(11)
                .style(theme::Text::Color(style::MUTED)),
                widget::horizontal_rule(1),
                // The label is the switch, not the manual. All of it on one
                // line ran off the right edge of the card and the sentence
                // ended mid-word
                checkbox(t("Auto-kick"), self.config.auto_kick)
                    .style(crate::ui::style::check())
                    .on_toggle(Message::AutoKickSet)
                    .size(16)
                    .text_size(12),
                style::note(t(
                    "Removes members graded 5 - offline over 48 h and the \
                     other rules - one per guild refresh, never officers \
                     and never newcomers.",
                )),
                widget::horizontal_rule(1),
                checkbox(
                    t("Greet new members in the guild chat"),
                    self.config.greet_enabled
                )
                .style(crate::ui::style::check())
                .on_toggle(Message::GreetSetEnabled)
                .size(16)
                .text_size(12),
                widget::text_input(t("Greeting"), &self.config.greet_text)
                    .font(crate::ui::style::FAT)
                    .on_input(Message::GreetSetText)
                    .size(12)
                    .padding(5),
                text(t("Sent 1-2 minutes after somebody new shows up in \
                        the member list, once per person, from the owner \
                        character in that guild. {name} = their name"))
                .size(11)
                .style(theme::Text::Color(style::MUTED)),
                checkbox(
                    t("Say goodbye when somebody leaves"),
                    self.config.farewell_enabled
                )
                .style(crate::ui::style::check())
                .on_toggle(Message::FarewellSetEnabled)
                .size(16)
                .text_size(12),
                widget::text_input(t("Goodbye"), &self.config.farewell_text)
                    .font(crate::ui::style::FAT)
                    .on_input(Message::FarewellSetText)
                    .size(12)
                    .padding(5),
                widget::horizontal_rule(1),
                row!(
                    text(t("How many people use this")).size(13),
                    horizontal_space(),
                    button(text(t("Count them")).size(12))
                        .padding(4)
                        .style(crate::ui::style::quiet())
                        .on_press(Message::FetchUserCount),
                )
                .align_items(Alignment::Center),
                text(match &self.user_count {
                    None => t("Not asked yet").to_string(),
                    Some(Err(e)) => format!("{}: {e}", t("Could not ask")),
                    // The API answers `null` when the Vercel project has
                    // no Redis behind it - the app side is fine, the
                    // store is what is missing
                    Some(Ok((None, None))) =>
                        t("The server has no store yet: in the Vercel \
                         project add Storage > Upstash Redis (the API picks \
                         it up on its own, nothing to change here)",)
                        .to_string(),
                    Some(Ok((today, week))) => {
                        let n = |v: &Option<u64>| match v {
                            Some(v) => v.to_string(),
                            None => "?".to_string(),
                        };
                        format!(
                            "{} {} \u{00B7} {} {}",
                            n(today),
                            t("today"),
                            n(week),
                            t("in the last 7 days")
                        )
                    }
                })
                .size(12)
                .style(theme::Text::Color(style::WARN)),
                text(t("Each install makes up a random number for itself on \
                     first start and sends only that with the update check. \
                     No character, no server, no login. The numbers are kept \
                     for eight days and then thrown away, so the only thing \
                     they can answer is how many different installs there \
                     are.",))
                .size(11)
                .style(theme::Text::Color(style::MUTED)),
                widget::horizontal_rule(1),
                text(crate::admin::DISCORD_INVITE).size(11),
                row!(
                    button(text(t("Post the Discord invite")).size(12))
                        .padding(4)
                        .style(crate::ui::style::quiet())
                        .on_press(Message::AnnounceDiscord),
                    horizontal_space(),
                )
                .align_items(Alignment::Center),
            )
            .spacing(6)
            .into()
        } else {
            column!(
                widget::horizontal_rule(1),
                row!(
                    widget::text_input(t("Owner password"), &self.admin_input)
                        .font(crate::ui::style::FAT)
                        .on_input(Message::AdminInput)
                        .on_submit(Message::AdminUnlock)
                        .secure(true)
                        .size(12)
                        .padding(5),
                    button(text(t("Unlock")).size(12))
                        .padding(5)
                        .on_press(Message::AdminUnlock),
                )
                .spacing(8)
                .align_items(Alignment::Center),
            )
            .spacing(6)
            .into()
        };

        // One slider, and what it currently means. The pace switch, the
        // sessions switch and the "lasting N hours" box are gone: they
        // each moved a part of the same thing and could contradict one
        // another (Full Stealth sessions at Full Power pace farmed
        // through the night). The slider in the top bar is the control;
        // this is the explanation
        let pace_row = column!(
            row!(
                text(t("How it plays:")).size(12),
                text(self.config.safety_line())
                    .size(12)
                    .font(iced::Font {
                        weight: iced::font::Weight::Bold,
                        ..iced::Font::DEFAULT
                    })
                    .style(theme::Text::Color(style::GOOD)),
                text(format!("({})", self.config.safety)).size(12),
            )
            .spacing(6)
            .align_items(Alignment::Center),
            text(t(
                "Set with the one slider in the top bar. At 0 the app is \
                 off: nothing goes out at all. From 1 up it plays in \
                 sessions - drawn every time and never the same twice, with \
                 a stretch away in between. The further right the slider, \
                 the sooner the next session comes round and the longer it \
                 runs: at 1 that is a short look-in about three times a day, \
                 at 10 a sitting of ordinary length every half hour or so, \
                 all day. It judges for itself how much there is to do - \
                 thirst left, an expedition under way, free fights up, a \
                 raid soon: with plenty to do it comes back within the hour \
                 and stays longer, with nothing to do it logs off early. The \
                 pauses between clicks and the cooldowns are respected at \
                 every position, and the night rest is its own tick below.",
            ))
            .size(11),
        )
        .spacing(4);

        // Queue order: every row has a position box - pick "1" and the job
        // jumps to the top. No drag & drop in this toolkit, but one click
        // per move is close enough
        let order = crate::player::Job::order();
        let positions: Vec<usize> = (1..=order.len()).collect();
        let mut order_col = column!(
            row!(
                text(t("Queue order (1 runs first):")),
                horizontal_space(),
                button(text(t("Default")).size(11))
                    .padding([2, 6])
                    .style(crate::ui::style::quiet())
                    .on_press(Message::ResetJobOrder),
            )
            .align_items(Alignment::Center),
        )
        .spacing(3);
        for (i, job) in order.iter().enumerate() {
            let job = *job;
            order_col = order_col.push(
                row!(
                    pick_list(positions.clone(), Some(i + 1), move |p| {
                        Message::MoveJobTo { job, pos: p - 1 }
                    })
                    .text_size(12)
                    .padding([2, 6])
                    .width(Length::Fixed(64.0)),
                    text(t(job.label())).size(12).width(Length::Fill),
                )
                .spacing(8)
                .align_items(Alignment::Center),
            );
        }

        let save_row = row!(
            button(text(t("Save")).size(13))
                .padding([5, 12])
                .on_press(Message::SaveSettings),
            text(match self.settings_saved_at {
                Some(at) => format!("{} {}", t("Saved"), at.format("%H:%M:%S")),
                None => t(
                    "Every change is saved on its own - this just confirms it"
                )
                .to_string(),
            })
            .size(11),
        )
        .spacing(10)
        .align_items(Alignment::Center);

        // ------------------------------------------------------- stealth
        // What the server can see of a client is timing. These are the
        // knobs on it (see `stealth`)
        // Every number box gets a fixed width: left to itself it fills the
        // row and the words after it wrap one per line down the edge
        let narrow = Length::Fixed(72.0);
        let mut stealth = column!(
            style::heading(t("Staying unremarkable")),
            row!(
                checkbox(t("Quiet hours from"), self.config.quiet_hours)
.style(crate::ui::style::check())
                    .on_toggle(Message::SetQuietHours)
                    .size(16)
                    .text_size(12),
                crate::ui::num_field(self.config.quiet_from, 23u8, Message::SetQuietFrom)
                    .width(narrow),
                text(t("to")).size(12),
                crate::ui::num_field(self.config.quiet_to, 24u8, Message::SetQuietTo)
                    .width(narrow),
                text(t("o'clock")).size(12),
            )
            .spacing(6)
            .align_items(Alignment::Center),
            text(t("Night rest: in these hours the bot does nothing at all. The edges move by up to 45 min every day, so it never stops or starts at the same minute."))
                .size(11)
                .style(theme::Text::Color(style::MUTED)),
            row!(
                text(t("At most")).size(12),
                crate::ui::num_field(
                    self.config.requests_per_min,
                    60u8,
                    Message::SetRequestsPerMin
                )
                .width(narrow),
                text(t("requests a minute per character")).size(12),
            )
            .spacing(6)
            .align_items(Alignment::Center),
            text(t("A person clicks a few times a minute; a script fires dozens. A full queue waits rather than pass this. Several characters also never start together: each gets its own delay into a session and its own minute after midnight."))
                .size(11)
                .style(theme::Text::Color(style::MUTED)),
            text(self.attendance_line())
                .size(12)
                .style(theme::Text::Color(style::ACCENT)),
            text(t("Nobody real takes every portal, every defense and every attack. The bot skips a few on purpose - about 15 % of portals, 10 % of defenses, 5 % of attacks at BALANCED, twice that at STEALTH, none at NO SHAME - and keeps count so the misses never pile up."))
                .size(11)
                .style(theme::Text::Color(style::MUTED)),
        )
        .spacing(6);
        // The proxy list is the owner's tool for several characters on one
        // machine - behind the owner password, and in plain words. For
        // everybody else it was a box nobody understood
        if self.config.admin_unlocked {
            stealth =
                stealth.push(text(t("Proxy per character (owner)")).size(12));
            stealth = stealth.push(
                text(t("Leave empty unless you run several characters and want each to come from a different internet address. Then paste the address your proxy provider gave you, e.g. http://user:password@host:port."))
                    .size(11)
                    .style(theme::Text::Color(style::MUTED)),
            );
        }
        for (index, acc) in self.config.accounts.iter().enumerate() {
            if !self.config.admin_unlocked {
                break;
            }
            let mut entries: Vec<(Option<usize>, String, String)> = Vec::new();
            match acc {
                crate::config::AccountConfig::Regular {
                    name,
                    server,
                    config,
                    ..
                } => entries.push((
                    None,
                    format!("{name} ({})", get_server_code(server)),
                    config.proxy.clone(),
                )),
                crate::config::AccountConfig::SF { characters, .. } => {
                    for (sub, c) in characters.iter().enumerate() {
                        entries.push((
                            Some(sub),
                            format!(
                                "{} ({})",
                                c.ident.name,
                                get_server_code(&c.ident.server)
                            ),
                            c.config.proxy.clone(),
                        ));
                    }
                }
            }
            for (sub, label, url) in entries {
                stealth = stealth.push(
                    row!(
                        text(label).size(12).width(Length::Fixed(150.0)),
                        widget::text_input("", &url)
                            .font(crate::ui::style::FAT)
                            .on_input(move |url| Message::SetProxy {
                                index,
                                sub,
                                url
                            })
                            .size(12)
                            .padding(4),
                    )
                    .spacing(8)
                    .align_items(Alignment::Center),
                );
            }
        }

        // ------------------------------------------------------- licence
        //
        // Behind the owner password, and nowhere else. Nothing is being
        // enforced yet, so to everybody who is not him this panel is a
        // box asking for a code that does not exist and a warning about
        // a seal that is not set - fifty people in the guild reading that
        // is fifty people asking him what it means. It comes out from
        // behind the password on the day a key is compiled in
        let licence_panel: Element<Message> = if !self.config.admin_unlocked {
            column!().into()
        } else {
            use crate::licence::{Status, fingerprint};
            // Whole sentences with the number put in, rather than three
            // pieces glued together - a sentence assembled from fragments
            // cannot be translated into a language that puts them in
            // another order
            let fill = |key: &'static str, n: i64| {
                t(key).replace("{}", &n.to_string())
            };
            let (line, colour) = match &self.licence {
                Status::Valid { days_left } => {
                    (fill("Valid, {} days left", *days_left), style::GOOD)
                }
                Status::EndingSoon { days_left } => (
                    fill("Runs out in {} days - time to renew", *days_left),
                    style::WARN,
                ),
                Status::Expired { days_over } => (
                    fill(
                        "Ran out {} days ago - the automation is off",
                        *days_over,
                    ),
                    style::BAD,
                ),
                Status::WrongMachine { issued_for } => (
                    t("This licence was issued for another computer: {}")
                        .replace("{}", issued_for),
                    style::BAD,
                ),
                // Not "no licence yet", which reads like a form with a
                // field left blank. This one is a seal that has not been
                // set, and it says what that means
                Status::Missing => (
                    t("Unsealed. Nothing binds this copy to anybody - and \
                       what is not bound can be taken back.")
                    .to_string(),
                    style::WARN,
                ),
                Status::Bad(e) => (e.to_string(), style::BAD),
            };
            // The machine's own code, which is what the author needs in
            // order to issue anything. A button rather than a line to read
            // out: it is twenty characters and people mistype them
            let machine = row!(
                text(t("This computer:")).size(12),
                horizontal_space(),
                text(fingerprint())
                    .size(13)
                    .style(theme::Text::Color(style::NAME_GOLD)),
                button(text(t("Copy")).size(11))
                    .padding([2, 6])
                    .style(style::brass(false))
                    .on_press(Message::LicenceCopyMachine),
            )
            .spacing(6)
            .align_items(Alignment::Center);

            let paste = row!(
                widget::text_input(
                    t("Paste the licence code"),
                    &self.licence_input
                )
                .font(style::FAT)
                .size(12)
                .on_input(Message::LicenceInput)
                .on_submit(Message::LicenceApply),
                button(text(t("Activate")).size(12))
                    .padding([4, 8])
                    .style(style::brass(true))
                    .on_press(Message::LicenceApply),
            )
            .spacing(6)
            .align_items(Alignment::Center);

            let mut col = column!(
                text(t("Licence")).size(15),
                text(line).size(13).style(theme::Text::Color(colour)),
                machine,
                paste,
            )
            .spacing(8)
            .width(Length::Fill);
            if !crate::licence::enforced() {
                col = col.push(style::note(t(
                    "This build has no licence key compiled in, so nothing \
                     is being enforced.",
                )));
            }
            col.into()
        };

        // ---------------------------------------------------------- tabs
        // This page was one column: language, theme, the pace, the whole
        // queue order, every stealth knob, the owner corner and the
        // licence, one under the other. Finding the licence meant
        // scrolling past forty other things, and changing the theme meant
        // scrolling back up past them. Four pages, each of which fits
        let mut tabs = row!().spacing(6).align_items(Alignment::Center);
        for tab in crate::SettingsTab::ALL {
            let active = tab == self.settings_tab;
            tabs = tabs.push(
                button(text(t(tab.label())).size(13))
                    .padding([6, 14])
                    .style(if active {
                        style::primary()
                    } else {
                        style::quiet()
                    })
                    .on_press(Message::ShowSettingsTab(tab)),
            );
        }

        let body: Element<Message> = match self.settings_tab {
            crate::SettingsTab::Look => column!(
                lang_row, theme_row, show_class_icons, crawling_restrict,
            )
            .spacing(18)
            .into(),
            crate::SettingsTab::Play => column!(
                pace_row,
                widget::horizontal_rule(1),
                auto_fetch_hof,
                max_threads,
                start_threads,
                blacklist_threshold,
                target_order,
                auto_battle_losses,
            )
            .spacing(18)
            .into(),
            crate::SettingsTab::Queue => order_col.into(),
            crate::SettingsTab::Stealth => stealth.into(),
            // His lock first, the licence under it - the two things that
            // are nobody else's business, on the page nobody else opens.
            // Locked, this tab is the password box and nothing else: no
            // rule under it with empty space below, which is a page that
            // looks like it failed to load
            crate::SettingsTab::Owner if !self.config.admin_unlocked => {
                column!(admin).spacing(18).into()
            }
            crate::SettingsTab::Owner => {
                column!(admin, widget::horizontal_rule(1), licence_panel)
                    .spacing(18)
                    .into()
            }
        };

        let settings_column = column!(tabs, body, save_row, coffee)
            .width(Length::Fixed(620.0))
            .spacing(20);

        // A tab can still be taller than the window - the stealth one is -
        // so it scrolls, but now only as far as one page of it goes
        column!(
            top_row,
            scrollable(
                column!(
                    container(settings_column).padding(24).style(style::card())
                )
                .width(Length::Fill)
                .padding([0, 0, 30, 0])
                .align_items(Alignment::Center)
            )
            .height(Length::Fill)
        )
        .spacing(20)
        .height(Length::Fill)
        .width(Length::Fill)
        .align_items(Alignment::Center)
        .into()
    }

    fn view_overview(
        &self,
        selected: &HashSet<AccountIdent>,
        currrent_action: &Option<ActionSelection>,
    ) -> Element<'_, Message> {
        // Every character in the list runs its own automation at the same
        // time - the button is the way to add the next one
        let top_bar = top_bar(
            row!(
                text("S&F Commander").size(20),
                button(text(t("+ Add character / account")).size(12))
                    .padding(4)
                    .on_press(Message::ViewLogin),
            )
            .spacing(16)
            .align_items(Alignment::Center)
            .into(),
            Some(Message::ViewLogin),
        );

        let mut accounts = column!()
            .padding(20)
            .spacing(5)
            .width(Length::Fill)
            .align_items(Alignment::Center);

        let info_row = row!(
            center(text(t("Status")).width(acc_status_width())),
            center(text(t("Server")).width(server_code_width())),
            text(t("Name")).width(acc_name_width()),
            horizontal_space(),
            center(text(t("Underworld")).width(underworld_width())),
            center(text(t("Arena")).width(next_fight_width())),
            center(text(t("Scrapbook")).width(scrapbook_count_width())),
            text(t("Crawling")).width(crawling_status_width()),
        )
        .spacing(10.0)
        .width(Length::Fill)
        .padding(5.0);

        let all_active: Vec<_> = self
            .servers
            .0
            .values()
            .flat_map(|a| a.accounts.values())
            .map(|a| a.ident)
            .collect();

        let cb = checkbox("", all_active.iter().all(|a| selected.contains(a)))
            .style(crate::ui::style::check())
            .on_toggle(move |nv| Message::SetOverviewSelected {
                ident: all_active.clone(),
                val: nv,
            })
            .size(13.0);

        let this_action = Some(ActionSelection::Multi);
        let is_acting = currrent_action == &this_action;

        let mut action_button = button(
            iced_aw::core::icons::bootstrap::icon_to_text(
                iced_aw::Bootstrap::ThreeDotsVertical,
            )
            .size(18.0),
        )
        .padding(4.0);

        if is_acting {
            action_button = action_button.on_press(Message::SetAction(None))
        } else if !selected.is_empty() {
            action_button =
                action_button.on_press(Message::SetAction(this_action))
        }

        let action_dd =
            DropDown::new(action_button, self.overview_actions(), is_acting)
                .width(Length::Fill)
                .on_dismiss(Message::SetAction(None))
                .alignment(iced_aw::drop_down::Alignment::BottomStart);

        let full_row =
            row!(cb, info_row, action_dd).align_items(Alignment::Center);

        accounts = accounts.push(full_row);

        let mut servers: Vec<_> = self.servers.0.values().collect();
        servers.sort_by_key(|a| &a.ident.ident);
        for server in servers {
            let server_status: Box<str> = match &server.crawling {
                CrawlingStatus::Waiting => "Waiting".into(),
                CrawlingStatus::Restoring => "Restoring".into(),
                CrawlingStatus::CrawlingFailed(_) => "Error".into(),
                CrawlingStatus::Crawling { que, .. } => {
                    let lock = que.lock_safe();
                    let remaining = lock.count_remaining();
                    drop(lock);
                    if remaining == 0 {
                        "Finished".into()
                    } else {
                        remaining
                            .to_formatted_string(&self.config.num_format)
                            .into()
                    }
                }
            };

            let mut accs: Vec<_> = server.accounts.values().collect();
            accs.sort_by_key(|a| &a.name);
            for acc in accs {
                let info_row =
                    overview_row(acc, server, &server_status, &self.config);
                let selected = selected.contains(&acc.ident);

                let ident = acc.ident;

                let cb = checkbox("", selected)
                    .style(crate::ui::style::check())
                    .on_toggle(move |nv| Message::SetOverviewSelected {
                        ident: vec![ident],
                        val: nv,
                    })
                    .size(13.0);

                let this_action = Some(ActionSelection::Character(ident));
                let is_acting = currrent_action == &this_action;

                let action_button = button(
                    iced_aw::core::icons::bootstrap::icon_to_text(
                        iced_aw::Bootstrap::ThreeDotsVertical,
                    )
                    .size(18.0),
                )
                .on_press(if is_acting {
                    Message::SetAction(None)
                } else {
                    Message::SetAction(this_action)
                })
                .padding(4.0);

                let action_dd = DropDown::new(
                    action_button,
                    self.overview_actions(),
                    is_acting,
                )
                .width(Length::Fill)
                .on_dismiss(Message::SetAction(None))
                .alignment(iced_aw::drop_down::Alignment::BottomStart);

                let full_row = row!(cb, info_row, action_dd)
                    .spacing(5.0)
                    .align_items(Alignment::Center);

                accounts = accounts.push(full_row);
            }
        }

        let footer = row!(horizontal_space(), coffee_icon()).padding([0, 10]);

        column!(
            top_bar,
            widget::scrollable(
                column!(accounts, self.harvest_panel())
                    .spacing(10)
                    .width(Length::Fill)
            ),
            footer
        )
        .spacing(5)
        .height(Length::Fill)
        .width(Length::Fill)
        .align_items(Alignment::Center)
        .into()
    }

    /// How often the bot has to look in, worked out rather than guessed at.
    ///
    /// Every store in the game - the fortress's three, the underworld's two
    /// - fills at a known rate into a known cap, and once it is full the
    /// production is thrown away. The tightest of them is therefore the
    /// longest gap between visits at which nothing is lost, and since fewer
    /// sessions is better for every other reason, that number is the answer.
    /// Across several characters the tightest one wins, because the slider
    /// is one setting for all of them
    /// Everything every logged-in character produces, with the character it
    /// belongs to. One list, because the slider is one setting for all of
    /// them: a six-hour wood store on one account is no comfort when
    /// another one fills in forty minutes
    fn all_producers(&self) -> Vec<(String, crate::harvest::Producer)> {
        let mut all = Vec::new();
        for server in self.servers.0.values() {
            for account in server.accounts.values() {
                let lock = account.status.lock_safe();
                let (AccountStatus::Idle(_, gs) | AccountStatus::Busy(gs, _)) =
                    &*lock
                else {
                    continue;
                };
                for p in crate::harvest::producers(gs) {
                    all.push((account.name.clone(), p));
                }
            }
        }
        all
    }

    /// The slider position the arithmetic asks for, or `None` while nothing
    /// is logged in or nothing can overflow
    fn recommended_slider(&self) -> Option<u8> {
        let all = self.all_producers();
        if all.is_empty() {
            return None;
        }
        let producers: Vec<crate::harvest::Producer> =
            all.into_iter().map(|(_, p)| p).collect();
        let floor = crate::stealth::gap_minutes(crate::stealth::MAX_LEVEL);
        let plan = crate::harvest::plan(&producers, floor);
        plan.binding.is_some().then_some(plan.slider)
    }

    fn harvest_panel(&self) -> Element<'_, Message> {
        use crate::harvest;
        // The tightest store of every character, and the character it
        // belongs to - a five-hour wood store on one account is no comfort
        // when another one fills in forty minutes
        let all = self.all_producers();
        if all.is_empty() {
            return column!().into();
        }
        let producers: Vec<harvest::Producer> =
            all.iter().map(|(_, p)| p.clone()).collect();
        // Never faster than the slider's own quickest rung: below that the
        // arithmetic is asking for a pattern no person has
        let floor = crate::stealth::gap_minutes(crate::stealth::MAX_LEVEL);
        let plan = harvest::plan(&producers, floor);
        let now = self.config.safety;

        let who = all
            .iter()
            .filter(|(_, p)| Some(&p.name) == plan.binding.as_ref())
            .map(|(who, _)| who.as_str())
            .next()
            .unwrap_or("");

        let head = match &plan.binding {
            Some(name) => format!(
                "{} {} \u{2013} {name}{} {}",
                t("Fills first:"),
                crate::util::long_wait(i64::from(plan.gap_minutes) * 60),
                if who.is_empty() {
                    String::new()
                } else {
                    format!(" ({who})")
                },
                t("is what sets the pace"),
            ),
            None => t("Nothing can overflow - visit as rarely as you like")
                .to_string(),
        };

        // The three figures the decision is made on
        let tiles = row!(
            style::tile_sub(
                t("Recommended gap"),
                crate::util::long_wait(i64::from(plan.gap_minutes) * 60),
                style::GOOD,
                Some(format!(
                    "{} {}\u{00d7} {}",
                    t("about"),
                    plan.visits_per_day,
                    t("a day")
                )),
            ),
            style::tile_sub(
                t("Slider setting"),
                plan.slider.to_string(),
                if now == plan.slider {
                    style::GOOD
                } else {
                    style::WARN
                },
                Some(format!(
                    "{} {}",
                    t("really every"),
                    crate::util::long_wait(
                        i64::from(plan.slider_gap_minutes) * 60
                    )
                )),
            ),
            style::tile_sub(
                t("Slider now"),
                now.to_string(),
                if now == 0 {
                    style::BAD
                } else if now >= plan.slider {
                    style::GOOD
                } else {
                    style::BAD
                },
                Some(if now == 0 {
                    t("off - nothing is collected").to_string()
                } else {
                    format!(
                        "{} {}",
                        t("every"),
                        crate::util::long_wait(
                            i64::from(crate::stealth::gap_minutes(now)) * 60
                        )
                    )
                }),
            ),
        )
        .spacing(8)
        .width(Length::Fill);

        // One line per store: how fast it fills, and what the slider as it
        // stands is throwing away. This is the table that answers "why is my
        // stone always at the cap"
        let gap_now = if now == 0 {
            // Off: nothing is collected at all, so everything overflows
            24 * 60
        } else {
            crate::stealth::gap_minutes(now)
        };
        let mut table = column!().spacing(2).width(Length::Fill);
        let mut seen: Vec<(String, String)> = Vec::new();
        for (who, p) in &all {
            let key = (who.clone(), p.name.clone());
            if seen.contains(&key) {
                continue;
            }
            seen.push(key);
            let label = format!("{} \u{00B7} {who}", p.name);
            let (value, colour) = match p.full_in_minutes() {
                Some(full) => {
                    let waste = p.wasted_share(gap_now);
                    (
                        format!(
                            "{} {} \u{2013} {}",
                            t("full in"),
                            crate::util::long_wait(i64::from(full) * 60),
                            if waste <= 0.0 {
                                t("nothing lost").to_string()
                            } else {
                                format!(
                                    "{} {:.0} %",
                                    t("throwing away"),
                                    waste * 100.0
                                )
                            }
                        ),
                        if waste <= 0.0 {
                            style::GOOD
                        } else {
                            style::BAD
                        },
                    )
                }
                None => match p.per_day_at(gap_now) {
                    Some(n) => (
                        format!("{n}\u{00d7} {}", t("a day at this pace")),
                        style::MUTED,
                    ),
                    None => continue,
                },
            };
            table = table.push(style::line_in(label, value, Some(colour)));
        }

        let apply = button(
            text(format!("{} {}", t("Set the slider to"), plan.slider))
                .size(12),
        )
        .padding([5, 10])
        .style(style::brass(now == plan.slider))
        .on_press(Message::SetSafety(plan.slider));

        container(
            style::panel(
                t("How often to look in"),
                column!(
                    tiles,
                    style::note(head),
                    table,
                    row!(
                        apply,
                        style::note(t(
                            "A store that is full has stopped earning, so the \
                             gap is the one that keeps the tightest of them \
                             just under its cap. The free fights spoil \
                             nothing - they are listed because they are a \
                             reason to look in more often that has nothing to \
                             do with waste.",
                        )),
                    )
                    .spacing(10)
                    .align_items(Alignment::Center),
                )
                .spacing(8),
            )
            .max_width(1180.0),
        )
        .width(Length::Fill)
        .padding([0, 20, 20, 20])
        .center_x()
        .into()
    }
    fn overview_actions(&self) -> Element<'_, Message> {
        let mut all_actions = column!().spacing(4.0);

        fn action(button: Button<Message>) -> Button<Message> {
            button.width(100.0)
        }

        all_actions = all_actions.push(action(
            button(row!(
                text(t("Auto Battle")),
                horizontal_space(),
                iced_aw::core::icons::bootstrap::icon_to_text(
                    iced_aw::Bootstrap::Check,
                )
            ))
            .on_press(Message::MultiAction {
                action: OverviewAction::AutoBattle(true),
            }),
        ));

        all_actions = all_actions.push(action(
            button(row!(
                text(t("Auto Battle")),
                horizontal_space(),
                iced_aw::core::icons::bootstrap::icon_to_text(
                    iced_aw::Bootstrap::X,
                )
            ))
            .on_press(Message::MultiAction {
                action: OverviewAction::AutoBattle(false),
            }),
        ));

        all_actions = all_actions.push(action(
            button(t("Logout"))
                .on_press(Message::MultiAction {
                    action: OverviewAction::Logout,
                })
                .style(crate::ui::style::danger()),
        ));

        all_actions.into()
    }
}

#[derive(Debug, Clone, Copy)]
pub enum OverviewAction {
    Logout,
    AutoBattle(bool),
}

/// The overview's columns. A column is as wide as the widest word that
/// will stand in it - the heading in whichever language is on, or the
/// longest thing the cell can say - and never narrower than the number it
/// was drawn to. These were seven hard numbers, tuned for the system face,
/// and against the drawn lettering they left "Podsvětí" reading "Podsvě"
/// and "Offline preview" reading "Offline pr"
fn col_w(min: f32, labels: &[&'static str]) -> f32 {
    let h = 16.0 * glyphs::INK;
    labels
        .iter()
        .map(|l| glyphs::measure(t(l), h) + 8.0)
        .fold(min, f32::max)
}

fn acc_status_width() -> f32 {
    col_w(80.0, &["Status", "Offline preview", "Waiting", "Active"])
}
fn acc_name_width() -> f32 {
    col_w(200.0, &["Name"])
}
fn server_code_width() -> f32 {
    col_w(50.0, &["Server"])
}
fn scrapbook_count_width() -> f32 {
    col_w(60.0, &["Scrapbook"])
}
fn next_fight_width() -> f32 {
    col_w(60.0, &["Arena"])
}
fn underworld_width() -> f32 {
    col_w(60.0, &["Underworld"])
}
fn crawling_status_width() -> f32 {
    col_w(80.0, &["Crawling"])
}

fn overview_row<'a>(
    acc: &'a AccountInfo,
    server: &'a ServerInfo,
    crawling_status: &'_ str,
    config: &'a Config,
) -> Element<'a, Message> {
    let status_text = |t: &str| center(text(t).width(acc_status_width()));

    let mut next_free_fight = None;

    let acc_status = match &*acc.status.lock_safe() {
        AccountStatus::LoggingIn => status_text("Logging in"),
        AccountStatus::Idle(_, gs) => {
            next_free_fight = Some(gs.arena.next_free_fight);
            status_text("Active")
        }
        AccountStatus::Busy(gs, reason) => {
            next_free_fight = Some(gs.arena.next_free_fight);
            status_text(reason)
        }
        AccountStatus::FatalError(_) => status_text("Error!"),
        AccountStatus::LoggingInAgain => status_text("Logging in"),
    };

    let server_code = center(
        text(get_server_code(&server.ident.url)).width(server_code_width()),
    );

    let acc_name = text(titlecase::titlecase(acc.name.as_str()).to_string())
        .width(acc_name_width());

    let scrapbook_count: String = match &acc.scrapbook_info {
        Some(si) => si
            .scrapbook
            .items
            .len()
            .to_formatted_string(&config.num_format),
        None => "".into(),
    };
    let scrapbook_count = text(scrapbook_count)
        .width(scrapbook_count_width())
        .horizontal_alignment(Horizontal::Center);

    let icon_to_text =
        |icon| iced_aw::core::icons::bootstrap::icon_to_text(icon).size(18.0);

    let abs = acc
        .scrapbook_info
        .as_ref()
        .map(|a| {
            if a.auto_battle {
                iced_aw::Bootstrap::LightningFill
            } else {
                iced_aw::Bootstrap::Lightning
            }
        })
        .unwrap_or(iced_aw::Bootstrap::Question);

    // The two icon arms come out of the Bootstrap icon font and the
    // countdown out of his lettering, so they meet as elements rather than
    // as one widget type
    let next_free_fight: Element<Message> = match next_free_fight {
        None => center(icon_to_text(iced_aw::Bootstrap::Question).width(25.0))
            .into(),
        Some(Some(x)) if x >= Local::now() => text(remaining_minutes(x))
            .width(25.0)
            .horizontal_alignment(Horizontal::Center)
            .into(),
        Some(_) => {
            center(icon_to_text(iced_aw::Bootstrap::Check).width(25.0)).into()
        }
    };

    let next_free_fight = row!(next_free_fight, center(icon_to_text(abs)))
        .align_items(Alignment::Center)
        .spacing(4.0);

    let next_free_fight = column!(next_free_fight)
        .align_items(Alignment::Center)
        .width(next_fight_width());

    let underworld_info: Element<Message> = acc
        .underworld_info
        .as_ref()
        .map(|a| {
            let auto_status = if a.auto_lure {
                iced_aw::Bootstrap::LightningFill
            } else {
                iced_aw::Bootstrap::Lightning
            };

            let remaining = crate::mechanics::remaining_lures(&a.underworld);
            let remaining: Element<Message> = if remaining == 0 {
                center(icon_to_text(iced_aw::Bootstrap::Check).width(25.0))
                    .into()
            } else {
                text(remaining.to_string())
                    .width(25.0)
                    .horizontal_alignment(Horizontal::Center)
                    .into()
            };

            let row = row!(remaining, center(icon_to_text(auto_status)))
                .align_items(Alignment::Center)
                .spacing(4.0);

            column!(row)
                .width(underworld_width())
                .align_items(Alignment::Center)
                .into()
        })
        .unwrap_or(
            center(icon_to_text(iced_aw::Bootstrap::X))
                .width(underworld_width())
                .into(),
        );

    let crawling_status = text(crawling_status).width(crawling_status_width());

    let info_row = row!(
        acc_status,
        server_code,
        acc_name,
        horizontal_space(),
        underworld_info,
        next_free_fight,
        scrapbook_count,
        crawling_status
    )
    .spacing(10.0)
    .align_items(Alignment::Center);

    button(info_row)
        .on_press(Message::ShowPlayer { ident: acc.ident })
        .width(Length::Fill)
        .height(Length::Shrink)
        .padding(4.0)
        .style(crate::ui::style::quiet())
        .into()
}

fn remaining_minutes(time: DateTime<Local>) -> String {
    let now = Local::now();
    let remaining = (time - now).num_seconds().max(0);
    let secs = remaining % 60;
    let mins = remaining / 60;
    format!("{mins}:{secs:02}")
}

/// Centre a label, whichever face it is set in. Most of the app is in his
/// lettering now, but the handful of icons still come out of the Bootstrap
/// icon font, and both meet in the same rows
trait Centred {
    #[must_use]
    fn centred(self) -> Self;
}

impl Centred for iced::widget::Text<'_> {
    fn centred(self) -> Self {
        self.horizontal_alignment(Horizontal::Center)
    }
}

impl Centred for glyphs::Lettered {
    fn centred(self) -> Self {
        self.horizontal_alignment(Horizontal::Center)
    }
}

fn center<T: Centred>(t: T) -> T {
    t.centred()
}

pub fn view_crawling<'a>(
    server: &'a ServerInfo,
    config: &'a Config,
) -> Element<'a, Message> {
    let mut left_col = column!().align_items(Alignment::Center).spacing(10);

    let sid = server.ident.id;

    match &server.crawling {
        CrawlingStatus::Crawling {
            threads,
            que,
            player_info,
            ..
        } => {
            let lock = que.lock_safe();
            let remaining = lock.count_remaining();
            let crawled = player_info.len();
            let total = remaining + crawled;

            let progress_text = text(format!(
                "Fetched {}/{}",
                crawled.to_formatted_string(&config.num_format),
                total.to_formatted_string(&config.num_format)
            ));
            left_col = left_col.push(progress_text);

            let progress = progress_bar(0.0..=total as f32, crawled as f32)
                .height(Length::Fixed(10.0));
            left_col = left_col.push(progress);

            let thread_num =
                crate::ui::num_field(*threads, config.max_threads, move |nv| {
                    Message::CrawlerSetThreads {
                        server: sid,
                        new_count: nv,
                    }
                });
            let thread_num =
                row!(text(t("Threads: ")), horizontal_space(), thread_num)
                    .align_items(Alignment::Center);
            left_col = left_col.push(thread_num);
            let order_picker = pick_list(
                [
                    CrawlingOrder::Random,
                    CrawlingOrder::TopDown,
                    CrawlingOrder::BottomUp,
                ],
                Some(lock.order),
                |nv| Message::OrderChange {
                    server: server.ident.id,
                    new: nv,
                },
            );
            left_col = left_col.push(
                row!(
                    text(t("Crawling Order:")).width(Length::FillPortion(1)),
                    order_picker.width(Length::FillPortion(1))
                )
                .align_items(Alignment::Center),
            );

            if config.show_crawling_restrict
                || !lock.lvl_skipped_accounts.is_empty()
            {
                let old_max = lock.max_level;
                let old_min = lock.min_level;

                let set_min_lvl =
                    crate::ui::num_field(lock.min_level, 9999u32, move |nv| {
                        Message::CrawlerSetMinMax {
                            server: sid,
                            min: nv,
                            max: old_max,
                        }
                    });
                let thread_num =
                    row!(text(t("Min Lvl: ")), horizontal_space(), set_min_lvl)
                        .align_items(Alignment::Center);
                left_col = left_col.push(thread_num);

                let set_min_lvl =
                    crate::ui::num_field(lock.max_level, 9999u32, move |nv| {
                        Message::CrawlerSetMinMax {
                            server: sid,
                            min: old_min,
                            max: nv,
                        }
                    });
                let thread_num =
                    row!(text(t("Max Lvl: ")), horizontal_space(), set_min_lvl)
                        .align_items(Alignment::Center);
                left_col = left_col.push(thread_num);
            }

            let clear = button(t("Clear HoF")).on_press(Message::ClearHof(sid));
            let save = button(t("Save HoF")).on_press(Message::SaveHoF(sid));
            left_col = left_col.push(
                column!(row!(clear, save).spacing(10))
                    .align_items(Alignment::Center),
            );

            drop(lock);
        }
        CrawlingStatus::Waiting => {
            left_col = left_col.push(text(t("Waiting for Player...")));
        }
        CrawlingStatus::Restoring => {
            left_col = left_col.push(text(t("Loading Server Data...")));
        }
        CrawlingStatus::CrawlingFailed(reason) => {
            // TODO: Maybe display this?
            _ = reason;
            left_col = left_col.push(text(t("Crawling Failed")));
        }
    }

    left_col.into()
}

impl Helper {
    /// A full-width strip under the top bar that says what mode the bot is
    /// in, so nobody wonders why nothing happens: paused, sessions (waiting
    /// or running), full power. Human pace shows nothing
    fn mode_banner(&self) -> Element<'_, Message> {
        if self.preview {
            return container(style::on_plate(
                t("Offline preview — sample data, no game connection"),
                style::ACCENT,
                12.0,
            ))
            .padding([6, 10])
            .width(Length::Fill)
            .center_x()
            .style(style::solid(style::SIDE_PLATE_ON))
            .into();
        }
        use crate::pace::Pace;
        let now = Local::now();
        // A waiting update outranks everything else on the banner - it is
        // the one thing that explains a restart that is about to happen
        // A level up outranks everything for half a minute. It is the one
        // thing in this game that only happens now and then, and the app
        // watched it go past in silence
        if let View::Account { ident, .. } = &self.current_view
            && let Some((_, player)) = self.servers.get_ident(ident)
            && let Some((lvl, at)) = player.level_up
            && (Local::now() - at).num_seconds() < 30
        {
            return container(
                // Just the fact and the number - the guild leader's word
                style::on_plate(
                    format!("{} {}", t("LEVEL UP"), lvl),
                    Color::WHITE,
                    15.0,
                ),
            )
            .width(Length::Fill)
            .center_x()
            .padding([6, 10])
            .style(style::solid(Color::from_rgb(0.45, 0.30, 0.62)))
            .into();
        }
        if let Some(status) = &self.update_status
            && self.update_ready.is_some()
        {
            return container(
                row!(
                    horizontal_space(),
                    style::on_plate(
                        format!("{status} - {}", t("takes over in a moment")),
                        Color::WHITE,
                        13.0,
                    ),
                    button(text(t("Restart now")).size(12))
                        .padding([3, 10])
                        .style(crate::ui::style::primary())
                        .on_press(Message::UpdateNow),
                    horizontal_space(),
                )
                .spacing(12)
                .align_items(Alignment::Center),
            )
            .width(Length::Fill)
            .padding([4, 10])
            .style(style::solid(Color::from_rgb(0.20, 0.42, 0.30)))
            .into();
        }
        // A licence that has run out outranks every other thing the strip
        // could be saying: nothing is going out, and the reason is not one
        // the player can fix from the top bar
        let (label, bg) = if !self.licence.may_run() {
            (
                match &self.licence {
                    crate::licence::Status::WrongMachine { .. } => {
                        t("This licence was issued for another computer. Open \
                         Settings, send the code shown there, and paste the \
                         new licence in.")
                    }
                    crate::licence::Status::Missing => {
                        t("No licence. The app will not play until one is \
                         pasted in under Settings.")
                    }
                    _ => t("The licence has run out. The app will not play \
                         until it is renewed - see Settings."),
                }
                .to_string(),
                Color::from_rgb(0.55, 0.20, 0.20),
            )
        } else if self.paused {
            (
                t("STOPPED - the app is not talking to the game. Go play in the \
                 browser; press RESUME in the top left when you are done.")
                    .to_string(),
                Color::from_rgb(0.55, 0.20, 0.20),
            )
        } else if self.boosting() {
            // Before the quiet hours on purpose: "do it now" goes through
            // them, so a strip saying "nothing goes out" while the queues
            // are working would be the app contradicting itself. And
            // before the sessions, which have stood down - the strip used
            // to fall through to "SESSIONS MODE · 3-120 min · idle gap
            // ~3 h", which describes a rhythm that is not running
            (
                format!(
                    "{} \u{00B7} {} {} \u{00B7} {}",
                    t("GOING THROUGH EVERYTHING"),
                    self.jobs_waiting(),
                    t("jobs left"),
                    t("ends when the round is done; the slider takes over \
                       again then"),
                ),
                Color::from_rgb(0.18, 0.42, 0.25),
            )
        } else if let Some(until) = self.quiet_until() {
            (
                format!(
                    "{} ~{}",
                    t("QUIET HOURS - nothing goes out until"),
                    until.format("%H:%M")
                ),
                Color::from_rgb(0.25, 0.25, 0.40),
            )
        } else {
            use crate::config::SessionsLevel;
            match (self.config.sessions_level, self.config.pace) {
                // Full power farms through sessions and quiet hours alike
                (_, Pace::FullPower) => (
                    t("FULL POWER - no pauses, no sessions, no quiet hours")
                        .to_string(),
                    Color::from_rgb(0.20, 0.32, 0.50),
                ),
                (level, _) if level != SessionsLevel::Off => {
                    // Say the whole pattern, not just the next step - the
                    // point of sessions mode is that it keeps going all day
                    // The stretch away is not a fixed range any more: the
                    // work waiting shortens it (see `session_gap_secs`), so
                    // the strip says what is waiting and lets the countdown
                    // above speak for the rest
                    let plan = format!(
                        "3-120 min · {} % {} · {} ~{} h",
                        self.need_now(),
                        t("to do"),
                        t("idle gap"),
                        self.config.burst_hours,
                    );
                    match (self.burst_until, self.burst_next) {
                        (Some(until), _) if now < until => (
                            format!(
                                "{} {} min · {plan}",
                                t("SESSION RUNNING - ends in"),
                                (until - now).num_minutes()
                            ),
                            Color::from_rgb(0.18, 0.42, 0.25),
                        ),
                        (_, Some(next)) => {
                            let s = (next - now).num_seconds().max(0);
                            (
                                format!(
                                    "{} {}h {:02}m · {plan}",
                                    t("SESSIONS MODE - next session in"),
                                    s / 3600,
                                    (s % 3600) / 60,
                                ),
                                Color::from_rgb(0.50, 0.38, 0.12),
                            )
                        }
                        _ => (
                            format!("{} · {plan}", t("SESSIONS MODE")),
                            Color::from_rgb(0.50, 0.38, 0.12),
                        ),
                    }
                }
                _ => return column!().into(),
            }
        };
        // The strip is a block of colour, so its lettering is set for one:
        // white letters with the artwork's dark outline still round them
        // came out muddy and ragged against the red and the green
        container(style::on_plate(label, Color::WHITE, 13.0))
            .width(Length::Fill)
            .padding([4, 10])
            .center_x()
            .style(style::solid(bg))
            .into()
    }

    /// Inside the quiet hours right now? Then when they end - read off the
    /// shift already drawn (the queue draws it; the view only reads)
    fn attendance_line(&self) -> String {
        use crate::attendance::Duty;
        let portal = self.attendance.total(Duty::Portal);
        let defense = self.attendance.total(Duty::Defense);
        let attack = self.attendance.total(Duty::Attack);
        format!(
            "Guild attendance over ~{} decisions: {}/{} portal, {}/{} defense, {}/{} attack ({})",
            crate::attendance::WINDOW,
            portal.0,
            portal.1,
            defense.0,
            defense.1,
            attack.0,
            attack.1,
            t("all characters")
        )
    }

    fn quiet_until(&self) -> Option<chrono::NaiveTime> {
        if !self.config.quiet_hours
            || self.config.pace == crate::pace::Pace::FullPower
        {
            return None;
        }
        let now = Local::now();
        let shift = match self.quiet_shift {
            Some((d, s)) if d == now.date_naive() => s,
            _ => 0,
        };
        if !crate::stealth::in_quiet(
            now.time(),
            self.config.quiet_from,
            self.config.quiet_to,
            shift,
            0,
        ) {
            return None;
        }
        let end = (i64::from(self.config.quiet_to.min(24)) * 60 + shift)
            .rem_euclid(24 * 60);
        chrono::NaiveTime::from_hms_opt((end / 60) as u32, (end % 60) as u32, 0)
    }

    /// Why a cooldown that is up is going uncollected right now, if it
    /// is - a short tail for the plates down the side.
    ///
    /// `None` means something is going to take it: a session is open, or
    /// "do it now" is running. Otherwise it is the wait itself, so that a
    /// free arena fight does not sit there in green while the app is two
    /// hours from looking at it. The arena's own cooldown is ten minutes,
    /// so "free" with nothing behind it is the one thing on this screen
    /// that can quietly cost a whole evening's fights
    fn idle_tail(&self) -> Option<String> {
        if self.paused {
            return Some(t("STOP").to_string());
        }
        if self.config.off() {
            return Some(t("off").to_string());
        }
        if self.boosting() {
            return None;
        }
        if self.burst_until.is_some_and(|u| Local::now() < u) {
            return None;
        }
        if let Some(until) = self.quiet_until() {
            return Some(format!("{} {}", t("night"), until.format("%H:%M")));
        }
        Some(match self.burst_next {
            Some(next) => countdown((next - Local::now()).num_seconds()),
            None => t("waiting").to_string(),
        })
    }

    /// Burst mode: "session open, 12 min left" or "next session in 3h 40m"
    /// plus a "now" button. Empty in the other modes
    fn burst_status(&self) -> Element<'_, Message> {
        use crate::config::SessionsLevel;
        // "Do it now": full speed for a few minutes without touching the
        // slider. Sitting and watching used to mean either waiting out
        // the human pauses or moving to FULL POWER, which is a settings
        // change that throws away the sessions and the quiet night for
        // good. This is the same thing, borrowed and given back
        // What the button says while it runs is what is left to do, not a
        // clock: the run ends when the round is done, and a countdown
        // that ran on with nothing left - or ran out with plenty left -
        // was the button and the feed telling two stories
        let boost: Element<Message> = if self.boosting() {
            let waiting = self.jobs_waiting();
            widget::tooltip(
                button(
                    text(format!(
                        "{} \u{00B7} {} {}",
                        t("GOING THROUGH IT"),
                        waiting,
                        t("left")
                    ))
                    .size(11),
                )
                .padding([3, 6])
                .style(crate::ui::style::primary())
                .on_press(Message::BoostToggle),
                text(t("Click to stop. Nothing else stops it - the slider \
                        can be moved meanwhile and only says what happens \
                        afterwards"))
                .size(11),
                widget::tooltip::Position::Bottom,
            )
            .gap(4)
            .style(theme::Container::Box)
            .into()
        } else {
            widget::tooltip(
                button(text(t("Do it now")).size(11))
                    .padding([3, 6])
                    .style(crate::ui::style::quiet())
                    .on_press(Message::BoostToggle),
                text(t(
                    "One round, now: everything that is due goes into the \
                     queues and is worked through with a few seconds between \
                     clicks, whatever the slider and the quiet hours say. It \
                     ends by itself when the round is done. The slider is \
                     not touched - it stays where it is and says how the app \
                     plays afterwards; only this button stops a run early. \
                     Nothing is saved",
                ))
                .size(11),
                widget::tooltip::Position::Bottom,
            )
            .gap(4)
            .style(theme::Container::Box)
            .into()
        };
        // No "next session in ..." while it runs: there is no next
        // session drawn at all until the quarter of an hour is up, and a
        // countdown ticking next to it was the app claiming to be on a
        // schedule it had just stood down
        if self.boosting()
            || self.config.sessions_level == SessionsLevel::Off
            || self.config.pace == crate::pace::Pace::FullPower
        {
            return boost;
        }
        let now = Local::now();
        let label = match (self.burst_until, self.burst_next) {
            (Some(until), _) if now < until => {
                let m = (until - now).num_minutes();
                format!("{} {m} min", t("session open:"))
            }
            (_, Some(next)) => {
                let s = (next - now).num_seconds().max(0);
                format!(
                    "{} {}h {:02}m",
                    t("next session in"),
                    s / 3600,
                    (s % 3600) / 60
                )
            }
            _ => String::new(),
        };
        row!(
            text(label).size(11),
            button(text(t("now")).size(11))
                .padding([3, 6])
                .style(crate::ui::style::quiet())
                .on_press(Message::BurstNow),
            boost,
        )
        .spacing(4)
        .align_items(Alignment::Center)
        .into()
    }

    /// The right-hand column: the good news, what the bot is doing right
    /// now, and the guild chat. The live feed was moved out to the Activity
    /// log once because it repeated every win the successes window already
    /// showed - but without it the column looked like nothing was running.
    /// It is back, minus the lines the successes window has (see
    /// `ConsoleLine::dup`); the Activity log still shows everything
    fn right_panel<'a>(
        &'a self,
        player: &'a AccountInfo,
    ) -> Element<'a, Message> {
        // Rewards waiting in the inbox - a Twitch drop, a coupon, a delivery.
        // Only while there is one: the line is news, not furniture. The
        // dailies step claims them within minutes, so it is usually short
        // lived - the Twitch button stays useful for the stream itself
        let waiting: Vec<(String, bool)> = match &*player.status.lock_safe() {
            AccountStatus::Idle(_, gs) | AccountStatus::Busy(gs, _) => {
                use sf_api::gamestate::social::{
                    ClaimableMailType, ClaimableStatus,
                };
                let now = Local::now();
                gs.mail
                    .claimables
                    .iter()
                    .filter(|c| {
                        c.status != ClaimableStatus::Claimed
                            && c.claimable_until.is_none_or(|t| t > now)
                    })
                    .map(|c| {
                        (c.name.clone(), c.typ == ClaimableMailType::TwitchDrop)
                    })
                    .collect()
            }
            _ => Vec::new(),
        };
        let rewards: Element<Message> = if waiting.is_empty() {
            column!().into()
        } else {
            let twitch = waiting.iter().any(|(_, t)| *t);
            let names: Vec<&str> =
                waiting.iter().map(|(n, _)| n.as_str()).collect();
            let mut r = column!(
                text(format!(
                    "{} {}",
                    if twitch {
                        t("Twitch drop in the inbox:")
                    } else {
                        t("Reward in the inbox:")
                    },
                    names.join(", ")
                ))
                .size(11)
                .style(theme::Text::Color(Color::from_rgb(0.36, 0.78, 0.45))),
            )
            .spacing(3);
            if twitch {
                r = r.push(
                    button(text(t("Open Twitch")).size(11))
                        .padding([2, 8])
                        .style(crate::ui::style::primary())
                        .on_press(Message::OpenLink(
                            "https://www.twitch.tv/directory/category/shakes-fidget"
                                .to_string(),
                        )),
                );
            }
            container(r)
                .padding([8, 8, 0, 8])
                .width(Length::Fill)
                .into()
        };
        container(
            column!(
                rewards,
                container(self.successes_feed(&player.name))
                    .height(Length::FillPortion(2))
                    .padding([10, 8, 4, 8]),
                container(self.live_feed(&player.name, true))
                    .height(Length::FillPortion(3))
                    .padding([0, 8, 4, 8]),
                container(guild_chat_panel(player))
                    .height(Length::FillPortion(2))
                    .padding([0, 8, 8, 8]),
            )
            .spacing(6)
            .width(Length::Fixed(300.0))
            .height(Length::Fill),
        )
        .height(Length::Fill)
        .style(style::frame())
        .into()
    }

    /// The newest console lines, trimmed to fit a narrow column
    /// Which lines a feed shows: the open character's own, or everybody's
    fn feed_shows(&self, who: &str, line_who: &str) -> bool {
        // Lines about the app itself (an update, a burst session) belong to
        // no character and were filtered out of every per-character feed -
        // so the update note was never seen
        self.feed_all
            || line_who.is_empty()
            || line_who.to_lowercase() == who.to_lowercase()
    }

    /// The "this character / everybody" switch shared by both feeds
    fn feed_switch(&self) -> Element<'_, Message> {
        button(
            text(if self.feed_all {
                t("everybody")
            } else {
                t("this character")
            })
            .size(10),
        )
        .padding([1, 5])
        .style(crate::ui::style::quiet())
        .on_press(Message::ToggleFeedAll)
        .into()
    }

    /// `skip_successes` leaves out the lines the successes window already
    /// carries - for the right column, where the two sit on top of each
    /// other; the Activity log passes false and shows everything
    fn live_feed<'a>(
        &'a self,
        who: &'a str,
        skip_successes: bool,
    ) -> Element<'a, Message> {
        // The feed's own three, taken from the app's palette rather than
        // from the mint and pink they used to be - those were picked
        // against a grey app and fight the gold
        let green = style::GOOD;
        let red = style::BAD;
        let grey = Color::from_rgb(0.75, 0.78, 0.82);
        // Newest first. At the bottom it was always just off the edge of a
        // column this narrow, so the one line worth reading needed a scroll
        let mut lines = column!().spacing(12);
        let mut shown = 0;
        for l in self
            .console
            .iter()
            .rev()
            .filter(|l| self.feed_shows(who, &l.who))
            .filter(|l| !(skip_successes && l.dup))
            .take(120)
        {
            shown += 1;
            // The bot's own thinking in the neon's purple, so it reads
            // apart from what happened
            let color = match l.ok {
                Some(true) => green,
                Some(false) => red,
                None if l.text.starts_with("Thinking:") => style::NEON,
                None => grey,
            };
            // The time in the feed's green, the character in the card's
            // gold - the grey pair was the one thing on the line that did
            // not read
            lines = lines.push(
                column!(
                    row!(
                        style::bold(
                            l.at.format("%H:%M:%S").to_string(),
                            green,
                            10.0
                        ),
                        style::bold(&l.who, style::NAME_GOLD, 10.0),
                    )
                    .spacing(6),
                    widget::text(&l.text)
                        .size(12)
                        .style(theme::Text::Color(color)),
                )
                .spacing(1),
            );
        }
        if shown == 0 {
            lines = lines.push(style::bold(
                t("Nothing has happened yet"),
                style::MUTED,
                11.0,
            ));
        }
        style::panel_bar(
            t("Live"),
            row!(
                self.feed_switch(),
                button(text(t("Clear")).size(10))
                    .padding([1, 5])
                    .style(crate::ui::style::quiet())
                    .on_press(Message::ClearConsole),
            )
            .spacing(4)
            .align_items(Alignment::Center),
            scrollable(lines).height(Length::Fill).width(Length::Fill),
        )
        .height(Length::Fill)
        .into()
    }

    /// Only the good news: what was gained, won, bought or upgraded, every
    /// character, newest first. The console has all of it too, but between
    /// refreshes and refusals - this is the window that answers "what did I
    /// actually get today"
    fn successes_feed<'a>(&'a self, who: &'a str) -> Element<'a, Message> {
        let green = style::GOOD;
        let mut lines = column!().spacing(12);
        let mut shown = 0;
        for l in self
            .successes
            .iter()
            .rev()
            .filter(|l| self.feed_shows(who, &l.who))
            .take(80)
        {
            shown += 1;
            lines = lines.push(
                column!(
                    row!(
                        style::bold(
                            l.at.format("%H:%M:%S").to_string(),
                            green,
                            10.0
                        ),
                        style::bold(&l.who, style::NAME_GOLD, 10.0),
                    )
                    .spacing(6),
                    widget::text(&l.text)
                        .size(12)
                        .style(theme::Text::Color(style::GOLD)),
                )
                .spacing(1),
            );
        }
        if shown == 0 {
            lines = lines.push(style::bold(
                t("Nothing gained yet"),
                style::MUTED,
                11.0,
            ));
        }
        style::panel_bar(
            format!("{} ({shown})", t("Successes")),
            row!(
                self.feed_switch(),
                button(text(t("Clear")).size(10))
                    .padding([1, 5])
                    .style(crate::ui::style::quiet())
                    .on_press(Message::ClearSuccesses),
            )
            .spacing(4)
            .align_items(Alignment::Center),
            scrollable(lines).height(Length::Fill).width(Length::Fill),
        )
        .height(Length::Fill)
        .into()
    }

    /// "Is there an update?" - and its own answer. A check that finds
    /// nothing used to be silent, so pressing the button looked like
    /// pressing a dead one; the label carries the result instead of needing
    /// a line of its own somewhere
    fn update_button(&self) -> Element<'_, Message> {
        let (label, style) = match (&self.update_ready, &self.update_status) {
            (Some(_), _) => {
                (t("Update ready").to_string(), crate::ui::style::primary())
            }
            (None, Some(s)) if s.starts_with("Downloading") => {
                // Keep the version the status carries ("Downloading 1.55...")
                (s.clone(), crate::ui::style::primary())
            }
            (None, Some(s)) if s.starts_with("Asking") => {
                (t("Checking...").to_string(), crate::ui::style::quiet())
            }
            (None, Some(s)) if s.starts_with("There's no new update") => {
                (s.clone(), crate::ui::style::quiet())
            }
            (None, Some(_)) => {
                (t("Check failed").to_string(), crate::ui::style::danger())
            }
            (None, None) => (
                t("Is there an update?").to_string(),
                crate::ui::style::quiet(),
            ),
        };
        button(text(label).size(12))
            .padding([4, 8])
            .style(style)
            .on_press(Message::UpdateCheck)
            .into()
    }

    /// The console: every action of every character, newest at the bottom
    pub fn view_console(&self) -> Element<'_, Message> {
        // The feed's own three, taken from the app's palette rather than
        // from the mint and pink they used to be - those were picked
        // against a grey app and fight the gold
        let green = style::GOOD;
        let red = style::BAD;
        let grey = Color::from_rgb(0.75, 0.78, 0.82);
        let mut lines = column!().spacing(1).padding([4, 8]);
        let skip = self.console.len().saturating_sub(400);
        for l in self.console.iter().skip(skip) {
            let color = match l.ok {
                Some(true) => green,
                Some(false) => red,
                None if l.text.starts_with("Thinking:") => style::NEON,
                None => grey,
            };
            lines = lines.push(
                row!(
                    style::bold_mono(
                        l.at.format("%H:%M:%S").to_string(),
                        green,
                        11.0,
                        Length::Fixed(64.0)
                    ),
                    style::bold_mono(
                        &l.who,
                        style::NAME_GOLD,
                        11.0,
                        Length::Fixed(120.0)
                    ),
                    style::bold_mono(&l.text, color, 11.0, Length::Shrink),
                )
                .spacing(8),
            );
        }
        let head = row!(
            text(format!("{} ({})", t("Console"), self.console.len())).size(12),
            horizontal_space(),
            button(text(t("Clear")).size(11))
                .padding([2, 6])
                .style(crate::ui::style::quiet())
                .on_press(Message::ClearConsole),
            button(text("\u{2715}").size(11))
                .padding([2, 6])
                .style(crate::ui::style::quiet())
                .on_press(Message::ToggleConsole),
        )
        .spacing(6)
        .padding([2, 8])
        .align_items(Alignment::Center);
        container(
            column!(
                head,
                scrollable(lines)
                    .height(Length::Fixed(200.0))
                    .width(Length::Fill)
                    .id(CONSOLE_SCROLL.clone()),
            )
            .width(Length::Fill),
        )
        .width(Length::Fill)
        .style(style::frame())
        .into()
    }
}

/// Scrollable id, so the console can be kept at the bottom
static CONSOLE_SCROLL: std::sync::LazyLock<widget::scrollable::Id> =
    std::sync::LazyLock::new(widget::scrollable::Id::unique);

pub fn console_snap_to_end() -> iced::Command<Message> {
    widget::scrollable::snap_to(
        CONSOLE_SCROLL.clone(),
        widget::scrollable::RelativeOffset::END,
    )
}

/// The guild chat, kept in the corner so it is readable without leaving the
/// page you are on. The Guild tab still has the full one with the member
/// list; this is the running conversation and a box to answer in
fn guild_chat_panel(player: &AccountInfo) -> Element<'_, Message> {
    let ident = player.ident;
    let lock = player.status.lock_safe();
    let guild = match &*lock {
        AccountStatus::Idle(_, gs) | AccountStatus::Busy(gs, _) => {
            gs.guild.as_ref()
        }
        _ => None,
    };

    let mut lines = column!().spacing(3);
    match guild {
        None => {
            lines = lines.push(style::bold(
                t("Not in a guild"),
                style::MUTED,
                11.0,
            ));
        }
        Some(guild) if guild.chat.is_empty() => {
            lines = lines.push(style::bold(
                t("No messages yet"),
                style::MUTED,
                11.0,
            ));
        }
        Some(guild) => {
            // The server hands the newest message first; a chat reads the
            // other way round - history above, what was just said at the
            // bottom. The scrollable below is pinned to the end, so the
            // newest line is the one you are looking at
            for msg in guild.chat.iter().take(60).rev() {
                // The game's own lines - a level up, a hit on the portal,
                // a promotion - have no author and no time
                let stamp = msg
                    .time
                    .map(|t| t.format("%H:%M").to_string())
                    .unwrap_or_default();
                // The same decoding as the Guild tab's own chat. This
                // panel is a second renderer of the same messages and it
                // was left printing the raw `#lu#Name#232` while the tab
                // next to it showed the sentence - one fix, two places
                let body = crate::chat::decode(&msg.message)
                    .unwrap_or_else(|| msg.message.clone());
                let line = if msg.user.is_empty() {
                    body.clone()
                } else {
                    format!("{stamp} {}: {body}", msg.user)
                };
                lines = lines.push(
                    button(
                        column!(
                            row!(
                                style::bold(stamp, style::MUTED, 10.0),
                                style::bold(&msg.user, style::ACCENT, 10.0),
                            )
                            .spacing(6),
                            style::bold(
                                &body,
                                if msg.user.is_empty() {
                                    style::MUTED
                                } else {
                                    iced::Color::WHITE
                                },
                                11.0
                            ),
                        )
                        .spacing(1)
                        .width(Length::Fill),
                    )
                    .padding(2)
                    .width(Length::Fill)
                    .style(crate::ui::style::quiet())
                    .on_press(Message::GuildChatCopy { ident, text: line }),
                );
            }
        }
    }
    drop(lock);

    style::panel(
        t("Guild chat"),
        column!(
            scrollable(lines)
                .height(Length::Fill)
                .width(Length::Fill)
                .direction(widget::scrollable::Direction::Vertical(
                    widget::scrollable::Properties::new()
                        .alignment(widget::scrollable::Alignment::End),
                )),
            widget::text_input(
                t("Write to the guild..."),
                &player.social.chat_input
            )
            .font(crate::ui::style::FAT)
            .on_input(move |text| Message::GuildChatInput { ident, text })
            .on_submit(Message::GuildChatSend { ident })
            .size(12)
            .padding(4),
        )
        .spacing(5)
        .height(Length::Fill),
    )
    .height(Length::Fill)
    .into()
}

/// Silver from the game shown as gold: "1.23M", "45.6K", "812"
/// A number field you can type into. The spinner it replaces could not be
/// typed into and took a click per unit - "2400 clicks to set a mushroom
/// reserve". Digits only; an emptied field reads as zero, anything over
/// `max` is `max`, and every keystroke is a message, so what is typed is
/// what is saved
pub fn num_field<'a, T>(
    value: T,
    max: T,
    on: impl Fn(T) -> Message + 'a,
) -> widget::TextInput<'a, Message>
where
    T: Copy + std::fmt::Display + std::str::FromStr + PartialOrd + 'a,
{
    let shown = value.to_string();
    widget::text_input("", &shown)
        .font(crate::ui::style::FAT)
        .on_input(move |s| {
            let digits: String =
                s.chars().filter(char::is_ascii_digit).collect();
            let digits = if digits.is_empty() {
                "0".to_string()
            } else {
                digits
            };
            let n = digits.parse::<T>().unwrap_or(value);
            on(if n > max { max } else { n })
        })
        .width(Length::Fixed(64.0))
        .size(12)
        .padding(4)
}

/// The number in full with a space every three digits, the way the game
/// writes gold: 333 000 000
/// A number short enough for a plate in the side column.
///
/// The plates are 210 pixels wide and most of that is the word. "1 228 999"
/// is nine characters plus the spaces and it ran off the end - and a figure
/// you have to read digit by digit was never what a plate is for. Three
/// significant figures and a suffix says the same thing at a glance and
/// always fits
#[must_use]
pub fn compact(n: impl Into<u128>) -> String {
    let n: u128 = n.into();
    #[allow(clippy::cast_precision_loss)]
    let v = n as f64;
    if v < 1e3 {
        return n.to_string();
    }
    // Smallest suffix first, and the first one that leaves fewer than three
    // whole digits wins - so 999 999 becomes 1.00M rather than rounding up
    // into a four-digit "1000k"
    for (limit, suffix) in [
        (1e3, "k"),
        (1e6, "M"),
        (1e9, "G"),
        (1e12, "T"),
        (1e15, "P"),
        (1e18, "E"),
        (1e21, "Z"),
        (1e24, "Y"),
    ] {
        let scaled = v / limit;
        if scaled < 999.5 {
            return if scaled < 10.0 {
                format!("{scaled:.2}{suffix}")
            } else if scaled < 100.0 {
                format!("{scaled:.1}{suffix}")
            } else {
                format!("{scaled:.0}{suffix}")
            };
        }
    }
    format!("{v:.2e}")
}

pub fn thousands(n: impl Into<u128>) -> String {
    let n: u128 = n.into();
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(' ');
        }
        out.push(ch);
    }
    out
}

pub fn gold_str(silver: u64) -> String {
    let gold = silver as f64 / 100.0;
    if gold >= 1e6 {
        format!("{:.2}M", gold / 1e6)
    } else if gold >= 1e3 {
        format!("{:.1}K", gold / 1e3)
    } else {
        format!("{gold:.0}")
    }
}

/// Every automation's log in one place, newest first
impl Helper {
    /// The Activity log tab: the live console of everything the bot sends
    /// (every character) on the left, this character's own module logs on
    /// the right
    fn view_log<'a>(&'a self, player: &'a AccountInfo) -> Element<'a, Message> {
        row!(
            container(self.live_feed(&player.name, false))
                .width(Length::FillPortion(1))
                .height(Length::Fill)
                .padding(12)
                .style(style::card()),
            container(view_module_log(player))
                .width(Length::FillPortion(1))
                .height(Length::Fill)
                .padding(12)
                .style(style::card()),
        )
        .spacing(12)
        .padding(18)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }
}

/// This character's own doings, every module merged and newest first
fn view_module_log(player: &AccountInfo) -> Element<'_, Message> {
    let mut all: Vec<(DateTime<Local>, &'static str, String, bool)> =
        Vec::new();
    let mut take =
        |src: &'static str, log: &[(DateTime<Local>, String, bool)]| {
            for (t, m, ok) in log.iter().rev().take(80) {
                all.push((*t, src, m.clone(), *ok));
            }
        };
    if let Some(fi) = &player.fortress_info {
        take("Fortress", &fi.log);
    }
    take("Dungeons", &player.dungeon_info.log);
    take("Pets", &player.pet_info.log);
    take("Arena Manager", &player.idle_info.log);
    take("Expeditions", &player.expedition_info.log);
    take("City guard", &player.guard_info.log);
    take("Wheel", &player.tavern_info.wheel_log);
    take("Shops", &player.shop_info.log);
    if let Some(si) = &player.scrapbook_info {
        for (t, target, won) in si.attack_log.iter().rev().take(80) {
            all.push((
                *t,
                "Arena",
                format!(
                    "{} {} ({} {})",
                    if *won { "Won against" } else { "Lost against" },
                    target.info.name,
                    target.missing,
                    "new items"
                ),
                *won,
            ));
        }
    }
    all.sort_by_key(|a| std::cmp::Reverse(a.0));
    all.truncate(300);

    let mut col = column!().spacing(3).padding(5);
    if all.is_empty() {
        col = col.push(text(t("Nothing happened yet")).size(12));
    }
    for (time, src, msg, ok) in all {
        col = col.push(
            row!(
                text(time.format("%d.%m. %H:%M").to_string())
                    .size(11)
                    .width(Length::Fixed(90.0)),
                text(t(src)).size(11).width(Length::Fixed(100.0)),
                text(msg).size(11).style(theme::Text::Color(if ok {
                    Color::from_rgb(0.36, 0.78, 0.45)
                } else {
                    Color::from_rgb(0.90, 0.45, 0.45)
                })),
            )
            .spacing(8),
        );
    }
    column!(
        crate::ui::style::heading(t("Activity log")),
        widget::scrollable(col).height(Length::Fill)
    )
    .spacing(8)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// Just a coffee cup. Hovering it says what it is, clicking opens the page
/// The cup, decoded once
static COFFEE: std::sync::LazyLock<widget::image::Handle> =
    std::sync::LazyLock::new(|| {
        let raw = include_bytes!("../../assets/coffee.webp");
        let img =
            image::load_from_memory_with_format(raw, image::ImageFormat::WebP)
                .expect("coffee.webp");
        widget::image::Handle::from_pixels(
            img.width(),
            img.height(),
            img.into_bytes(),
        )
    });

/// A small PNG from `assets/`, decoded once
fn png_handle(raw: &[u8], what: &str) -> widget::image::Handle {
    let img = image::load_from_memory_with_format(raw, image::ImageFormat::Png)
        .unwrap_or_else(|e| panic!("{what}: {e}"))
        .into_rgba8();
    widget::image::Handle::from_pixels(
        img.width(),
        img.height(),
        img.into_raw(),
    )
}

/// The game's coin and mushroom for the purse in the sidebar - the guild
/// leader's pictures, cut out and scaled down (see the icons script in
/// ZMENY.md)
static COIN: std::sync::LazyLock<widget::image::Handle> =
    std::sync::LazyLock::new(|| {
        png_handle(include_bytes!("../../assets/coin.png"), "coin.png")
    });
static MUSHROOM: std::sync::LazyLock<widget::image::Handle> =
    std::sync::LazyLock::new(|| {
        png_handle(include_bytes!("../../assets/mushroom.png"), "mushroom.png")
    });
static HOURGLASS: std::sync::LazyLock<widget::image::Handle> =
    std::sync::LazyLock::new(|| {
        png_handle(
            include_bytes!("../../assets/hourglass.png"),
            "hourglass.png",
        )
    });
/// The clover coin of the wheel of fortune
static LUCKY: std::sync::LazyLock<widget::image::Handle> =
    std::sync::LazyLock::new(|| {
        png_handle(include_bytes!("../../assets/lucky.png"), "lucky.png")
    });

/// The four pictures the guild leader drew for the purse, so that a number
/// box anywhere in the app can carry the same one the purse does - a
/// mushroom count with the purse's mushroom beside it needs no caption at
/// all, and reads at a glance where a word does not
pub fn coin_pic() -> widget::image::Handle {
    COIN.clone()
}
pub fn mushroom_pic() -> widget::image::Handle {
    MUSHROOM.clone()
}
pub fn hourglass_pic() -> widget::image::Handle {
    HOURGLASS.clone()
}
pub fn lucky_pic() -> widget::image::Handle {
    LUCKY.clone()
}

pub fn coffee_icon() -> Element<'static, Message> {
    widget::tooltip(
        button(widget::image(COFFEE.clone()).width(16).height(16))
            .padding(2)
            .style(theme::Button::Text)
            .on_press(Message::OpenLink(crate::COFFEE_URL.to_string())),
        "Buy me a coffee",
        widget::tooltip::Position::Top,
    )
    .gap(4)
    .style(theme::Container::Box)
    .into()
}

/// A small "what is this character doing right now" indicator. Without it the
/// automatic tabs look identical whether something is running or not
/// Everything that runs on its own for this character, one label each
pub fn running_jobs(player: &AccountInfo) -> Vec<&'static str> {
    let mut jobs: Vec<&str> = Vec::new();
    if player
        .scrapbook_info
        .as_ref()
        .is_some_and(|a| a.auto_battle)
    {
        jobs.push(t("Arena"));
    }
    if player.dungeon_info.auto {
        jobs.push(t("Dungeons"));
    }
    if player.expedition_info.auto {
        jobs.push(t("Expeditions"));
    }
    if let Some(fi) = &player.fortress_info {
        if fi.auto_attack {
            jobs.push(t("Fortress attacks"));
        }
        if fi.auto_collect || fi.auto_gems {
            jobs.push(t("Fortress collect / gems"));
        }
        if fi.auto_build {
            jobs.push(t("Fortress building"));
        }
        if fi.recruit_all {
            jobs.push(t("Fortress recruiting"));
        }
    }
    if player.underworld_info.as_ref().is_some_and(|a| a.auto_lure) {
        jobs.push(t("Underworld lure"));
    }
    if player.social.auto_attack || player.social.auto_defense {
        jobs.push(t("Guild sign-up"));
    }
    if player.pet_info.auto_feed {
        jobs.push(t("Pet feeding"));
    }
    if player.idle_info.auto_upgrade {
        jobs.push(t("Arena Manager"));
    }
    if player.guard_info.auto {
        jobs.push(t("City guard"));
    }
    if player.tavern_info.wheel_auto {
        jobs.push(t("Wheel of fortune"));
    }
    if player.tavern_info.beer_auto {
        jobs.push(t("Beer"));
    }
    if player.shop_info.auto {
        jobs.push(t("Shopping"));
    }
    if player.shop_info.attr_auto {
        jobs.push(t("Attributes"));
    }

    jobs
}

/// The one number each page is waiting for, for the plates down the side.
///
/// A countdown when something is on a clock, a plain figure when it is
/// not - the same idea the game has on its own tabs, where the dungeon
/// door carries "51:27" and nothing else. Gold while it runs, green the
/// moment it is up, and red for the underworld's build so the two
/// building sites are never mistaken for each other at a glance.
/// One plate's number, and optionally a second line under it
#[derive(Clone)]
pub struct Badge {
    pub text: String,
    pub color: Color,
    pub sub: Option<(String, Color)>,
}

impl Badge {
    fn new(text: String, color: Color) -> Self {
        Self {
            text,
            color,
            sub: None,
        }
    }

    fn with(mut self, sub: Option<(String, Color)>) -> Self {
        self.sub = sub;
        self
    }
}

#[derive(Default)]
pub struct PageBadges {
    pub tavern: Option<Badge>,
    pub arena: Option<Badge>,
    pub guard: Option<Badge>,
    pub idle: Option<Badge>,
    pub dungeon: Option<Badge>,
    pub fortress: Option<Badge>,
    pub underworld: Option<Badge>,
    pub pets: Option<Badge>,
    pub guild: Option<Badge>,
}

/// `mm:ss` under the hour - the game's own shape on a dungeon door - and
/// then `6:12 h` and `2d 21h`. The unit is not decoration: a fortress
/// build printed as "68:53" reads as sixty-eight minutes at a glance, and
/// it is nearly three days
fn countdown(secs: i64) -> String {
    let s = secs.max(0);
    if s < 3600 {
        format!("{}:{:02}", s / 60, s % 60)
    } else if s < 24 * 3600 {
        format!("{}:{:02} h", s / 3600, (s % 3600) / 60)
    } else {
        format!("{}d {}h", s / (24 * 3600), (s % (24 * 3600)) / 3600)
    }
}

/// A moment in the future as a countdown, or `None` once it has passed -
/// a plate with nothing on it is a plate with nothing to wait for
fn until(t: Option<chrono::DateTime<Local>>) -> Option<i64> {
    let secs = (t? - Local::now()).num_seconds();
    (secs > 0).then_some(secs)
}

pub fn page_badges(player: &AccountInfo, idle: Option<&str>) -> PageBadges {
    let mut b = PageBadges::default();
    let lock = player.status.lock_safe();
    let (AccountStatus::Idle(_, gs) | AccountStatus::Busy(gs, _)) = &*lock
    else {
        return b;
    };
    let gold = Color::from_rgb(0.90, 0.75, 0.30);
    let good = style::GOOD;
    let red = Color::from_rgb(0.90, 0.40, 0.40);

    // The free arena fight and the free dungeon fight: the two clocks
    // that cost real income when they run out unwatched
    // ...and when one of them is up while nothing is going to collect
    // it, the plate says so. A green "free" next to an app that is
    // between sessions reads as "all good" when what is actually
    // happening is that a fight is going under every ten minutes
    // Same shape: the figure on the big line, the word underneath. "free
    // - 42:10" on one line is two numbers' worth of text on a 190 px
    // plate, and it collided with the label
    let free_badge = |cooldown| match until(cooldown) {
        Some(s) => Badge::new(countdown(s), gold),
        None => match idle {
            Some(why) => Badge::new(why.to_string(), red)
                .with(Some((t("free").to_string(), red))),
            None => Badge::new(t("free").to_string(), good),
        },
    };
    b.arena = Some(free_badge(gs.arena.next_free_fight));
    b.dungeon = Some(free_badge(gs.dungeons.next_free_fight));

    // The tavern's thirst, in minutes of adventure left - the thing the
    // whole day's XP is made of, and it does not carry over
    // Green means there is something there and it is being taken, gold
    // means the game's own clock is running and nobody can do anything
    // about it, red means nothing is happening with something that
    // matters. The day's thirst is the whole of a character's XP, so an
    // empty one is red, not grey
    // Thirst is a bar in the game, not a stopwatch: what matters is how
    // much of the day's total is still there. "114 min" made it a duration,
    // which it is not - "114/320" is the same number saying what it is out
    // of. The ceiling is this character's own, because an eleventh beer
    // moves it (see `mechanics::thirst_ceiling_min`)
    let thirst = gs.tavern.thirst_for_adventure_sec / 60;
    b.tavern = Some(if thirst == 0 {
        Badge::new(t("spent").to_string(), red)
    } else {
        Badge::new(
            crate::mechanics::thirst_line(thirst, gs.tavern.beer_max),
            good,
        )
    });

    // A guard shift is the character standing still for hours, so it
    // belongs on the plate
    if let sf_api::gamestate::tavern::CurrentAction::CityGuard {
        busy_until,
        ..
    } = gs.tavern.current_action
    {
        b.guard = Some(match until(Some(busy_until)) {
            Some(s) => Badge::new(countdown(s), gold),
            None => Badge::new(t("done").to_string(), good),
        });
    }

    // Both building sites, and the underworld's in red: two upgrades
    // running at once is the usual state and the plates must not read
    // alike
    if let Some(f) = gs.fortress.as_ref() {
        b.fortress = match until(f.building_upgrade.finish) {
            Some(s) => Some(Badge::new(countdown(s), gold)),
            // Nothing going up while the app is meant to be building is
            // worth seeing. An empty plate says the same as a plate for a
            // page nobody switched on, and those are not the same thing
            None => player
                .fortress_info
                .as_ref()
                .is_some_and(|fi| fi.auto_build)
                .then(|| Badge::new(t("idle").to_string(), red)),
        };
    }
    if let Some(u) = gs.underworld.as_ref() {
        let ui = player.underworld_info.as_ref();
        // What the souls are being held for. This is a *second* wait, not
        // an alternative to the first: while one building is going up the
        // souls for the next one are already piling, so the plate carries
        // the running build on top and the saving under it
        // Two figures, and the plate is 190 px wide, so the word goes on
        // the small line and the number on the big one - the same shape
        // as every other plate, where the eye looks right for a figure
        // and not for a sentence
        let saving: Option<(Option<String>, &'static str, Color)> =
            ui.filter(|i| i.auto_build).map(|i| {
                match crate::mechanics::underworld_saving_for(u, i.phase) {
                    Some((_, cost)) => (
                        crate::mechanics::souls_eta_secs(u, cost)
                            .map(crate::util::long_wait),
                        t("saving"),
                        gold,
                    ),
                    // Building is on and there is nothing to save for
                    None => (None, t("idle"), red),
                }
            });
        b.underworld = match (until(u.upgrade_finish), saving) {
            // A build running and souls piling for the next one: the
            // build on top, the saving in small type under it
            (Some(s), Some((wait, word, col))) => {
                Some(Badge::new(countdown(s), red).with(Some((
                    match wait {
                        Some(w) => format!("{word} {w}"),
                        None => word.to_string(),
                    },
                    col,
                ))))
            }
            (Some(s), None) => Some(Badge::new(countdown(s), red)),
            // Nothing going up: the wait is the figure and the word
            // explains it underneath
            (None, Some((Some(wait), word, col))) => Some(
                Badge::new(wait, col)
                    .with(Some((word.to_string(), style::MUTED))),
            ),
            (None, Some((None, word, col))) => {
                Some(Badge::new(word.to_string(), col))
            }
            (None, None) => None,
        };
    }

    // Pets: how many feeds are still possible today. A pet eats three
    // fruits a day and the fruit is only good for its own habitat, so
    // "fruit in the bag" on its own says nothing - what matters is
    // whether any of it can still go into a pet
    if let Some(pets) = gs.pets.as_ref()
        && player.pet_info.auto_feed
    {
        let mut waiting = 0u32;
        for (_, habitat) in &pets.habitats {
            let mut fruit = u32::from(habitat.fruits);
            for pet in &habitat.pets {
                if fruit == 0 {
                    break;
                }
                if pet.level == 0 {
                    continue;
                }
                let room = 3u32.saturating_sub(u32::from(pet.fruits_today));
                let take = room.min(fruit);
                waiting += take;
                fruit -= take;
            }
        }
        b.pets = Some(if waiting > 0 {
            Badge::new(
                waiting.to_string(),
                if idle.is_some() { red } else { good },
            )
            .with(Some((t("to feed").to_string(), style::MUTED)))
        } else {
            Badge::new(t("all full").to_string(), good)
        });
    }

    // The guild: whether this character is signed up for what the guild
    // has planned. A battle nobody signed up for is the one thing on this
    // page that cannot be put right afterwards
    if let Some(guild) = gs.guild.as_ref() {
        use sf_api::gamestate::guild::BattlesJoined;
        let mine = guild
            .members
            .iter()
            .find(|m| m.name.eq_ignore_ascii_case(&player.name))
            .and_then(|m| m.battles_joined);
        let atk = guild.attacking.is_some();
        let def = guild.defending.is_some();
        let in_atk =
            matches!(mine, Some(BattlesJoined::Attack | BattlesJoined::Both));
        let in_def =
            matches!(mine, Some(BattlesJoined::Defense | BattlesJoined::Both));
        let planned = u8::from(atk) + u8::from(def);
        if planned > 0 {
            let joined = u8::from(atk && in_atk) + u8::from(def && in_def);
            let what = match (atk, def) {
                (true, true) => t("attack + defense"),
                (true, false) => t("attack"),
                _ => t("defense"),
            };
            b.guild = Some(
                Badge::new(
                    format!("{joined}/{planned}"),
                    if joined == planned { good } else { red },
                )
                .with(Some((what.to_string(), style::MUTED))),
            );
        }
    }

    // The arena manager has no clock at all - what it is for is the runes
    // The runes are outside the green/gold/red scale on purpose: that
    // scale is about urgency, and the runes are neither urgent nor
    // idle - they only ever go up and they are the total this whole
    // page exists to grow. So they get a colour of their own
    if let Some(idle) = gs.idle_game.as_ref() {
        let runes = crate::mechanics::runes_of(&idle.current_runes);
        b.idle = Some(Badge::new(compact(runes), style::NEON));
    }
    b
}

#[cfg(test)]
mod badge_tests {
    use super::countdown;

    /// The shape the game uses on its own tabs: a dungeon door says
    /// "51:27", never "0:51:27" and never "3087 s"
    #[test]
    fn a_countdown_reads_like_the_games_own() {
        assert_eq!(countdown(51 * 60 + 27), "51:27");
        assert_eq!(countdown(59), "0:59");
        assert_eq!(countdown(60), "1:00");
        assert_eq!(countdown(0), "0:00");
        // Past the hour the unit has to be there: "68:53" for a fortress
        // build reads as sixty-eight minutes and it is two and a half days
        assert_eq!(countdown(3600), "1:00 h");
        assert_eq!(countdown(3660), "1:01 h");
        assert_eq!(countdown(11 * 3600 + 5 * 60), "11:05 h");
        assert_eq!(countdown(68 * 3600 + 53 * 60), "2d 20h");
        // Never negative, whatever the clock says
        assert_eq!(countdown(-5), "0:00");
    }
}

#[cfg(test)]
mod plate_tests {
    use super::compact;

    /// The plate that ran off the end: 1 228 999 runes
    #[test]
    fn a_plate_figure_is_short_enough_to_fit() {
        assert_eq!(compact(1_228_999u64), "1.23M");
        assert_eq!(compact(112_952u64), "113k");
        assert_eq!(compact(1_219_000u64), "1.22M");
        assert_eq!(compact(999u64), "999");
        assert_eq!(compact(0u64), "0");
        // Rounding must roll over into the next suffix, not print four
        // digits in the old one
        assert_eq!(compact(999_999u64), "1.00M");
        // Whatever the number, it never grows past six characters, which is
        // what the plate has room for beside its word
        for n in [1u64, 999, 1_000, 999_999, 12_345_678, u64::MAX] {
            assert!(compact(n).len() <= 6, "{n} -> {}", compact(n));
        }
    }
}

/// One page's plate in the sidebar. `height` is Fill when the plates share
/// the column down to the logout; the icon and caption stay in the middle
/// of however tall the plate ends up
fn nav_plate(
    viewer: AccountIdent,
    this_page: AccountPage,
    active: bool,
    compact: bool,
    label: String,
    badge: Option<Badge>,
    height: Length,
) -> Element<'static, Message> {
    let mut hint = label.clone();
    let mut caption = column!(widget::text(label).size(if compact { 12 } else { 13 }).font(style::FAT))
        .spacing(0).width(Length::Fill);
    if let Some(badge) = badge {
        hint.push_str(&format!(" · {}", badge.text));
        if let Some((sub, _)) = &badge.sub { hint.push_str(&format!(" · {sub}")); }
        let short: String = badge.text.chars().take(23).collect();
        caption = caption.push(widget::text(short).size(if compact { 9 } else { 10 })
            .style(theme::Text::Color(badge.color)));
    }
    let face = row!(widget::image(page_icon(this_page)).width(21).height(21), caption)
        .spacing(10).align_items(Alignment::Center);
    widget::tooltip(
        button(container(face).height(Length::Fill).center_y())
            .padding([2, 9]).height(height)
            .width(Length::Fill).style(style::plate(active))
            .on_press(Message::ViewSubPage { player: viewer, page: this_page }),
        container(widget::text(hint).size(13)).padding(12).style(style::card()),
        widget::tooltip::Position::Right,
    ).gap(10).into()
}

/// Embedded Lucide icons, one semantic symbol per game module.
fn page_icon(page: AccountPage) -> widget::image::Handle {
    macro_rules! icon {
        ($name:literal) => {{
            static IMAGE: std::sync::OnceLock<widget::image::Handle> = std::sync::OnceLock::new();
            IMAGE.get_or_init(|| png_handle(include_bytes!(concat!("../../assets/navigation/", $name, ".png")), $name)).clone()
        }};
    }
    match page {
        AccountPage::Tavern => icon!("beer"),
        AccountPage::Scrapbook => icon!("swords"),
        AccountPage::Guard => icon!("shield"),
        AccountPage::Shops => icon!("store"),
        AccountPage::ArenaManager => icon!("chart-no-axes-combined"),
        AccountPage::Pets => icon!("paw-print"),
        AccountPage::Dungeons => icon!("skull"),
        AccountPage::Fortress => icon!("castle"),
        AccountPage::Underworld => icon!("flame"),
        AccountPage::Guild => icon!("users"),
        AccountPage::Mail => icon!("mail"),
        AccountPage::Log => icon!("scroll-text"),
        AccountPage::Options => icon!("user-round"),
    }
}
