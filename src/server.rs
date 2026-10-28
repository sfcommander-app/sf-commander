use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::{Arc, Mutex},
};

use chrono::{DateTime, Local};
use nohash_hasher::{IntMap, IntSet};
use sf_api::{
    gamestate::unlockables::EquipmentIdent, session::ServerConnection,
};

use crate::{
    AccountID, AccountIdent, CharacterInfo, QueID, ServerID,
    crawler::{CrawlAction, CrawlerState, WorkerQue},
    player::AccountInfo,
};

#[derive(Debug, Clone)]
pub enum CrawlingStatus {
    Waiting,
    Restoring,
    CrawlingFailed(String),
    Crawling {
        que_id: QueID,
        threads: usize,
        que: Arc<Mutex<WorkerQue>>,
        player_info: IntMap<u32, CharacterInfo>,
        equipment: HashMap<
            EquipmentIdent,
            HashSet<u32, ahash::RandomState>,
            ahash::RandomState,
        >,
        naked: BTreeMap<u16, IntSet<u32>>,
        last_update: DateTime<Local>,
        crawling_session: Option<Arc<CrawlerState>>,
        recent_failures: Vec<CrawlAction>,
    },
}

pub struct ServerInfo {
    pub ident: ServerIdent,
    pub accounts: HashMap<AccountID, AccountInfo, ahash::RandomState>,
    pub crawling: CrawlingStatus,
    pub connection: ServerConnection,
    pub headless_progress: Option<indicatif::ProgressBar>,
    /// How many times in a row the crawler had to log in again without a
    /// single request going through in between. Drives the pause before
    /// the next attempt (see `crawler_backoff`)
    pub crawler_revives: u32,
}

/// The pause before the crawler logs in again, by how many times in a row
/// it has already done so. The first relogin is immediate - a dropped
/// session happens. After that the pause doubles from 30 s up to 30 min:
/// a session that dies again within seconds is being taken away (another
/// copy of the app on the same crawler account, a server kick), and
/// logging in every seven seconds forever only spams the server and the
/// log. It used to do exactly that - 650 MB of helper.log
#[must_use]
pub fn crawler_backoff(revives: u32) -> std::time::Duration {
    if revives <= 1 {
        return std::time::Duration::ZERO;
    }
    let secs = 30u64.saturating_mul(1u64 << (revives - 2).min(10));
    std::time::Duration::from_secs(secs.min(30 * 60))
}

/// FNV-1a. The server id is written into helper.toml (the settings of a
/// character without an account entry), so it has to come out the same on
/// every start. `ahash::AHasher::default()` did not: its keys are drawn
/// fresh per process, so every restart gave the server a new id, the saved
/// settings were never found again, and a new default entry was added each
/// time - 71 of them for one character
#[must_use]
pub fn stable_hash(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ServerIdent {
    pub id: ServerID,
    pub url: String,
    pub ident: String,
}

impl ServerIdent {
    pub fn new(url: &str) -> Self {
        let url = url.trim_start_matches("https:");
        let url: String = url
            .chars()
            .map(|a| a.to_ascii_lowercase())
            .filter(|a| *a != '/')
            .collect();
        let ident: String =
            url.chars().filter(|a| a.is_alphanumeric()).collect();
        let id = stable_hash(ident.as_bytes());
        ServerIdent {
            id: ServerID(id),
            url,
            ident,
        }
    }
}

#[derive(Default)]
pub struct Servers(pub HashMap<ServerID, ServerInfo, ahash::RandomState>);

impl Servers {
    pub fn get_or_insert_default(
        &mut self,
        server_ident: ServerIdent,
        connection: ServerConnection,
        pb: Option<indicatif::ProgressBar>,
    ) -> &mut ServerInfo {
        self.0.entry(server_ident.id).or_insert_with(|| ServerInfo {
            ident: server_ident.clone(),
            accounts: Default::default(),
            crawling: CrawlingStatus::Waiting,
            connection,
            headless_progress: pb,
            crawler_revives: 0,
        })
    }

    pub fn get_ident(
        &self,
        ident: &AccountIdent,
    ) -> Option<(&ServerInfo, &AccountInfo)> {
        let server = self.0.get(&ident.server_id)?;
        let account = server.accounts.get(&ident.account)?;
        Some((server, account))
    }

    pub fn get_account_mut(
        &mut self,
        ident: &AccountIdent,
    ) -> Option<&mut AccountInfo> {
        self.0
            .get_mut(&ident.server_id)?
            .accounts
            .get_mut(&ident.account)
    }

    pub fn get(&self, id: &ServerID) -> Option<&ServerInfo> {
        self.0.get(id)
    }

    pub fn get_mut(&mut self, id: &ServerID) -> Option<&mut ServerInfo> {
        self.0.get_mut(id)
    }
}

#[cfg(test)]
mod tests_516 {
    use super::*;

    #[test]
    fn server_id_is_the_same_on_every_start() {
        // A fixed number, not just "equal twice in one process": the old
        // hasher was equal within a process too and different across them
        let a = ServerIdent::new("https://s29.sfgame.eu/");
        let b = ServerIdent::new("S29.SFGAME.EU");
        assert_eq!(a.id, b.id);
        assert_eq!(stable_hash(b"s29sfgameeu"), a.id.raw());
        assert_eq!(stable_hash(b""), 0xcbf2_9ce4_8422_2325);
    }

    #[test]
    fn crawler_backoff_grows_and_stops_at_half_an_hour() {
        use std::time::Duration;
        assert_eq!(crawler_backoff(0), Duration::ZERO);
        assert_eq!(crawler_backoff(1), Duration::ZERO);
        assert_eq!(crawler_backoff(2), Duration::from_secs(30));
        assert_eq!(crawler_backoff(3), Duration::from_secs(60));
        assert_eq!(crawler_backoff(6), Duration::from_secs(480));
        assert_eq!(crawler_backoff(8), Duration::from_secs(1800));
        assert_eq!(crawler_backoff(500), Duration::from_secs(1800));
    }
}
