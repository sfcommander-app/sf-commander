use std::{
    sync::{Arc, Mutex, atomic::AtomicU64},
    time::Duration,
};

use crate::i18n::t;
// His lettering stands in for the toolkit's `text` here as on every other
// page - this was the one screen still set in the system face
use crate::ui::glyphs::lettered as text;
use iced::{
    Alignment, Command, Element, Length, theme,
    widget::{
        self, button, column, container, horizontal_space, row, text_input,
    },
};
use sf_api::{
    error::SFError,
    gamestate::GameState,
    session::{PWHash, ServerConnection, Session},
    sso::{SFAccount, SSOAuth, SSOProvider},
};
use tokio::time::sleep;

use crate::util::LockExt;
use crate::{
    AccountID, AccountIdent, AccountInfo, AccountPage, Helper, ServerIdent,
    View, config::AccountConfig, get_server_code, message::Message, top_bar,
};

pub struct LoginState {
    pub login_typ: LoginType,
    pub name: String,
    pub password: String,
    pub server: String,
    pub remember_me: bool,
    pub error: Option<String>,
    pub active_sso: Vec<SSOLogin>,
    pub import_que: Vec<Session>,
    pub google_sso: Arc<Mutex<SSOStatus>>,
    pub steam_sso: Arc<Mutex<SSOStatus>>,
}

pub enum SSOStatus {
    Waiting { url: String },
    Initializing,
}

#[derive(Debug)]
pub struct SSOLogin {
    pub ident: SSOIdent,
    pub status: SSOLoginStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SSOIdent {
    SF(String),
    Google(String),
    Steam(String),
}

#[derive(Debug)]
pub enum SSOLoginStatus {
    Loading,
    Success,
}

impl LoginState {
    /// The door of the app.
    ///
    /// This was a row of seven grey tabs across the top and a form
    /// floating in the middle of an empty screen - the layout of the tool
    /// this one grew out of, to the pixel, and the one screen every new
    /// member sees first. It now looks like the rest of this app and
    /// nothing else: the word-mark over the top, the ways in down the left
    /// as the same plates the account pages use for their pages, and the
    /// form on a card beside them. Every way in is still here; what moved
    /// is where it stands and what it is called
    pub fn view(
        &self,
        accounts: &[AccountConfig],
        has_active: bool,
    ) -> Element<'_, Message> {
        use crate::ui::{glyphs, style};

        // One plate per way in, down the left. The name in his lettering,
        // and on the right a count where there is something to count: how
        // many saved characters, how many SSO characters waiting to be
        // picked up. The same shape as the pages plates on an account
        let plate = |label: &'static str,
                     kind: LoginType,
                     badge: Option<String>|
         -> Element<'_, Message> {
            let active = kind == self.login_typ;
            let right: Element<Message> = match badge {
                Some(b) => glyphs::write_right(&b, style::NAME_GOLD, 16.0),
                None => text("").into(),
            };
            let inner = row!(
                glyphs::write(
                    &t(label).to_uppercase(),
                    style::SIDE_TEXT,
                    17.0
                ),
                horizontal_space(),
                right,
            )
            .spacing(8)
            .align_items(Alignment::Center)
            .height(Length::Fill);
            button(inner)
                .on_press(Message::LoginViewChanged(kind))
                .padding([0, 10])
                .width(Length::Fill)
                .height(Length::Fixed(44.0))
                .style(style::plate(active))
                .into()
        };

