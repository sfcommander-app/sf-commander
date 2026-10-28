//! Handing the live logins over to the next version.
//!
//! An update used to be a plain restart: the new exe started, logged
//! everybody in again from `helper.toml`, and whoever had come in through
//! Steam or Google landed on the login page - those logins can not be
//! renewed without the browser, and the library says so outright. The plain
//! accounts paid for it too: a fresh login is a new session, which is exactly
//! what throws a player out of their own browser window mid-game.
//!
//! Now the old version writes its sessions down (`Session::export()` in the
//! vendored sf-api), starts the new one with `--handover <file>` and only
//! quits once the file has been picked up. The new version carries on with
//! the very same sessions: nobody logs in, Steam stays, the game does not
//! notice. The file holds session ids and bearer tokens, so it lives for
//! seconds - the new version deletes it the moment it has read it, and the
//! old one deletes it if the new one never turns up.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use chrono::{DateTime, Local, NaiveDate};
use iced::Command;
use serde::{Deserialize, Serialize};
use sf_api::session::{
    LoginExport, PWHash, SSOAccountPool, Session, SessionExport,
};

use crate::{
    AccountPage, ConsoleLine, Helper, View, config::AccountConfig,
    login::PlayerAuth, message::Message, player::AccountStatus,
    server::ServerIdent, util::LockExt,
};

/// How long a handover stands still for a character that is busy, so it
/// can be carried instead of dropped. A password account only pays for a
/// drop with a fresh login, so it does not wait long
const CARRY_WAIT: Duration = Duration::from_secs(3);

/// The same for a Steam or Google character. A dropped SSO login can not
/// be renewed without the browser - the player has to come back by hand -
/// so it is worth a longer wait
const CARRY_WAIT_SSO: Duration = Duration::from_secs(15);

/// How often the wait looks again
const CARRY_STEP: Duration = Duration::from_millis(100);

/// Everything the next version needs to pick up where this one stops
#[derive(Debug, Serialize, Deserialize)]
pub struct Handover {
    pub from_version: String,
    pub written_at: DateTime<Local>,
    pub accounts: Vec<AccountHandover>,
    /// The character page that was open, if one was
    #[serde(default)]
    pub view: Option<ViewHandover>,
    #[serde(default)]
    pub console: Vec<ConsoleLine>,
    #[serde(default)]
    pub successes: Vec<ConsoleLine>,
    /// STOP was pressed - the new version must not start talking to the
    /// game just because it is new
    #[serde(default)]
    pub paused: bool,
    /// Burst mode: the session that is open and when the next one is due.
    /// Without these the new version would open a fresh session at once
    #[serde(default)]
    pub burst_until: Option<DateTime<Local>>,
    #[serde(default)]
    pub burst_next: Option<DateTime<Local>>,
    /// Characters the old version could not write down - they have to log
    /// in again. Carried so the new version can say so out loud instead
    /// of leaving the player to notice a missing tab
    #[serde(default)]
    pub lost: Vec<String>,
}

/// One logged in character
#[derive(Debug, Serialize, Deserialize)]
pub struct AccountHandover {
    /// `ServerIdent::url` - "s29.sfgame.net"
    pub server: String,
    /// Lowercase, the way `AccountInfo::name` has it
    pub name: String,
    /// The password hash of a plain login, so that a later relog and the
    /// "remember me" bookkeeping work as before. `None` for an SSO character
    pub pw_hash: Option<PWHash>,
    pub session: SessionExport,
    /// The game state as JSON text. It is parsed on its own so that a state
    /// the new version can not read only costs one login - never the
    /// session, which is the part that matters
    pub gamestate: String,
    /// Today's mushroom floor - see `AccountInfo::mushroom_day`. Without it
    /// the new version would take the current count as the day's start and
    /// the "one more than the morning" rule would quietly reset mid-day
    #[serde(default)]
    pub mushroom_day: Option<(NaiveDate, u32)>,
    #[serde(default)]
    pub last_level: Option<u16>,
    #[serde(default)]
    pub epics_seen: Option<usize>,
    #[serde(default)]
    pub dungeons_cleared: Option<usize>,
    /// Jobs that were waiting their turn, by `Job::key`. Whether anything
    /// was automated at all is read off the config by the new version -
    /// every switch is written there the moment it is flipped
    #[serde(default)]
    pub queue: Vec<String>,
    /// Every automation switch as it was - the config has them only for
    /// characters logged in by password, a Steam login has nothing there
    #[serde(default)]
    pub flags: Option<crate::player::AutoFlags>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ViewHandover {
    pub server: String,
    pub name: String,
    pub page: AccountPage,
}

/// What a config entry is matched against, to know whether the handover
/// already covers it - so it is not logged in a second time on top
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountKey {
    server: ServerIdent,
    name: String,
    /// The SSO account this character came through, lowercase
    sso_account: Option<String>,
}

impl AccountKey {
    pub fn of(acc: &AccountHandover) -> Self {
        AccountKey {
            server: ServerIdent::new(&acc.server),
            name: acc.name.to_lowercase(),
            sso_account: match &acc.session.login {
                LoginExport::SSO { account, .. } => {
                    Some(account.username.to_lowercase())
                }
                LoginExport::Basic { .. } => None,
            },
        }
    }