        // Saved characters first, because that is what everybody who has
        // been here before is looking for. The S&F account next, which is
        // how almost everybody logs in now. The old per-server login is
        // still here for whoever needs it, at the bottom, under a name
        // that says what it is - "Regular" said nothing
        let mut ways = column!().spacing(4).width(Length::Fill);
        if !accounts.is_empty() {
            ways = ways.push(plate(
                "Saved characters",
                LoginType::Saved,
                Some(accounts.len().to_string()),
            ));
        }
        ways = ways
            .push(plate("S&F Account", LoginType::SFAccount, None))
            .push(plate("Steam", LoginType::Steam, None))
            .push(plate("Google", LoginType::Google, None))
            .push(plate("Server login", LoginType::Regular, None));
        if !self.active_sso.is_empty() {
            ways = ways.push(plate(
                "SSO",
                LoginType::SSOAccounts,
                Some(self.active_sso.len().to_string()),
            ));
        }
        if !self.import_que.is_empty() {
            ways = ways.push(plate(
                "SSO characters",
                LoginType::SSOChars,
                Some(self.import_que.len().to_string()),
            ));
        }
        // The plates stand on the page by themselves. They sat on the
        // black strip the account pages' column uses, and here, not
        // running the height of the window, that strip was a black box
        // drawn around five buttons
        let ways = container(ways).width(Length::Fixed(230.0));