    /// Would logging this config entry in duplicate a character (or an SSO
    /// account) the handover already brought along? A second login of the
    /// same character is a new session, and a new session is precisely the
    /// thing the handover exists to avoid
    pub fn covers(&self, acc: &AccountConfig) -> bool {
        match acc {
            AccountConfig::Regular { name, server, .. } => {
                self.name == name.to_lowercase()
                    && self.server == ServerIdent::new(server)
            }
            AccountConfig::SF { name, .. } => self
                .sso_account
                .as_deref()
                .is_some_and(|a| a == name.to_lowercase()),
        }
    }
}

const PREFIX: &str = "SF-Commander-handover-";

/// A fresh file name next to the exe, random so that two handovers (a
/// crashed one and a retry) can never mean the same file
#[must_use]
pub fn new_path() -> PathBuf {
    crate::app_path(&format!("{PREFIX}{:016x}.json", fastrand::u64(..)))
}

/// Only files this program itself would have written: our prefix, sixteen
/// hex digits, `.json`. Anything else on the command line is ignored rather
/// than read - the path comes from `--handover`, and a process argument is
/// not something to trust blindly
fn is_handover_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix(PREFIX) else {
        return false;
    };
    let Some(hex) = rest.strip_suffix(".json") else {
        return false;
    };
    hex.len() == 16 && hex.bytes().all(|b| b.is_ascii_hexdigit())
}

fn is_handover_file(path: &Path) -> bool {
    let named_right = path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(is_handover_name);
    let own_folder = crate::app_path("x").parent().map(Path::to_path_buf);
    let here = path.parent().map(Path::to_path_buf);
    // Both folders have to exist and be the same one - two paths that fail
    // to canonicalize are not "equal", they are unknown
    named_right
        && match (own_folder, here) {
            (Some(a), Some(b)) => {
                matches!(
                    (a.canonicalize(), b.canonicalize()),
                    (Ok(a), Ok(b)) if a == b
                )
            }
            _ => false,
        }
}

pub fn write(handover: &Handover, path: &Path) -> Result<(), String> {
    let text = serde_json::to_string(handover).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())
}

/// Reads the file the previous version left and deletes it right away -
/// whether or not it could be parsed, because whatever is in there must not
/// stay on disk
pub fn read_and_remove(path: &Path) -> Result<Handover, String> {
    if !is_handover_file(path) {
        return Err(format!(
            "{} is not a handover file of this program",
            path.display()
        ));
    }
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string());
    if let Err(e) = std::fs::remove_file(path) {
        log::warn!("could not remove {}: {e}", path.display());
    }
    let handover: Handover =
        serde_json::from_str(&text?).map_err(|e| e.to_string())?;
    Ok(handover)
}