        // ------------------------------------------------------- the form
        let field = |hint: &'static str, value: &str| {
            text_input(t(hint), value)
                .font(style::FAT)
                .size(14)
                .padding(8)
        };
        let go = |label: &'static str, msg: Message| {
            button(
                container(glyphs::write_on(
                    &t(label).to_uppercase(),
                    style::SIDE_TEXT,
                    18.0,
                    glyphs::Plate::Dark,
                ))
                .width(Length::Fill)
                .center_x(),
            )
            .padding([8, 14])
            .width(Length::Fill)
            .style(style::brass(true))
            .on_press(msg)
        };
        let remember = || {
            glyphs::ticked(t("Remember me"), self.remember_me)
                .on_toggle(Message::RememberMe)
                .size(16)
                .text_size(13)
        };
        // What went wrong, in red, or nothing. A line that is always
        // there and usually empty still takes its height
        let error_line: Element<Message> = match &self.error {
            Some(e) => container(
                text(format!("{} {e}", t("Error:")))
                    .size(13)
                    .style(theme::Text::Color(style::BAD)),
            )
            .width(Length::Fill)
            .into(),
            None => column!().into(),
        };

        // The entry in a list of characters or accounts: name on the left
        // in his lettering, where it lives on the right
        let entry = |name: &str,
                     where_: &str,
                     colour: iced::Color,
                     on: Option<Message>|
         -> Element<'_, Message> {
            let inner = row!(
                glyphs::write(
                    &titlecase::titlecase(name).to_uppercase(),
                    colour,
                    16.0
                ),
                horizontal_space(),
                text(where_.to_string())
                    .size(12)
                    .style(theme::Text::Color(style::MUTED)),
            )
            .spacing(10)
            .align_items(Alignment::Center);
            button(inner)
                .padding([6, 10])
                .width(Length::Fill)
                .style(style::quiet())
                .on_press_maybe(on)
                .into()
        };

        // A list of characters is as tall as its rows up to this, and
        // scrolls past it - two saved characters do not need a card the
        // height of twenty, and twenty must not run off the window
        const LIST_H: f32 = 300.0;
        const ROW_H: f32 = 38.0;
        let list_h = |n: usize| {
            Length::Fixed((n.max(1) as f32 * ROW_H).min(LIST_H))
        };
        let form: Element<Message> = match self.login_typ {
            LoginType::SFAccount => column!(
                style::heading(t("S&F Account Login")),
                field("Username", &self.name)
                    .on_input(Message::LoginNameInputChange),
                field("Password", &self.password)
                    .on_input(Message::LoginPWInputChange)
                    .on_submit(Message::LoginSFSubmit)
                    .secure(true),
                remember(),
                go("Login", Message::LoginSFSubmit),
                error_line,
                style::note(t(
                    "The account you log into the game's own website with. \
                     Every character on it shows up as its own row.",
                )),
            )
            .spacing(10)
            .into(),
            LoginType::Regular => column!(
                style::heading(t("Server login")),
                field("Username", &self.name)
                    .on_input(Message::LoginNameInputChange),
                field("Password", &self.password)
                    .on_input(Message::LoginPWInputChange)
                    .on_submit(Message::LoginRegularSubmit)
                    .secure(true),
                field("f1.sfgame.net", &self.server)
                    .on_input(Message::LoginServerChange)
                    .on_submit(Message::LoginRegularSubmit),
                remember(),
                go("Login", Message::LoginRegularSubmit),
                error_line,
                style::note(t(
                    "A character that lives on one server and was never \
                     tied to an S&F account. The server is the address you \
                     play on, e.g. s29.sfgame.eu.",
                )),
            )
            .spacing(10)
            .into(),
            LoginType::Steam | LoginType::Google => {
                let (label, status) = if self.login_typ == LoginType::Steam {
                    ("Steam", &self.steam_sso)
                } else {
                    ("Google", &self.google_sso)
                };
                let action: Element<Message> = match &*status.lock_safe() {
                    SSOStatus::Waiting { url } => go(
                        "Open the login page",
                        Message::OpenLink(url.to_string()),
                    )
                    .into(),
                    _ => text(t("Waiting..."))
                        .size(13)
                        .style(theme::Text::Color(style::MUTED))
                        .into(),
                };
                column!(
                    style::heading(label),
                    action,
                    style::note(t(
                        "The browser opens the provider's own page. Once you \
                         are through, come back here - the characters turn \
                         up on their own.",
                    )),
                )
                .spacing(10)
                .into()
            }
            LoginType::Saved => {
                let mut list = column!().spacing(4).width(Length::Fill);
                for acc in accounts {
                    let on = Some(Message::Login {
                        account: acc.clone(),
                        auto_login: false,
                    });
                    list = list.push(match acc {
                        AccountConfig::Regular { name, server, .. } => entry(
                            name,
                            &get_server_code(server),
                            style::SIDE_TEXT,
                            on,
                        ),
                        AccountConfig::SF { name, .. } => {
                            entry(name, t("S&F account"), style::GOOD, on)
                        }
                    });
                }
                column!(
                    style::heading(t("Saved characters")),
                    widget::scrollable(list).height(list_h(accounts.len())),
                )
                .spacing(10)
                .into()
            }
            LoginType::SSOAccounts => {
                let mut list = column!().spacing(4).width(Length::Fill);
                for active in &self.active_sso {
                    let name = match &active.ident {
                        SSOIdent::SF(n) | SSOIdent::Google(n) | SSOIdent::Steam(n) => {
                            n.as_str()
                        }
                    };
                    let (where_, colour, on) = match active.status {
                        SSOLoginStatus::Loading => {
                            (t("loading..."), style::MUTED, None)
                        }
                        SSOLoginStatus::Success => (
                            t("ready"),
                            style::GOOD,
                            Some(Message::LoginViewChanged(LoginType::SSOChars)),
                        ),
                    };
                    list = list.push(entry(name, where_, colour, on));
                }
                column!(
                    style::heading(t("SSO Accounts")),
                    widget::scrollable(list)
                        .height(list_h(self.active_sso.len())),
                )
                .spacing(10)
                .into()
            }
            LoginType::SSOChars => {
                let mut list = column!().spacing(4).width(Length::Fill);
                for (pos, active) in self.import_que.iter().enumerate() {
                    list = list.push(entry(
                        active.username(),
                        &get_server_code(active.server_url().as_str()),
                        style::SIDE_TEXT,
                        Some(Message::SSOImport { pos }),
                    ));
                }
                column!(
                    style::heading(t("SSO Characters")),
                    widget::scrollable(list)
                        .height(list_h(self.import_que.len())),
                )
                .spacing(10)
                .into()
            }
        };
        let form = container(form)
            .padding(18)
            .width(Length::Fixed(420.0))
            .style(style::card());

        // The word-mark, big, over the top - the one thing on this screen
        // that says whose it is before a word is read
        let mark = column!(
            glyphs::write("S&F COMMANDER", style::NAME_GOLD, 46.0),
            text(format!(
                "v{} \u{00B7} \u{00A9} {} {}",
                crate::display_version(),
                chrono::Local::now().format("%Y"),
                crate::admin::AUTHOR,
            ))
            .size(11)
            .style(theme::Text::Color(style::MUTED)),
        )
        .spacing(2)
        .align_items(Alignment::Center);

        let body = row!(ways, form)
            .spacing(14)
            .align_items(Alignment::Start);

        // The strip stays: Settings on the right, Back on the left once
        // there is something to go back to. Its middle is empty - the
        // ways in are not tabs any more
        let top_bar = top_bar(
            column!().into(),
            if has_active {
                Some(Message::ViewOverview)
            } else {
                None
            },
        );

        column!(
            top_bar,
            widget::vertical_space().height(Length::Fixed(40.0)),
            mark,
            widget::vertical_space().height(Length::Fixed(28.0)),
            body,
        )
        .height(Length::Fill)
        .width(Length::Fill)
        .align_items(Alignment::Center)
        .into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub enum LoginType {
    Regular,
    SFAccount,
    Steam,
    Google,
    Saved,
    SSOAccounts,
    SSOChars,
}

pub struct SSOValidator {
    pub status: Arc<Mutex<SSOStatus>>,
    pub provider: SSOProvider,
}

impl SSOValidator {
    pub async fn check(
        &self,
    ) -> Result<Option<(Vec<Result<Session, SFError>>, String)>, SFError> {
        sleep(Duration::from_millis(fastrand::u64(500..=1000))).await;
        let mut auth = SSOAuth::new(self.provider).await?;
        {
            *self.status.lock_safe() = SSOStatus::Waiting {
                url: auth.auth_url().to_string(),
            }
        }

        for _ in 0..50 {
            let resp = auth.try_login().await?;
            match resp {
                sf_api::sso::AuthResponse::Success(res) => {
                    println!("Success");
                    let name = res.username().to_string();
                    let chars = res.characters().await?;
                    return Ok(Some((chars, name)));
                }
                sf_api::sso::AuthResponse::NoAuth(res) => {
                    auth = res;
                }
            }
            sleep(Duration::from_secs(6)).await;
        }
        {
            *self.status.lock_safe() = SSOStatus::Initializing
        }
        Ok(None)
    }
}

/// The connection a character talks through: the machine's own address,
/// or the proxy set for that character (see `CharacterConfig::proxy`).
/// Several characters from one address are the plainest sign of
/// multi-accounting a server can see, and a different route per
/// character is the only thing that changes it
pub fn connection_for(
    config: &crate::config::Config,
    name: &str,
    server: &str,
) -> Option<ServerConnection> {
    let ident = ServerIdent::new(server);
    let proxy = config
        .get_char_conf(name, ident.id)
        .map(|c| c.proxy.trim().to_string())
        .filter(|p| !p.is_empty());
    match proxy {
        None => ServerConnection::new(server),
        Some(url) => {
            log::info!("{name}: connecting through a proxy");
            ServerConnection::new_with_options(
                server,
                sf_api::session::ConnectionOptions {
                    proxy: Some(sf_api::session::ProxySettings {
                        url,
                        username: None,
                        password: None,
                    }),
                    ..Default::default()
                },
            )
        }
    }
}

impl Helper {
    pub fn login_regular(
        &mut self,
        name: String,
        server: String,
        pw_hash: PWHash,
        remember: bool,
        auto_login: bool,
    ) -> Command<Message> {
        let name = name.trim().to_string();
        let server = server.trim().to_string();

        let Some(con) = connection_for(&self.config, &name, &server) else {
            self.login_state.error = Some("Invalid Server URL".to_string());
            return Command::none();
        };

        let session =
            sf_api::session::Session::new_hashed(&name, pw_hash.clone(), con);

        self.login(session, remember, PlayerAuth::Normal(pw_hash), auto_login)
    }