/// Blocks until the new version has deleted the file, or `timeout` is up -
/// in which case the file is removed here, so session ids never outlive the
/// handover. Called right before the old process quits, so blocking the UI
/// thread is fine
pub fn wait_until_taken(path: &Path, timeout: Duration) {
    let start = std::time::Instant::now();
    while path.exists() {
        if start.elapsed() > timeout {
            log::warn!(
                "the new version did not pick up the handover in time - \
                 removing it, the characters will log in from the config"
            );
            let _ = std::fs::remove_file(path);
            return;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// The most recent lines of a feed, oldest first
fn tail(lines: &std::collections::VecDeque<ConsoleLine>) -> Vec<ConsoleLine> {
    const KEEP: usize = 400;
    lines
        .iter()
        .skip(lines.len().saturating_sub(KEEP))
        .cloned()
        .collect()
}

impl Helper {
    /// Writes everything down, starts `new_exe` with it and quits. Only
    /// returns when the new version could not be started - the sessions are
    /// back in place then and the app carries on as it was
    pub fn hand_over_to(&mut self, new_exe: &Path) -> Result<(), String> {
        let (handover, taken) = self.collect_handover();
        let carried = handover.accounts.len();
        let file = new_path();
        let started = write(&handover, &file).and_then(|()| {
            crate::update::restart_into(new_exe, Some(&file))
                .map_err(|e| e.to_string())
        });
        if let Err(e) = started {
            let _ = std::fs::remove_file(&file);
            for (status, session) in taken {
                status.lock_safe().put_session(session);
            }
            return Err(e);
        }
        log::info!(
            "handing {carried} characters over to {}",
            new_exe.display()
        );
        wait_until_taken(&file, Duration::from_secs(20));
        std::process::exit(0);
    }

    /// Takes every idle session out of its account - so that no poll can
    /// touch it while the new version is starting up - and writes it down.
    /// The sessions come back with the result, in case the handover has to
    /// be undone
    #[allow(clippy::type_complexity)]
    fn collect_handover(
        &mut self,
    ) -> (Handover, Vec<(Arc<Mutex<AccountStatus>>, Box<Session>)>) {
        let mut accounts = Vec::new();
        let mut taken = Vec::new();
        let mut lost: Vec<String> = Vec::new();
        for server in self.servers.0.values() {
            for acc in server.accounts.values() {
                let status = acc.status.clone();
                // A character in the middle of a request can not be written
                // down, and neither can an SSO character whose account lock
                // is held by one (`Session::export` can only `try_lock` the
                // shared SFAccount - it is a tokio mutex and this is sync
                // code). Both clear in a second or two.
                //
                // For a password account being dropped only costs a fresh
                // login. For a Steam or Google one it costs the key: those
                // logins can not be renewed without the browser, so the
                // player lands on the login page and has to come back by
                // hand. That is worth standing still a few seconds for -
                // we are about to exit anyway
                let sso = matches!(acc.auth, PlayerAuth::SSO);
                let mut waited = Duration::ZERO;
                let patience = if sso { CARRY_WAIT_SSO } else { CARRY_WAIT };
                let (session, gamestate) = loop {
                    let mut lock = status.lock_safe();
                    let taken = lock.take_session("Handing over to the new version");
                    let gamestate = match &*lock {
                        AccountStatus::Busy(gs, _) => {
                            serde_json::to_string(&**gs).ok()
                        }
                        _ => None,
                    };
                    drop(lock);
                    if let Some(session) = taken {
                        // The session is ours now, so nothing of this
                        // character's can hold the SSO lock - only another
                        // character of the same account, and that clears too
                        let mut export = session.export();
                        while export.is_none() && waited < patience {
                            std::thread::sleep(CARRY_STEP);
                            waited += CARRY_STEP;
                            export = session.export();
                        }
                        break (Some((session, export)), gamestate);
                    }
                    if waited >= patience {
                        break (None, gamestate);
                    }
                    std::thread::sleep(CARRY_STEP);
                    waited += CARRY_STEP;
                };
                let Some((session, export)) = session else {
                    log::warn!(
                        "{}: still busy after {}s - the character logs in \
                         again after the update",
                        acc.ident,
                        patience.as_secs()
                    );
                    lost.push(titlecase::titlecase(&acc.name));
                    continue;
                };
                match (export, gamestate) {
                    (Some(export), Some(gamestate)) => {
                        accounts.push(AccountHandover {
                            server: server.ident.url.clone(),
                            name: acc.name.clone(),
                            pw_hash: match &acc.auth {
                                PlayerAuth::Normal(h) => Some(h.clone()),
                                PlayerAuth::SSO => None,
                            },
                            session: export,
                            gamestate,
                            mushroom_day: acc.mushroom_day,
                            last_level: acc.last_level,
                            epics_seen: acc.epics_seen,
                            dungeons_cleared: acc.dungeons_cleared,
                            queue: acc
                                .queue
                                .iter()
                                .map(|(j, _)| j.key().to_string())
                                .collect(),
                            flags: Some(crate::player::AutoFlags::of(acc)),
                        });
                    }
                    _ => {
                        log::warn!(
                            "{}: the session could not be written down - the \
                             character logs in again after the update",
                            acc.ident
                        );
                        lost.push(titlecase::titlecase(&acc.name));
                    }
                }
                taken.push((status, session));
            }
        }
        let view = match &self.current_view {
            View::Account { ident, page } => {
                self.servers.get_ident(ident).map(|(s, a)| ViewHandover {
                    server: s.ident.url.clone(),
                    name: a.name.clone(),
                    page: *page,
                })
            }
            _ => None,
        };
        let handover = Handover {
            from_version: env!("CARGO_PKG_VERSION").to_string(),
            written_at: Local::now(),
            accounts,
            view,
            console: tail(&self.console),
            successes: tail(&self.successes),
            paused: self.paused,
            burst_until: self.burst_until,
            burst_next: self.burst_next,
            lost,
        };
        (handover, taken)
    }

    /// Picks up what the previous version left: every character continues
    /// on its own session, the feeds and the open page come back. Returns
    /// the commands that finish the job asynchronously
    pub fn resume_handover(
        &mut self,
        handover: Handover,
    ) -> Vec<Command<Message>> {
        let mut pool = SSOAccountPool::new();
        let mut commands = Vec::new();
        let mut carried = 0usize;
        for acc in handover.accounts {
            let key = (acc.server.clone(), acc.name.clone());
            match self.resume(acc, &mut pool) {
                Some(cmd) => {
                    carried += 1;
                    commands.push(cmd);
                }
                None => log::warn!(
                    "{}@{}: could not take the session over - logging in \
                     from the config instead",
                    key.1,
                    key.0
                ),
            }
        }
        self.console.extend(handover.console);
        self.successes.extend(handover.successes);
        self.console_seen = self.console.len();
        self.paused = handover.paused;
        self.burst_until = handover.burst_until;
        self.burst_next = handover.burst_next;
        if let Some(v) = handover.view {
            let server = ServerIdent::new(&v.server);
            let found = self.servers.0.get(&server.id).and_then(|s| {
                s.accounts.values().find(|a| a.name == v.name).map(|a| a.ident)
            });
            if let Some(ident) = found {
                self.current_view = View::Account {
                    ident,
                    page: v.page,
                };
            }
        }
        let line = format!(
            "Updated to version {} (from {}) - {carried} characters carried \
             over without a new login, the automation carries on where it \
             was",
            crate::display_version(),
            handover.from_version.trim_end_matches(".0")
        );
        log::info!("{line}");
        self.console(None, line, Some(true));
        // Say plainly who did not make it. A Steam or Google character
        // that is dropped can not be logged in again from the config -
        // the player has to come back through the browser, and they
        // should hear that from the app rather than find an empty tab
        if !handover.lost.is_empty() {
            let warn = format!(
                "These characters were in the middle of something and could \
                 not be carried over: {}. A password character logs in again \
                 on its own; a Steam or Google one has to be logged in by \
                 hand",
                handover.lost.join(", ")
            );
            log::warn!("{warn}");
            self.console(None, warn, Some(false));
        }
        commands
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CharacterConfig;
    use sf_api::sso::{AccountSessionExport, SFAccountExport, SSOAuthExport};

    fn export(login: LoginExport) -> SessionExport {
        SessionExport {
            server_url: "https://s29.sfgame.net/".into(),
            session_id: "1".repeat(32),
            player_id: 7,
            login_count: 2,
            crypto_id: "5-1".into(),
            crypto_key: "k".into(),
            login,
        }
    }

    fn plain(name: &str) -> AccountHandover {
        AccountHandover {
            server: "s29.sfgame.net".into(),
            name: name.to_lowercase(),
            pw_hash: Some(PWHash::from_hash("h".into())),
            session: export(LoginExport::Basic {
                username: name.into(),
                pw_hash: PWHash::from_hash("h".into()),
            }),
            gamestate: "{}".into(),
            mushroom_day: Some((NaiveDate::from_ymd_opt(2026, 9, 11).unwrap(), 30)),
            last_level: Some(120),
            epics_seen: None,
            dungeons_cleared: None,
            queue: vec!["expedition".into()],
            flags: Some(crate::player::AutoFlags {
                expedition: true,
                ..Default::default()
            }),
        }
    }

    fn steam(name: &str, account: &str) -> AccountHandover {
        let mut a = plain(name);
        a.pw_hash = None;
        a.session = export(LoginExport::SSO {
            username: name.into(),
            character_id: "c1".into(),
            account: SFAccountExport {
                username: account.into(),
                auth: SSOAuthExport::Steam,
                session: AccountSessionExport {
                    uuid: "u".into(),
                    bearer_token: "t".into(),
                },
            },
            session: AccountSessionExport {
                uuid: "u".into(),
                bearer_token: "t".into(),
            },
        });
        a
    }

    /// The whole point of the file is to come back exactly as written -
    /// the mushroom floor included, or the +1 rule resets on every update
    #[test]
    fn handover_survives_json() {
        let h = Handover {
            from_version: "1.60.3".into(),
            written_at: Local::now(),
            accounts: vec![plain("Hero"), steam("Alt", "steamguy")],
            view: Some(ViewHandover {
                server: "s29.sfgame.net".into(),
                name: "hero".into(),
                page: AccountPage::Fortress,
            }),
            console: vec![ConsoleLine {
                at: Local::now(),
                who: "Hero".into(),
                text: "did a thing".into(),
                ok: Some(true),
                dup: false,
            }],
            successes: vec![],
            paused: true,
            burst_until: None,
            burst_next: Some(Local::now()),
            lost: vec!["Busybody".into()],
        };
        let json = serde_json::to_string(&h).unwrap();
        let back: Handover = serde_json::from_str(&json).unwrap();
        assert_eq!(back.accounts.len(), 2);
        assert_eq!(back.accounts[0].mushroom_day.unwrap().1, 30);
        assert_eq!(back.accounts[0].last_level, Some(120));
        assert!(back.accounts[1].pw_hash.is_none());
        assert_eq!(back.view.unwrap().page, AccountPage::Fortress);
        assert_eq!(back.console[0].text, "did a thing");
        assert_eq!(back.accounts[0].queue, vec!["expedition".to_string()]);
        assert!(back.accounts[0].flags.as_ref().is_some_and(|f| f.expedition));
        assert!(back.paused);
        assert!(back.burst_next.is_some());
        // Whoever could not be carried is named, so the new version can
        // say so instead of leaving an empty tab to be noticed
        assert_eq!(back.lost, vec!["Busybody".to_string()]);
        // ...and a file from a version that did not have the field still
        // loads, with nobody lost
        let older = json.replace(",\"lost\":[\"Busybody\"]", "");
        let back: Handover = serde_json::from_str(&older).unwrap();
        assert!(back.lost.is_empty());
    }

    /// A character the handover brought along must not be logged in a
    /// second time from the config - that second login is the very kick
    /// the handover is there to prevent. Case and URL spelling must not
    /// matter, the config is typed by people
    #[test]
    fn config_entries_covered_by_the_handover_are_recognised() {
        let keys: Vec<AccountKey> = [plain("Hero"), steam("Alt", "SteamGuy")]
            .iter()
            .map(AccountKey::of)
            .collect();
        let covered = |acc: &AccountConfig| keys.iter().any(|k| k.covers(acc));

        let regular = |name: &str, server: &str| AccountConfig::Regular {
            name: name.into(),
            pw_hash: PWHash::from_hash("h".into()),
            server: server.into(),
            config: CharacterConfig::default(),
        };
        assert!(covered(&regular("hero", "s29.sfgame.net")));
        assert!(covered(&regular("Hero", "https://s29.sfgame.net/")));
        assert!(!covered(&regular("hero", "s30.sfgame.net")));
        assert!(!covered(&regular("other", "s29.sfgame.net")));

        let sf = |name: &str| AccountConfig::SF {
            name: name.into(),
            pw_hash: PWHash::from_hash("h".into()),
            characters: vec![],
        };
        assert!(covered(&sf("steamguy")));
        assert!(!covered(&sf("somebody")));
        // A plain character is not an SSO account, whatever its name
        assert!(!covered(&sf("hero")));
    }

    /// Only our own randomly named files are ever read or deleted
    #[test]
    fn only_own_handover_names_are_accepted() {
        assert!(is_handover_name("SF-Commander-handover-0123456789abcdef.json"));
        for bad in [
            "SF-Commander-handover-0123456789abcdef.json.bak",
            "SF-Commander-handover-0123456789abcde.json",
            "SF-Commander-handover-0123456789abcdefg.json",
            "SF-Commander-handover-zzzzzzzzzzzzzzzz.json",
            "helper.toml",
            "handover-0123456789abcdef.json",
        ] {
            assert!(!is_handover_name(bad), "{bad}");
        }
        assert!(!is_handover_file(Path::new(
            "C:/somewhere/else/SF-Commander-handover-0123456789abcdef.json"
        )));
    }
}