    /// Puts a character on its server's list, in the "logging in" state.
    /// `None` when a character of that name is already there - the page
    /// switches to it instead, a second login would only kick the first
    fn register_account(
        &mut self,
        session: &Session,
        auth: PlayerAuth,
        auto_login: bool,
    ) -> Option<AccountIdent> {
        let server_ident = ServerIdent::new(session.server_url().as_str());
        let Some(connection) = ServerConnection::new(&server_ident.url) else {
            self.login_state.error =
                Some("Server Url is not valid".to_string());
            return None;
        };
        let name: String = session
            .username()
            .chars()
            .map(|a| a.to_ascii_lowercase())
            .collect();

        let account_id = AccountID::new();
        let account_ident = AccountIdent {
            server_id: server_ident.id,
            account: account_id,
        };
        let info = AccountInfo::new(&name, auth, account_ident);
        let server = self
            .servers
            .get_or_insert_default(server_ident, connection, None);

        if let Some((_, existing)) =
            server.accounts.iter().find(|(_, a)| a.name == name)
        {
            self.current_view = View::Account {
                ident: existing.ident,
                page: AccountPage::Scrapbook,
            };
            return None;
        }
        if !auto_login {
            self.current_view = View::Account {
                ident: info.ident,
                page: AccountPage::Scrapbook,
            };
        }
        server.accounts.insert(info.ident.account, info);
        Some(account_ident)
    }

    /// Continues a character on the session the previous version handed
    /// over - no login. The game state comes along as text; if the new
    /// version can not read it (the library changed underneath), the
    /// session logs in once instead, which for an SSO character still needs
    /// no browser: the bearer token is in the session. `None` when the
    /// session itself can not be rebuilt
    pub fn resume(
        &mut self,
        acc: crate::handover::AccountHandover,
        pool: &mut sf_api::session::SSOAccountPool,
    ) -> Option<Command<Message>> {
        let connection = connection_for(&self.config, &acc.name, &acc.server)?;
        let session = Session::restore(acc.session, connection, pool)?;
        let auth = acc.pw_hash.map_or(PlayerAuth::SSO, PlayerAuth::Normal);
        let ident = self.register_account(&session, auth, true)?;
        if let Some(a) = self.servers.get_account_mut(&ident) {
            a.mushroom_day = acc.mushroom_day;
            a.last_level = acc.last_level;
            a.epics_seen = acc.epics_seen;
            a.dungeons_cleared = acc.dungeons_cleared;
            a.pending_flags = acc.flags;
            // What was waiting, plus the full-bot starter set - the same
            // push the ON button gives. The checkers of the new process
            // start their clocks from zero, so without this the first job
            // after an update came a minute and a half later. Jobs whose
            // automation is off are dropped by the queue, so this costs a
            // character with everything switched off nothing
            let now = chrono::Local::now();
            let carried = acc
                .queue
                .iter()
                .filter_map(|k| crate::player::Job::from_key(k));
            for job in carried.chain(crate::player::Job::FULL_BOT_START) {
                if !a.queue.iter().any(|(j, _)| *j == job) {
                    a.queue.push((job, now));
                }
            }
        }
        let gamestate = acc.gamestate;
        Some(Command::perform(
            async move {
                let mut session = session;
                let gs = match serde_json::from_str::<GameState>(&gamestate) {
                    Ok(gs) => gs,
                    Err(e) => {
                        log::warn!(
                            "{ident}: the handed over game state does not \
                             parse ({e}) - logging in on the same session"
                        );
                        let resp = session.login().await?;
                        GameState::new(resp)?
                    }
                };
                Ok((Box::new(gs), Box::new(session)))
            },
            move |a: Result<_, SFError>| match a {
                Ok((gs, session)) => Message::LoggininSuccess {
                    ident,
                    gs,
                    session,
                    remember: false,
                },
                Err(err) => Message::LoggininFailure {
                    ident,
                    error: err.to_string(),
                },
            },
        ))
    }

    /// Logs a character in, one second later per login already waiting -
    /// several at once at start look like an attack, and the server rate
    /// limits them
    pub fn login(
        &mut self,
        mut session: sf_api::session::Session,
        remember: bool,
        auth: PlayerAuth,
        auto_login: bool,
    ) -> Command<Message> {
        let Some(account_ident) =
            self.register_account(&session, auth, auto_login)
        else {
            return Command::none();
        };
        static WAITING: AtomicU64 = AtomicU64::new(0);

        Command::perform(
            async move {
                // The place in the line is given back when `_slot` drops -
                // on success, on error, and when this future is dropped
                let (ahead, _slot) = LoginSlot::take(&WAITING);
                if ahead > 0 {
                    sleep(Duration::from_secs(ahead)).await;
                }
                let resp = session.login().await?;
                let gs = GameState::new(resp)?;
                let gs = Box::new(gs);
                Ok((gs, Box::new(session)))
            },
            move |a: Result<_, SFError>| match a {
                Ok((gs, session)) => Message::LoggininSuccess {
                    ident: account_ident,
                    gs,
                    session,
                    remember,
                },
                Err(err) => Message::LoggininFailure {
                    ident: account_ident,
                    error: err.to_string(),
                },
            },
        )
    }

    pub fn login_sf_acc(
        &mut self,
        name: String,
        pwhash: PWHash,
        remember_sf: bool,
        auto_login: bool,
    ) -> Command<Message> {
        let ident = SSOIdent::SF(name.clone());
        self.login_state.login_typ = LoginType::SSOAccounts;
        if self
            .login_state
            .active_sso
            .iter()
            .any(|a| matches!(&a.ident, SSOIdent::SF(s) if s.as_str() == name.as_str()))
        {
            return Command::none();
        }
        self.login_state.active_sso.push(SSOLogin {
            ident: ident.clone(),
            status: SSOLoginStatus::Loading,
        });

        let n2 = name.clone();
        let p2 = pwhash.clone();
        Command::perform(
            async move {
                let account = SFAccount::login_hashed(n2, p2).await?;
                account.characters().await.into_iter().flatten().collect()
            },
            move |res| match res {
                Ok(chars) => Message::SSOLoginSuccess {
                    name,
                    pass: pwhash,
                    chars,
                    remember: remember_sf,
                    auto_login,
                },
                Err(error) => Message::SSOLoginFailure {
                    name,
                    error: error.to_string(),
                },
            },
        )
    }
}

#[allow(clippy::upper_case_acronyms)]
pub enum PlayerAuth {
    Normal(PWHash),
    SSO,
}

/// One login's place in the line. Logins are spaced a second apart per
/// login already waiting, and the place is given back when this is
/// dropped - on success, on error, and when the future is cancelled. It
/// used to be given back only after a successful login (`inspect` on the
/// `Ok`), so every failed one left a ghost in the line and each later
/// login of the process waited a second longer, for good
pub struct LoginSlot<'a> {
    line: &'a AtomicU64,
}

impl<'a> LoginSlot<'a> {
    /// Joins the line: how many are ahead, and the slot to hold
    pub fn take(line: &'a AtomicU64) -> (u64, Self) {
        let ahead = line.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        (ahead, LoginSlot { line })
    }
}

impl Drop for LoginSlot<'_> {
    fn drop(&mut self) {
        self.line.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

#[cfg(test)]
mod slot_tests {
    use super::*;

    /// Three logins, two of them failing (dropped without success): the
    /// line is empty afterwards and the next login does not wait
    #[test]
    fn failed_logins_leave_the_line() {
        let line = AtomicU64::new(0);
        let (a, slot_a) = LoginSlot::take(&line);
        let (b, slot_b) = LoginSlot::take(&line);
        let (c, slot_c) = LoginSlot::take(&line);
        assert_eq!((a, b, c), (0, 1, 2));
        // Two "errors": the futures end without a success
        drop(slot_a);
        drop(slot_c);
        assert_eq!(line.load(std::sync::atomic::Ordering::SeqCst), 1);
        drop(slot_b);
        assert_eq!(line.load(std::sync::atomic::Ordering::SeqCst), 0);
        let (again, _slot) = LoginSlot::take(&line);
        assert_eq!(again, 0);
    }
}
