#![windows_subsystem = "windows"]
mod admin;
mod archive;
mod attendance;
mod backpack;
mod backup;
mod chat;
mod companions;
mod config;
mod crawler;
mod dungeon;
mod expedition;
mod fight_log;
mod gems;
mod goblin;
mod guild_fight;
mod guild_finder;
mod guild_history;
mod guild_report;
mod harvest;
mod handover;
mod i18n;
mod licence;
mod login;
mod mechanics;
mod message;
mod pace;
mod pet_sim;
mod player;
mod preview;
mod probe_history;
mod quest;
mod server;
mod shopping;
mod smith;
mod sound;
mod spend_ledger;
mod stealth;
mod storage;
mod ui;
mod update;
mod util;

use std::{
    cmp::Reverse,
    collections::{BTreeMap, BinaryHeap, HashMap, HashSet, hash_map::Entry},
    sync::{Arc, Mutex, atomic::AtomicU64},
    time::Duration,
};

use chrono::{Local, NaiveDate, Utc};
use clap::{Parser, Subcommand};
use config::{AccountConfig, Config};
use crawler::{CrawlAction, Crawler, CrawlerState, CrawlingOrder, WorkerQue};
use iced::{
    Alignment, Application, Command, Element, Length, Settings, Subscription,
    Theme, executor, subscription,
    widget::{button, container, horizontal_space, row},
};
// His lettering stands in for the toolkit's `text`, so every line on
// this screen is set in the face the purse card is
use crate::ui::glyphs::lettered as text;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use log::{debug, info, trace};
use log4rs::{
    append::{
        console::{ConsoleAppender, Target},
        file::FileAppender,
        rolling_file::{
            RollingFileAppender,
            policy::compound::{
                CompoundPolicy, roll::fixed_window::FixedWindowRoller,
                trigger::size::SizeTrigger,
            },
        },
    },
    config::{Appender, Logger, Root},
    encode::pattern::PatternEncoder,
};
use login::{LoginState, LoginType, PlayerAuth, SSOStatus, SSOValidator};
use nohash_hasher::{IntMap, IntSet};
use player::{
    AccountInfo, AccountStatus, AttrChecker, AutoAttackChecker,
    AutoDungeonChecker, AutoFortressChecker, AutoLureChecker, DailiesChecker,
    DiceChecker, ExpeditionChecker, FortressMaintainChecker, GuardChecker,
    GuildAutoChecker, IdleChecker, PetFeedChecker, ScrapbookInfo, ShopChecker,
    UnderworldChecker, WheelChecker,
};
use serde::{Deserialize, Serialize};
use server::{CrawlingStatus, ServerIdent, ServerInfo, Servers};
use sf_api::{
    gamestate::{character::Class, unlockables::EquipmentIdent},
    session::ServerConnection,
    sso::{SSOProvider, ServerLookup},
};
use tokio::time::sleep;

pub use crate::util::{LockExt, friendly_error, retry_delay};
use crate::{
    config::{AccountCreds, AvailableTheme, TargetOrder},
    message::Message,
};
pub const PER_PAGE: usize = 51;
/// How many fights the greedy battle plan looks ahead
pub const MAX_PLAN_STEPS: usize = 30;
/// Where a thank-you can go
pub const COFFEE_URL: &str = "https://buymeacoffee.com/gussolean";

#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    pub sub: Option<CLICommand>,
    /// Set by an update on the version it replaced, so the new one can delete
    /// the old file once it is no longer running. Not meant to be typed
    #[arg(long, hide = true)]
    pub replaced: Option<std::path::PathBuf>,
    /// Set by an update: the file the previous version wrote its live logins
    /// to, so this one continues on them instead of logging in again. Read
    /// and deleted on start. Not meant to be typed
    #[arg(long, hide = true)]
    pub handover: Option<std::path::PathBuf>,
    /// Open an offline, read-only preview with sample character data
    #[arg(long)]
    pub preview: bool,
}

#[derive(Debug, Subcommand, Clone)]
enum CLICommand {
    Crawl {
        /// The amount of servers that will be simultaniously crawled
        #[arg(short, long, default_value_t = 4, value_parser=concurrency_limits)]
        concurrency: usize,
        /// The amount of threads per server used to
        #[arg(short, long, default_value_t = 1, value_parser=concurrency_limits)]
        threads: usize,
        #[clap(flatten)]
        servers: ServerSelect,
    },
}
fn concurrency_limits(s: &str) -> Result<usize, String> {
    clap_num::number_range(s, 1, 50)
}

#[derive(Debug, clap::Args, Clone)]
#[group(required = true, multiple = false)]
pub struct ServerSelect {
    /// Fetches a list of all servers and crawls all of them. Supercedes urls
    #[arg(short, long)]
    all: bool,
    /// The list of all server urls to fetch
    #[arg(short, long, value_delimiter = ' ', num_args = 1..)]
    urls: Option<Vec<String>>,
}

impl Args {
    pub fn is_headless(&self) -> bool {
        self.sub.is_some()
    }
}

fn main() -> iced::Result {
    let args = Args::parse();

    let is_headless = args.is_headless();
    let config = get_log_config(is_headless);
    log4rs::init_config(config).unwrap();
    info!("Starting up");

    let mut settings = Settings::with_flags(args);
    // Every screen is laid out for a maximised 1920x1080 display: three
    // columns, the arena's three offer cards, the whole guild on one page
    // without scrolling. Opening a shade under that leaves room for the
    // title bar and the taskbar, so nothing is cut off before the window
    // is maximised
    settings.window.size = iced::Size {
        width: 1880.0,
        height: 1000.0,
    };
    if settings.flags.preview {
        if let Ok(size) = std::env::var("SFC_PREVIEW_SIZE") {
            if let Some((w, h)) = size.split_once('x') {
                if let (Ok(width), Ok(height)) = (w.parse::<f32>(), h.parse::<f32>()) {
                    if width.is_finite() && height.is_finite() {
                        settings.window.size = iced::Size::new(width.max(1200.0), height.max(720.0));
                    }
                }
            }
        }
    }
    settings.window.position = iced::window::Position::Centered;
    // The layout is built for this: three columns, the arena's three offer
    // cards, the member table. Anything smaller clips the right column
    settings.window.min_size = Some(iced::Size {
        width: 1200.0,
        height: 720.0,
    });
    settings.fonts.extend([
        std::borrow::Cow::Borrowed(include_bytes!("../assets/fonts/Aurora-Circuit.ttf").as_slice()),
        std::borrow::Cow::Borrowed(include_bytes!("../assets/fonts/Aurora-Circuit-Strong.ttf").as_slice()),
        std::borrow::Cow::Borrowed(include_bytes!("../assets/fonts/Aurora-Circuit-Display.ttf").as_slice()),
    ]);
    settings.default_font = crate::ui::style::UI_FONT;
    settings.default_text_size = 13.0f32.into();
    settings.window.visible = !is_headless;

    let raw_img = include_bytes!("../assets/icon.ico");
    let img =
        image::load_from_memory_with_format(raw_img, image::ImageFormat::Ico)
            .ok();
    if let Some(img) = img {
        let height = img.height();
        let width = img.width();
        let icon =
            iced::window::icon::from_rgba(img.into_bytes(), width, height).ok();
        settings.window.icon = icon;
    }
    Helper::run(settings)
}

struct Helper {
    preview: bool,
    /// How big the window is right now. The member table sizes its type
    /// from the width and stretches its rows down to the height - the
    /// alternative is a ladder of guesses that is either too small to read
    /// or too wide to fit, and it was both in turn
    window_w: f32,
    window_h: f32,
    servers: Servers,
    current_view: View,
    tavern_tab: ui::TavernTab,
    login_state: LoginState,
    config: Config,
    should_update: bool,
    class_images: ClassImages,
    cli_crawling: Option<CLICrawling>,
    /// Diary of guild refreshes: level progress and battle sign-ups
    guild_history: guild_history::GuildHistory,
    /// What past fights taught us about the enemies the simulation can not
    /// rate (the twister and its kind)
    probe_history: probe_history::ProbeHistory,
    /// Mushrooms spent today per character (dungeon, arena, shop rerolls),
    /// on disk - the daily ceilings used to live in memory only and started
    /// over with every restart
    spend: spend_ledger::SpendLedger,
    /// What the simulation predicted against what really happened - the
    /// arena's odds are calibrated from it (see `fight_log`)
    fights: fight_log::FightLog,
    /// How often each character took the portal, the defense and the
    /// attack - and the misses handed out to keep it under 100 %
    attendance: attendance::Attendance,
    /// `helper.toml` is there but unreadable (and so is its backup): the
    /// error, shown on the recovery page until the user decides
    config_problem: Option<String>,
    /// The config came from the backup - said once in the banner
    config_notice: Option<String>,
    /// What is typed into the admin password box
    admin_input: String,
    /// Which page of the settings is showing
    settings_tab: SettingsTab,
    /// The licence code as it is being typed or pasted, before it is
    /// checked. Kept apart from the stored one so a bad paste does not
    /// throw away a good licence
    licence_input: String,
    /// When any account last started a queued job (global spacing)
    last_job: Option<chrono::DateTime<chrono::Local>>,
    /// The earliest moment the *next* request of any character may go out.
    ///
    /// The queue already keeps a gap between jobs, but the chains that
    /// bypass it - the arena manager's run of purchases, the attribute
    /// buying, the blacksmith - each sleep on their own clock, so two
    /// characters ended up clicking in the same second. One person running
    /// several accounts from one address is exactly what that looks like
    /// from the server's side, so every request now takes a slot
    next_send_at: Option<chrono::DateTime<chrono::Local>>,
    /// When every enabled job was last pushed into every queue at once -
    /// the sweep that catches whatever a checker is still sleeping on
    last_sweep: Option<chrono::DateTime<chrono::Local>>,
    global_gap: i64,
    /// Pause: nothing is started from the queues, settings stay as they are
    paused: bool,
    /// Where the licence stands. Worked out at startup and whenever a code
    /// is pasted, not on every frame - it is a signature check, and the
    /// answer only changes when the code or the day does
    licence: crate::licence::Status,
    /// Everything that happened, every character, newest last
    console: std::collections::VecDeque<ConsoleLine>,
    /// Only the good news, every character, newest last: XP, gold, wood,
    /// stone, gems, souls, wins, purchases, upgrades. The console has all of
    /// this too, but buried between refreshes and refusals - this is the
    /// "what did I actually get" window
    successes: std::collections::VecDeque<ConsoleLine>,
    console_open: bool,
    /// The successes and the live feed show the character whose tab is open;
    /// this switches them to every character at once
    feed_all: bool,
    /// Console length at the last scroll-to-bottom
    console_seen: usize,
    /// When the Save button in Settings was last pressed
    settings_saved_at: Option<chrono::DateTime<chrono::Local>>,
    /// Burst mode: when the current session ends / the next one starts
    burst_until: Option<chrono::DateTime<chrono::Local>>,
    burst_next: Option<chrono::DateTime<chrono::Local>>,
    /// The day's shift of the quiet hours (see `stealth`), drawn once a day
    quiet_shift: Option<(chrono::NaiveDate, i64)>,
    /// The day the goblin last said good night, so he says it once
    quiet_said: Option<chrono::NaiveDate>,
    /// "Do it now": full speed until this moment, then back to whatever
    /// the slider says. The one thing the slider could not do was "I am
    /// sitting here watching, get on with it" - the only way to see a
    /// result at once was FULL POWER, which is a settings change that
    /// also throws away the sessions and the quiet night for good
    boost_until: Option<chrono::DateTime<chrono::Local>>,
    /// Since when nothing has been in hand during a "do it now" run - the
    /// run ends once that has lasted `BOOST_DONE_AFTER_SECS`
    boost_idle_since: Option<chrono::DateTime<chrono::Local>>,
    /// How long the last session was (seconds), so the next is never alike
    last_session_len: Option<i64>,
    /// When the open session began, for the cap on extensions
    session_started: Option<chrono::DateTime<chrono::Local>>,
    /// Since when there has been nothing at all to do in the open session
    idle_since: Option<chrono::DateTime<chrono::Local>>,
    /// A newer version has been downloaded and is waiting for a quiet moment
    /// to take over
    /// How many people use the app, once it has been asked for: (today,
    /// this week). Only ever shown behind the owner password
    user_count: Option<UserCount>,
    /// Level ups nobody has clicked away yet, oldest first. One entry per
    /// level, so somebody who was away all night sees every single one and
    /// can count them
    level_popups: std::collections::VecDeque<(String, u16)>,
    /// Epic finds waiting to be acknowledged: (character, what it was). Shown
    /// over everything, from whichever page the user is on
    epic_popups: std::collections::VecDeque<(String, String)>,
    update_ready: Option<std::path::PathBuf>,
    /// When the new version finished downloading. The handover happens as
    /// soon as no request is in flight, with a short settling pause so a
    /// chain of steps can finish first
    update_since: Option<chrono::DateTime<chrono::Local>>,
    /// What the update check last did, for the banner
    update_status: Option<String>,
    /// Guild keys whose chat and member list have been taken as the
    /// starting point for the hello/goodbye - everything already there is
    /// old news. Switching the feature on used to greet everybody whose
    /// join line was still in the chat, four hellos in a row
    chat_baseline: HashSet<String>,
    /// The last few hundred chat lines written to each guild's archive
    /// file, so a line the game repeats on every refresh goes in once
    chat_archived: HashMap<String, std::collections::VecDeque<String>>,
    /// What the archive last had to say - the path written to, or why it
    /// could not be
    chat_archive_note: Option<String>,
}

/// One line of the console
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsoleLine {
    pub at: chrono::DateTime<chrono::Local>,
    pub who: String,
    pub text: String,
    /// None = neutral, Some(true) = success, Some(false) = failure
    pub ok: Option<bool>,
    /// The same event already went to the successes window, so the live
    /// feed next to it leaves this line out - it used to show every win
    /// twice, once green on top and once green below
    #[serde(default)]
    pub dup: bool,
}

impl ConsoleLine {
    /// Was a success for `who` written just now? The successes line and the
    /// console line of one event are pushed in the same update, so "the last
    /// success is this character's and less than a second old" is that
    /// event - the bot never does two things to one character that close
    pub fn just_succeeded(
        successes: &std::collections::VecDeque<ConsoleLine>,
        who: &str,
    ) -> bool {
        successes.back().is_some_and(|s| {
            s.who.eq_ignore_ascii_case(who)
                && (chrono::Local::now() - s.at).num_milliseconds() < 1000
        })
    }
}

struct CLICrawling {
    todo_servers: Vec<String>,
    mbp: MultiProgress,
    threads: usize,
    active: usize,
}

struct ClassImages {
    assassin: iced::widget::image::Handle,
    bard: iced::widget::image::Handle,
    berserk: iced::widget::image::Handle,
    battle_mage: iced::widget::image::Handle,
    demon_hunter: iced::widget::image::Handle,
    druid: iced::widget::image::Handle,
    necromancer: iced::widget::image::Handle,
    scout: iced::widget::image::Handle,
    warrior: iced::widget::image::Handle,
    mage: iced::widget::image::Handle,
    paladin: iced::widget::image::Handle,
}

macro_rules! load_class_image {
    ($path:expr) => {{
        let raw_img = include_bytes!($path);
        let image = image::load_from_memory_with_format(
            raw_img,
            image::ImageFormat::WebP,
        )
        .unwrap();
        iced::widget::image::Handle::from_pixels(
            image.width(),
            image.height(),
            image.into_bytes(),
        )
    }};
}

impl ClassImages {
    pub fn new() -> ClassImages {
        ClassImages {
            assassin: load_class_image!("../assets/classes/assassin.webp"),
            paladin: load_class_image!("../assets/classes/paladin.webp"),
            bard: load_class_image!("../assets/classes/bard.webp"),
            berserk: load_class_image!("../assets/classes/berserk.webp"),
            demon_hunter: load_class_image!(
                "../assets/classes/demon_hunter.webp"
            ),
            druid: load_class_image!("../assets/classes/druid.webp"),
            necromancer: load_class_image!(
                "../assets/classes/necromancer.webp"
            ),
            scout: load_class_image!("../assets/classes/scout.webp"),
            warrior: load_class_image!("../assets/classes/warrior.webp"),
            mage: load_class_image!("../assets/classes/mage.webp"),
            battle_mage: load_class_image!(
                "../assets/classes/battle_mage.webp"
            ),
        }
    }

    pub fn get_handle(&self, class: Class) -> iced::widget::image::Handle {
        match class {
            Class::Warrior => self.warrior.clone(),
            Class::Mage => self.mage.clone(),
            Class::Scout => self.scout.clone(),
            Class::Assassin => self.assassin.clone(),
            Class::BattleMage => self.battle_mage.clone(),
            Class::Berserker => self.berserk.clone(),
            Class::DemonHunter => self.demon_hunter.clone(),
            Class::Druid => self.druid.clone(),
            Class::Bard => self.bard.clone(),
            Class::Necromancer => self.necromancer.clone(),
            Class::Paladin => self.paladin.clone(),
            // TODO: Give this a custom icon
            Class::PlagueDoctor => self.assassin.clone(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
enum View {
    Account {
        ident: AccountIdent,
        page: AccountPage,
    },
    Overview {
        selected: HashSet<AccountIdent>,
        action: Option<ActionSelection>,
    },
    Login,
    Settings,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ActionSelection {
    Multi,
    Character(AccountIdent),
}

/// Which part of the settings is on screen.
///
/// Everything used to be one column: theme, threads, the whole queue
/// order, every stealth knob, the owner corner and the licence, stacked
/// into a single scroll that ran several screens deep. Nothing was ever
/// two screens from anything else - it was all one screen, arbitrarily
/// far down it. These are four pages instead, and each one fits
#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
pub enum SettingsTab {
    /// Language, theme, what the lists show
    #[default]
    Look,
    /// How it plays: the pace and the automation numbers
    Play,
    /// What order the jobs run in - thirteen rows of its own
    Queue,
    /// Timing, quiet hours, proxies - everything the server could notice
    Stealth,
    /// The owner password, what is behind it, and the licence
    Owner,
}

impl SettingsTab {
    pub const ALL: [SettingsTab; 5] =
        [Self::Look, Self::Play, Self::Queue, Self::Stealth, Self::Owner];

    /// The word on the tab. English, like every other key - `i18n` turns
    /// it into whatever language is set
    pub fn label(self) -> &'static str {
        match self {
            Self::Look => "Look",
            Self::Play => "Playing",
            Self::Queue => "Queue",
            Self::Stealth => "Unremarkable",
            Self::Owner => "Owner",
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
enum AccountPage {
    Tavern,
    Scrapbook,
    Guard,
    Shops,
    ArenaManager,
    Pets,
    Dungeons,
    Fortress,
    Underworld,
    Guild,
    Mail,
    Log,
    Options,
}

/// The version as shown to people: "1.17" instead of the "1.17.0" cargo
/// insists on
pub fn display_version() -> &'static str {
    let v = env!("CARGO_PKG_VERSION");
    v.strip_suffix(".0").unwrap_or(v)
}

/// Where a data file (the config, the diaries) belongs: next to the running
/// exe, not next to whatever folder the process happened to start in. An
/// update ships a new exe with a new name into the same folder and restarts,
/// and the working directory it inherits is not guaranteed to be that folder -
/// so a file addressed by working directory could go missing on every update,
/// taking the login and every setting with it. The exe's own folder never
/// moves. Falls back to a bare relative name only if the exe path can not be
/// read at all, which is the old behaviour
#[must_use]
pub fn app_path(name: &str) -> std::path::PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.join(name)))
        .unwrap_or_else(|| std::path::PathBuf::from(name))
}

pub fn get_server_code(server: &str) -> String {
    let server = server.trim_start_matches("https:");
    let server = server.trim_start_matches("http:");
    let server = server.replace('/', "");
    let mut parts = server.split('.');
    let a = parts.next();
    _ = parts.next();
    let b = parts.next();

    match (a, b) {
        (Some(a), Some(b)) => {
            format!("{a}.{b}")
        }
        _ => String::new(),
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct CharacterInfo {
    equipment: Vec<EquipmentIdent>,
    name: String,
    uid: u32,
    level: u16,
    #[serde(skip)]
    stats: Option<u32>,
    #[serde(skip)]
    fetch_date: Option<NaiveDate>,
    #[serde(skip)]
    class: Option<Class>,
    /// Not in any guild when last looked at - a recruit for ours. Kept in
    /// the backup: a crawl takes days and this is what the recruiting list
    /// is made of
    #[serde(default)]
    no_guild: bool,
}

impl CharacterInfo {
    pub fn is_old(&self) -> bool {
        self.fetch_date.unwrap_or_default() < Utc::now().date_naive()
    }

    /// A bare target crawled `today` or not, for tests that only care about
    /// whether `is_old` sees it as fresh. Not compiled into the app.
    #[cfg(test)]
    pub fn test_stub(fresh: bool) -> Self {
        Self::test_stub_at(fresh, 1)
    }

    /// The same, at a chosen level - the lure picks by level
    #[cfg(test)]
    pub fn test_stub_at(fresh: bool, level: u16) -> Self {
        Self {
            equipment: Vec::new(),
            name: String::new(),
            uid: 0,
            level,
            stats: None,
            no_guild: false,
            fetch_date: fresh.then(|| Utc::now().date_naive()),
            class: None,
        }
    }
}

impl PartialOrd for CharacterInfo {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CharacterInfo {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.level.cmp(&other.level) {
            core::cmp::Ordering::Equal => {}
            ord => return ord.reverse(),
        }
        match self.name.cmp(&other.name) {
            core::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        self.uid.cmp(&other.uid)
    }
}

impl Application for Helper {
    type Executor = executor::Default;

    type Message = Message;

    type Theme = Theme;

    type Flags = Args;

    fn new(flags: Args) -> (Self, iced::Command<Self::Message>) {
        // A config that is there but unreadable is not "no accounts". The
        // app then runs on an empty config with every write blocked and
        // shows the recovery page instead of the login page
        let (config, config_problem, config_notice) = if flags.preview {
            (Config::default(), None, None)
        } else {
            match Config::restore() {
                Ok(None) => (Config::default(), None, None),
                Ok(Some((c, storage::Source::Main))) => (c, None, None),
                Ok(Some((c, storage::Source::Backup))) => (
                    c,
                    None,
                    Some(
                        "helper.toml could not be read - the backup \
                         (helper.toml.bak) was loaded instead. The broken \
                         file is kept next to it as helper.toml.broken-..."
                            .to_string(),
                    ),
                ),
                Err(e) => {
                    Config::block_writes();
                    (Config::default(), Some(e), None)
                }
            }
        };
        i18n::set_lang(config.lang);
        pace::set(config.pace);
        player::Job::set_order(&config.job_order);
        // Before the first frame is drawn, and before any of the early
        // returns below - the preview takes one of them, so a switch set
        // further down would have been read by the real app and not by the
        // harness that is supposed to show what the real app looks like
        crate::ui::glyphs::set_artwork(config.drawn_font);
        crate::ui::glyphs::set_help(config.show_help);

        let mut helper = Helper {
            preview: flags.preview,
            tavern_tab: ui::TavernTab::default(),
            // What the window opens at (see `main`); a resize corrects it
            window_w: 1880.0,
            window_h: 1000.0,
            servers: Default::default(),
            login_state: LoginState {
                login_typ: if config.accounts.is_empty() {
                    LoginType::Regular
                } else {
                    LoginType::Saved
                },
                name: String::new(),
                password: String::new(),
                server: "f1.sfgame.net".to_string(),
                error: None,
                remember_me: true,
                active_sso: vec![],
                import_que: vec![],
                google_sso: Arc::new(Mutex::new(SSOStatus::Initializing)),
                steam_sso: Arc::new(Mutex::new(SSOStatus::Initializing)),
            },
            current_view: View::Login,
            should_update: false,
            class_images: ClassImages::new(),
            config,
            cli_crawling: None,
            guild_history: if flags.preview {
                Default::default()
            } else {
                guild_history::GuildHistory::load()
            },
            probe_history: if flags.preview {
                Default::default()
            } else {
                probe_history::ProbeHistory::load()
            },
            admin_input: String::new(),
            settings_tab: SettingsTab::default(),
            licence_input: String::new(),
            last_job: None,
            next_send_at: None,
            last_sweep: None,
            global_gap: 2,
            paused: false,
            licence: crate::licence::Status::Missing,
            console: std::collections::VecDeque::new(),
            successes: std::collections::VecDeque::new(),
            console_open: false,
            // Everybody's lines, told apart by the name - the per-character
            // view made the column look like nothing was happening
            feed_all: true,
            console_seen: 0,
            settings_saved_at: None,
            burst_until: None,
            burst_next: None,
            quiet_shift: None,
            quiet_said: None,
            boost_until: None,
            boost_idle_since: None,
            last_session_len: None,
            session_started: None,
            idle_since: None,
            user_count: None,
            level_popups: std::collections::VecDeque::new(),
            epic_popups: std::collections::VecDeque::new(),
            update_ready: None,
            update_since: None,
            update_status: None,
            chat_baseline: HashSet::new(),
            chat_archived: HashMap::new(),
            chat_archive_note: None,
            config_problem,
            config_notice,
            spend: if flags.preview {
                Default::default()
            } else {
                spend_ledger::SpendLedger::load()
            },
            fights: if flags.preview {
                Default::default()
            } else {
                fight_log::FightLog::load()
            },
            attendance: if flags.preview {
                Default::default()
            } else {
                attendance::Attendance::load()
            },
        };

        if flags.preview {
            preview::populate(&mut helper);
            return (
                helper,
                iced::font::load(iced_aw::BOOTSTRAP_FONT_BYTES)
                    .map(Message::FontLoaded),
            );
        }
        let mut commands = vec![];
        // An update just took over - the version it replaced can go now
        if let Some(old) = flags.replaced.clone() {
            commands.push(Command::perform(
                crate::update::clean_up_old(old),
                |()| Message::Noop,
            ));
        }
        // ...and it may have left its live logins behind. Whatever is in the
        // file is picked up further down, once the config accounts have
        // been gone through - the ones the handover covers are skipped
        // there, a second login would only kick the first
        let restored = flags.handover.as_deref().and_then(|path| {
            match handover::read_and_remove(path) {
                Ok(h) => Some(h),
                Err(e) => {
                    log::warn!(
                        "no handover to pick up ({e}) - logging in from \
                         the config instead"
                    );
                    None
                }
            }
        });
        let covered: Vec<handover::AccountKey> = restored
            .iter()
            .flat_map(|h| h.accounts.iter().map(handover::AccountKey::of))
            .collect();
        // First check shortly after start, then on the subscription's clock
        commands.push(Command::perform(
            async {
                sleep(Duration::from_secs(10)).await;
            },
            |()| Message::UpdateCheck,
        ));

        if let Some(CLICommand::Crawl {
            concurrency,
            threads,
            servers,
        }) = flags.sub
        {
            let mut info = CLICrawling {
                todo_servers: Vec::new(),
                mbp: MultiProgress::new(),
                active: concurrency,
                threads,
            };

            if let Some(servers) = servers.urls {
                info.todo_servers = servers;

                for _ in 0..concurrency {
                    commands.push(Command::perform(async {}, move |_| {
                        Message::NextCLICrawling
                    }))
                }
            } else if servers.all {
                let c = Command::perform(
                    async {
                        ServerLookup::fetch().await.ok().map(|a| {
                            a.all()
                                .into_iter()
                                .map(|a| a.to_string())
                                .filter(|a| a != "https://speed.sfgame.net/")
                                .collect()
                        })
                    },
                    move |servers| Message::CrawlAllRes {
                        servers,
                        concurrency,
                    },
                );
                commands.push(c);
            }
            helper.cli_crawling = Some(info);
        }
        commands.push(
            iced::font::load(iced_aw::BOOTSTRAP_FONT_BYTES)
                .map(Message::FontLoaded),
        );

        let (loading, logins) = helper.saved_logins(&covered);
        commands.extend(logins);

        if loading > 0 || !covered.is_empty() {
            helper.current_view = View::Overview {
                selected: Default::default(),
                action: Default::default(),
            };
        }
        if let Some(h) = restored {
            commands.extend(helper.resume_handover(h));
        }
        // Before anything is allowed to go out. Every send path asks
        // `halted()`, and `halted()` reads what this works out
        helper.refresh_licence();

        (helper, Command::batch(commands))
    }

    fn theme(&self) -> Theme {
        self.config.theme.theme()
    }

    fn title(&self) -> String {
        format!("S&F Commander {}", display_version())
    }

    fn update(
        &mut self,
        message: Self::Message,
    ) -> iced::Command<Self::Message> {
        if self.preview {
            return preview::update(self, message);
        }
        // let start = std::time::Instant::now();
        // let msg = format!("{message:?}");
        let res = self.handle_msg(message);
        _ = &res;
        // let time = start.elapsed();
        // if true{
        //     println!(
        //         "{} took: {time:?}",
        //         msg.split('{').next().unwrap_or(&msg).trim(),
        //     );
        // }
        res
    }

    fn view(
        &self,
    ) -> iced::Element<'_, Self::Message, Self::Theme, iced::Renderer> {
        let page = self.view_current_page();
        if !self.console_open {
            return page;
        }
        // The console sits under whatever page is open
        iced::widget::column!(page, self.view_console())
            .height(iced::Length::Fill)
            .width(iced::Length::Fill)
            .into()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        // The one event the layout needs from the window itself
        let resized = iced::event::listen_with(|event, _| match event {
            iced::Event::Window(
                _,
                iced::window::Event::Resized { width, height },
            ) => Some(Message::WindowResized { width, height }),
            _ => None,
        });
        if self.preview {
            return resized;
        }
        // Disambiguates running subscriptions
        #[derive(Debug, Hash, PartialEq, Eq)]
        enum SubIdent {
            RefreshUI,
            Announce,
            UpdateCheck,
            AutoBattle(AccountIdent),
            AutoLure(AccountIdent),
            AutoFortress(AccountIdent),
            FortressMaintain(AccountIdent),
            GuildAuto(AccountIdent),
            AutoDungeon(AccountIdent),
            PetFeed(AccountIdent),
            Idle(AccountIdent),
            Underworld(AccountIdent),
            Guard(AccountIdent),
            Wheel(AccountIdent),
            Dailies(AccountIdent),
            Dice(AccountIdent),
            Shop(AccountIdent),
            Attr(AccountIdent),
            Expedition(AccountIdent),
            GuildChat(AccountIdent),
            SSOCheck(SSOProvider),
            Crawling(usize, ServerID),
        }

        let mut subs = vec![resized];
        // Ask for a newer version every few hours
        subs.push(subscription::unfold(
            SubIdent::UpdateCheck,
            (),
            move |()| async move {
                sleep(crate::update::CHECK_EVERY).await;
                (Message::UpdateCheck, ())
            },
        ));
        if self.config.admin_unlocked
            && (self.config.announce_enabled || self.config.discord_daily)
        {
            subs.push(subscription::unfold(
                SubIdent::Announce,
                (),
                move |()| async move {
                    sleep(Duration::from_secs(300)).await;
                    (Message::AnnounceTick, ())
                },
            ));
        }
        let subscription = subscription::unfold(
            SubIdent::RefreshUI,
            (),
            move |a: ()| async move {
                sleep(Duration::from_millis(200)).await;
                (Message::UIActive, a)
            },
        );
        subs.push(subscription);

        // The game does not push chat messages, so the guild gets re-read on
        // a timer. The chat sits in the corner of every page now, so this
        // cannot only run on the guild tab any more - it just asks less
        // often when that tab is not the one being looked at
        let viewed = match &self.current_view {
            View::Account { ident, page } if !self.paused => {
                let ident = *ident;
                let every = if *page == AccountPage::Guild { 15 } else { 60 };
                subs.push(subscription::unfold(
                    SubIdent::GuildChat(ident),
                    (),
                    move |()| async move {
                        sleep(Duration::from_secs(every)).await;
                        (Message::GuildRefresh { ident }, ())
                    },
                ));
                Some(ident)
            }
            _ => None,
        };
        // The greeting watches the chat for newcomers, so while it is on
        // every owner character keeps reading its guild once a minute even
        // when another tab is open - otherwise a hello could wait for the
        // ten-minute sign-up job
        if !self.paused
            && self.config.admin_unlocked
            && (self.config.greet_enabled || self.config.farewell_enabled)
        {
            for acc in self.servers.0.values().flat_map(|s| s.accounts.values())
            {
                if Some(acc.ident) == viewed || !admin::is_owner(&acc.name) {
                    continue;
                }
                let ident = acc.ident;
                subs.push(subscription::unfold(
                    SubIdent::GuildChat(ident),
                    (),
                    move |()| async move {
                        sleep(Duration::from_secs(60)).await;
                        (Message::GuildRefresh { ident }, ())
                    },
                ));
            }
        }

        // STOP means the app says nothing to the game at all. The game only
        // keeps one session per character, so a single background poll is
        // enough to throw the player out of their own browser window - which
        // is exactly what STOP is there to prevent. Crawling is not affected;
        // it uses its own throwaway logins, not the character
        for (server_id, server) in &self.servers.0 {
            for acc in server.accounts.values() {
                if self.paused {
                    continue;
                }
                // **There is no background poll any more.** It sent an
                // `Update` per character every minute and a half, day and
                // night - more than everything else the app sends put
                // together, straight through the quiet hours, and the one
                // pattern in the whole app with a metronome in it. Nothing
                // was gained by it either: every command the automation
                // sends comes back with a fresh game state, so the panels
                // follow the bot's own work. Between sessions they show
                // what was true when it last did something, which is the
                // honest answer rather than a request bought to hide it

                if let Some(si) = &acc.scrapbook_info
                    && si.auto_battle
                {
                    let subscription = subscription::unfold(
                        SubIdent::AutoBattle(acc.ident),
                        AutoAttackChecker {
                            player_status: acc.status.clone(),
                            ident: acc.ident,
                        },
                        move |a: AutoAttackChecker| async move {
                            (a.check().await, a)
                        },
                    );
                    subs.push(subscription);
                };

                if let Some(fi) = &acc.fortress_info
                    && fi.auto_attack
                {
                    let subscription = subscription::unfold(
                        SubIdent::AutoFortress(acc.ident),
                        AutoFortressChecker { ident: acc.ident },
                        move |a: AutoFortressChecker| async move {
                            (a.check().await, a)
                        },
                    );
                    subs.push(subscription);
                }

                if acc.fortress_info.as_ref().is_some_and(|a| a.maintains()) {
                    subs.push(subscription::unfold(
                        SubIdent::FortressMaintain(acc.ident),
                        FortressMaintainChecker {
                            ident: acc.ident,
                            player_status: acc.status.clone(),
                            per_day: self.config.fortress_visits_per_day,
                        },
                        move |a: FortressMaintainChecker| async move {
                            (a.check().await, a)
                        },
                    ));
                }

                if acc.dungeon_info.auto {
                    subs.push(subscription::unfold(
                        SubIdent::AutoDungeon(acc.ident),
                        AutoDungeonChecker {
                            player_status: acc.status.clone(),
                            ident: acc.ident,
                        },
                        move |a: AutoDungeonChecker| async move {
                            (a.check().await, a)
                        },
                    ));
                }

                if acc.expedition_info.auto {
                    subs.push(subscription::unfold(
                        SubIdent::Expedition(acc.ident),
                        ExpeditionChecker {
                            player_status: acc.status.clone(),
                            ident: acc.ident,
                        },
                        move |a: ExpeditionChecker| async move {
                            (a.check().await, a)
                        },
                    ));
                }

                if acc.shop_info.auto {
                    subs.push(subscription::unfold(
                        SubIdent::Shop(acc.ident),
                        ShopChecker { ident: acc.ident },
                        move |a: ShopChecker| async move { (a.check().await, a) },
                    ));
                }
                if acc.shop_info.attr_auto {
                    subs.push(subscription::unfold(
                        SubIdent::Attr(acc.ident),
                        AttrChecker { ident: acc.ident },
                        move |a: AttrChecker| async move { (a.check().await, a) },
                    ));
                }

                if acc.tavern_info.wheel_auto {
                    subs.push(subscription::unfold(
                        SubIdent::Wheel(acc.ident),
                        WheelChecker {
                            player_status: acc.status.clone(),
                            ident: acc.ident,
                        },
                        move |a: WheelChecker| async move {
                            (a.check().await, a)
                        },
                    ));
                }

                if acc.tavern_info.dice_auto {
                    subs.push(subscription::unfold(
                        SubIdent::Dice(acc.ident),
                        DiceChecker { ident: acc.ident },
                        move |a: DiceChecker| async move {
                            (a.check().await, a)
                        },
                    ));
                }

                if acc.dailies_info.auto {
                    subs.push(subscription::unfold(
                        SubIdent::Dailies(acc.ident),
                        DailiesChecker { ident: acc.ident },
                        move |a: DailiesChecker| async move {
                            (a.check().await, a)
                        },
                    ));
                }

                if acc.guard_info.auto {
                    subs.push(subscription::unfold(
                        SubIdent::Guard(acc.ident),
                        GuardChecker {
                            player_status: acc.status.clone(),
                            ident: acc.ident,
                        },
                        move |a: GuardChecker| async move {
                            (a.check().await, a)
                        },
                    ));
                }

                if acc.idle_info.auto_upgrade
                    || acc.idle_info.auto_collect
                    || acc.idle_info.auto_sacrifice
                {
                    subs.push(subscription::unfold(
                        SubIdent::Idle(acc.ident),
                        IdleChecker { ident: acc.ident },
                        move |a: IdleChecker| async move {
                            (a.check().await, a)
                        },
                    ));
                }

                if acc
                    .underworld_info
                    .as_ref()
                    .is_some_and(|u| u.auto_collect || u.auto_build)
                {
                    subs.push(subscription::unfold(
                        SubIdent::Underworld(acc.ident),
                        UnderworldChecker { ident: acc.ident },
                        move |a: UnderworldChecker| async move {
                            (a.check().await, a)
                        },
                    ));
                }

                if acc.pet_info.auto_feed {
                    subs.push(subscription::unfold(
                        SubIdent::PetFeed(acc.ident),
                        PetFeedChecker { ident: acc.ident },
                        move |a: PetFeedChecker| async move {
                            (a.check().await, a)
                        },
                    ));
                }

                if acc.social.auto_attack
                    || acc.social.auto_defense
                    || acc.social.raid_auto
                {
                    subs.push(subscription::unfold(
                        SubIdent::GuildAuto(acc.ident),
                        GuildAutoChecker { ident: acc.ident },
                        move |a: GuildAutoChecker| async move {
                            (a.check().await, a)
                        },
                    ));
                }

                if let Some(ui) = &acc.underworld_info
                    && ui.auto_lure
                {
                    let subscription = subscription::unfold(
                        SubIdent::AutoLure(acc.ident),
                        AutoLureChecker {
                            player_status: acc.status.clone(),
                            ident: acc.ident,
                        },
                        move |a: AutoLureChecker| async move {
                            (a.check().await, a)
                        },
                    );
                    subs.push(subscription);
                }
            }

            if let CrawlingStatus::Crawling {
                crawling_session,
                threads,
                que,
                ..
            } = &server.crawling
            {
                let Some(session) = crawling_session else {
                    continue;
                };
                for thread in 0..*threads {
                    let subscription = subscription::unfold(
                        SubIdent::Crawling(thread, server.ident.id),
                        Crawler {
                            que: que.clone(),
                            state: session.clone(),
                            server_id: *server_id,
                        },
                        move |mut a: Crawler| async move { (a.crawl().await, a) },
                    );
                    subs.push(subscription);
                }
            }
        }

        for (arc, prov) in [
            (&self.login_state.steam_sso, SSOProvider::Steam),
            (&self.login_state.google_sso, SSOProvider::Google),
        ] {
            let arc = arc.clone();
            let subscription = subscription::unfold(
                SubIdent::SSOCheck(prov),
                SSOValidator {
                    status: arc,
                    provider: prov,
                },
                move |a: SSOValidator| async move {
                    let msg = match a.check().await {
                        Ok(Some((chars, name))) => {
                            let chars = chars.into_iter().flatten().collect();
                            Message::SSOSuccess {
                                auth_name: name,
                                chars,
                                provider: prov,
                            }
                        }
                        Ok(None) => Message::SSORetry,
                        Err(e) => Message::SSOAuthError {
                            _error: e.to_string(),
                        },
                    };

                    (msg, a)
                },
            );
            subs.push(subscription);
        }

        Subscription::batch(subs)
    }
}

impl Helper {
    fn force_init_crawling(
        &mut self,
        url: &str,
        threads: usize,
        pb: ProgressBar,
    ) -> Option<Command<Message>> {
        let ident = ServerIdent::new(url);
        let connection = ServerConnection::new(url)?;
        pb.enable_steady_tick(Duration::from_millis(30));
        pb.set_prefix(ident.ident.to_string());
        set_full_bar(&pb, "Crawling", 0);
        let server = self.servers.get_or_insert_default(
            ident,
            connection,
            Some(pb.clone()),
        );

        let que_id = QueID::new();

        let que = WorkerQue {
            que_id,
            todo_pages: Default::default(),
            todo_accounts: Default::default(),
            invalid_pages: Default::default(),
            invalid_accounts: Default::default(),
            in_flight_pages: Default::default(),
            in_flight_accounts: Default::default(),
            order: Default::default(),
            lvl_skipped_accounts: Default::default(),
            min_level: Default::default(),
            max_level: 9999,
            self_init: true,
        };

        server.crawling = CrawlingStatus::Crawling {
            que_id,
            threads: 0,
            que: Arc::new(Mutex::new(que)),
            player_info: Default::default(),
            equipment: Default::default(),
            naked: Default::default(),
            last_update: Local::now(),
            crawling_session: None,
            recent_failures: Default::default(),
        };
        Some(server.set_threads(threads, &self.config.base_name))
    }

    fn has_accounts(&self) -> bool {
        self.servers.0.iter().any(|a| !a.1.accounts.is_empty())
    }

    fn update_best(
        &mut self,
        ident: AccountIdent,
        keep_recent: bool,
    ) -> Command<Message> {
        trace!("Updating best for {ident:?} - keep recent: {keep_recent}");
        let Some(server) = self.servers.get_mut(&ident.server_id) else {
            return Command::none();
        };

        let CrawlingStatus::Crawling {
            que,
            threads,
            player_info,
            equipment,
            naked,
            ..
        } = &mut server.crawling
        else {
            return Command::none();
        };

        let Some(account) = server.accounts.get_mut(&ident.account) else {
            return Command::none();
        };

        if keep_recent
            && account.last_updated + Duration::from_millis(500) >= Local::now()
        {
            return Command::none();
        }

        let mut has_old = false;

        let mut lock = que.lock_safe();
        let invalid =
            lock.invalid_accounts.iter().map(|a| a.as_str()).collect();

        let result_limit = 100;

        if let Some(si) = &mut account.scrapbook_info {
            let per_player_counts = calc_per_player_count(
                player_info, equipment, &si.scrapbook.items, si,
                self.config.blacklist_threshold,
            );
            si.best = find_best(
                &per_player_counts, player_info, result_limit, &invalid,
                si.order, si.own_stats,
            );

            // An outdated plan is worse than no plan, so it only survives, as
            // long as every target in it is still a valid target
            if !si.plan.is_empty() {
                let still_valid = si.plan.iter().all(|t| {
                    per_player_counts.contains_key(&t.info.uid)
                        && !invalid.contains(t.info.name.as_str())
                });
                if !still_valid {
                    si.plan.clear();
                }
            }

            for target in &si.best {
                if target.is_old()
                    && !lock.todo_accounts.contains(&target.info.name)
                    && !lock.invalid_accounts.contains(&target.info.name)
                    && !lock.in_flight_accounts.contains(&target.info.name)
                {
                    has_old = true;
                    lock.todo_accounts.push(target.info.name.to_string())
                }
            }
        };

        if let Some(ui) = &mut account.underworld_info {
            ui.best.clear();
            'a: for (_, players) in naked.range(..=ui.max_level).rev() {
                for player in players.iter() {
                    if ui.best.len() >= result_limit {
                        break 'a;
                    }
                    let Some(info) = player_info.get(player) else {
                        continue;
                    };
                    if info.is_old()
                        && !lock.todo_accounts.contains(&info.name)
                        && !lock.invalid_accounts.contains(&info.name)
                        && !lock.in_flight_accounts.contains(&info.name)
                    {
                        has_old = true;
                        lock.todo_accounts.push(info.name.to_string())
                    }
                    ui.best.push(info.to_owned());
                }
            }
        }
        drop(lock);

        account.last_updated = Local::now();

        if (has_old || player_info.is_empty()) && *threads == 0 {
            return server.set_threads(1, &self.config.base_name);
        }
        Command::none()
    }
}

pub fn calc_per_player_count(
    player_info: &HashMap<
        u32,
        CharacterInfo,
        std::hash::BuildHasherDefault<nohash_hasher::NoHashHasher<u32>>,
    >,
    equipment: &HashMap<
        EquipmentIdent,
        HashSet<u32, ahash::RandomState>,
        ahash::RandomState,
    >,
    scrapbook: &HashSet<EquipmentIdent>,
    si: &ScrapbookInfo,
    blacklist_th: usize,
) -> IntMap<u32, usize> {
    let mut per_player_counts = IntMap::default();
    per_player_counts.reserve(player_info.len());

    for (eq, players) in equipment.iter() {
        if scrapbook.contains(eq) || eq.model_id >= 100 {
            continue;
        }
        for player in players.iter() {
            *per_player_counts.entry(*player).or_insert(0) += 1;
        }
    }

    per_player_counts.retain(|a, _| {
        let Some(info) = player_info.get(a) else {
            return false;
        };

        if info.level > si.max_level {
            return false;
        }

        if info.stats.unwrap_or_default() > si.max_attributes {
            return false;
        }

        if let Some((_, lost)) = si.blacklist.get(&info.uid)
            && *lost >= blacklist_th.max(1)
        {
            return false;
        }
        true
    });
    per_player_counts
}

macro_rules! impl_unique_id {
    ($type:ty) => {
        impl $type {
            fn new() -> Self {
                static COUNTER: AtomicU64 = AtomicU64::new(0);
                Self(COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst))
            }
        }
    };
}

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Copy, Clone)]
pub struct ServerID(u64);

impl ServerID {
    /// The number itself, for the settings file
    #[must_use]
    pub fn raw(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for ServerID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("server-{}", self.0))
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Copy, Clone)]
pub struct QueID(u64);
impl_unique_id!(QueID);

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Copy, Clone)]
pub struct AccountID(u64);
impl_unique_id!(AccountID);

impl std::fmt::Display for AccountID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("character-{}", self.0))
    }
}

/// (today, this week) from the install counter, or why it could not be read
pub type UserCount = Result<(Option<u64>, Option<u64>), String>;

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Copy, Clone)]
pub struct AccountIdent {
    server_id: ServerID,
    account: AccountID,
}

impl std::fmt::Display for AccountIdent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "character-{}@{}",
            self.account.0, self.server_id.0
        ))
    }
}

impl Helper {
    /// The auto-logins of every saved account the handover did not bring
    /// along, spaced 200 ms apart. Returns how many there are. Used at
    /// start and again after a config recovery
    pub fn saved_logins(
        &self,
        covered: &[handover::AccountKey],
    ) -> (u64, Vec<Command<Message>>) {
        let mut loading = 0;
        let mut commands = Vec::new();
        for acc in &self.config.accounts {
            if covered.iter().any(|k| k.covers(acc)) {
                continue;
            }
            let wants_login = match acc {
                AccountConfig::Regular { config, .. } => config.login,
                AccountConfig::SF { characters, .. } => {
                    characters.iter().any(|a| a.config.login)
                }
            };
            if !wants_login {
                continue;
            }
            let acc = acc.clone();
            loading += 1;
            commands.push(Command::perform(
                async move {
                    sleep(Duration::from_millis((loading - 1) * 200)).await
                },
                move |_| Message::Login {
                    account: acc,
                    auto_login: true,
                },
            ));
        }
        (loading, commands)
    }

    /// `<server code>#<name>` - the key the on-disk ledgers use for a
    /// character, stable across restarts (an `AccountIdent` is not)
    pub fn char_key(&self, ident: &AccountIdent) -> Option<String> {
        let (server, account) = self.servers.get_ident(ident)?;
        Some(probe_history::ProbeHistory::key(
            &server.ident.url, &account.name,
        ))
    }
}

impl ServerInfo {
    pub fn set_threads(
        &mut self,
        new_count: usize,
        base_name: &str,
    ) -> Command<Message> {
        let CrawlingStatus::Crawling {
            threads,
            crawling_session,
            ..
        } = &mut self.crawling
        else {
            return Command::none();
        };

        let not_logged_in = *threads == 0 && crawling_session.is_none();

        *threads = new_count;

        let base_name = base_name.to_string();
        let con = self.connection.clone();
        let id = self.ident.id;

        if not_logged_in {
            Command::perform(
                CrawlerState::try_login(base_name, con),
                move |res| match res {
                    Ok(state) => Message::CrawlerStartup {
                        server: id,
                        state: Arc::new(state),
                    },
                    Err(err) => Message::CrawlerDied {
                        server: id,
                        error: err.to_string(),
                    },
                },
            )
        } else {
            Command::none()
        }
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct AttackTarget {
    missing: usize,
    info: CharacterInfo,
}
impl AttackTarget {
    fn is_old(&self) -> bool {
        self.info.is_old()
    }

    /// One of the game's own arena opponents. Nothing to gain for the
    /// scrapbook - it is here so a free fight is never thrown away when the
    /// crawler has nobody
    pub fn from_offer(o: &crate::player::ArenaOpponent) -> Self {
        AttackTarget {
            missing: 0,
            info: CharacterInfo {
                equipment: vec![],
                name: o.name.clone(),
                uid: o.pid,
                level: o.level,
                stats: Some(o.stats),
                fetch_date: Some(chrono::Local::now().date_naive()),
                class: Some(o.class),
                no_guild: o.guild.is_none(),
            },
        }
    }
}

/// The sorting key of a target. Bigger is better, so that we can keep the
/// `max_out` best targets in a min-heap without having to sort millions of
/// players
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
struct TargetKey {
    /// Whatever the selected order optimizes for
    primary: i64,
    /// Weaker opponents win ties
    inv_stats: i64,
    /// Then lower levels
    inv_level: i64,
    /// Purely so that the result is stable between runs
    uid: u32,
}

fn target_key(
    order: TargetOrder,
    missing: usize,
    info: &CharacterInfo,
    own_stats: u32,
) -> TargetKey {
    let stats = info.stats.unwrap_or(u32::MAX / 2) as i64;
    let primary = match order {
        TargetOrder::MostItems => missing as i64,
        TargetOrder::BestValue => {
            // How dangerous the opponent is compared to us. Everything below
            // half of our stats is considered equally harmless, so that we do
            // not chase completely irrelevant characters
            let own = own_stats.max(1) as f64;
            let danger = (stats as f64 / own).max(0.5);
            ((missing as f64 / danger) * 1000.0) as i64
        }
        TargetOrder::Weakest => -stats,
    };

    TargetKey {
        primary,
        inv_stats: -stats,
        inv_level: -(info.level as i64),
        uid: info.uid,
    }
}

fn find_best(
    per_player_counts: &IntMap<u32, usize>,
    player_info: &IntMap<u32, CharacterInfo>,
    max_out: usize,
    invalid: &HashSet<&str>,
    order: TargetOrder,
    own_stats: u32,
) -> Vec<AttackTarget> {
    if max_out == 0 {
        return Vec::new();
    }

    // Keep only the `max_out` best players in a min-heap. That way we never
    // have to sort (or even copy) the hundreds of thousands of characters a
    // full HoF contains, and - unlike the old bucket approach - the amount of
    // missing items stays exact, even above 10
    let mut heap: BinaryHeap<Reverse<(TargetKey, u32, usize)>> =
        BinaryHeap::with_capacity(max_out + 1);

    for (player, count) in per_player_counts.iter().map(|a| (*a.0, *a.1)) {
        if count == 0 {
            continue;
        }
        let Some(info) = player_info.get(&player) else {
            continue;
        };
        if invalid.contains(info.name.as_str()) {
            continue;
        }
        let key = target_key(order, count, info, own_stats);

        if heap.len() == max_out {
            // Cheap early exit for the overwhelming majority of players
            match heap.peek() {
                Some(Reverse((worst, _, _))) if *worst >= key => continue,
                _ => {
                    heap.pop();
                }
            }
        }
        heap.push(Reverse((key, player, count)));
    }

    let mut best: Vec<_> = heap.into_iter().map(|a| a.0).collect();
    // Descending, so that the best target is the first one
    best.sort_unstable_by_key(|a| std::cmp::Reverse(a.0));

    best.into_iter()
        .filter_map(|(_, player, count)| {
            player_info.get(&player).map(|info| AttackTarget {
                missing: count,
                info: info.to_owned(),
            })
        })
        .collect()
}

/// Greedily builds a whole sequence of fights instead of just looking at the
/// single best target. After every simulated win, the items of that target are
/// treated as collected, which means the next target is picked by how many
/// items it adds *on top of* the previous ones. Without this, the top of the
/// list is usually a bunch of characters, that all hold the very same items
pub fn greedy_battle_plan(
    player_info: &IntMap<u32, CharacterInfo>,
    equipment: &HashMap<
        EquipmentIdent,
        HashSet<u32, ahash::RandomState>,
        ahash::RandomState,
    >,
    si: &ScrapbookInfo,
    blacklist_th: usize,
    invalid: &HashSet<&str>,
    max_steps: usize,
) -> Vec<AttackTarget> {
    let mut scrapbook = si.scrapbook.items.clone();
    let mut per_player_counts = calc_per_player_count(
        player_info, equipment, &scrapbook, si, blacklist_th,
    );

    let mut plan: Vec<AttackTarget> = Vec::new();
    let mut used: IntSet<u32> = Default::default();

    for _ in 0..max_steps {
        let Some(target) = find_best(
            &per_player_counts, player_info, 1, invalid, si.order, si.own_stats,
        )
        .into_iter()
        .next() else {
            break;
        };
        if target.missing == 0 || !used.insert(target.info.uid) {
            break;
        }

        for eq in &target.info.equipment {
            if eq.model_id >= 100 || !scrapbook.insert(*eq) {
                // Either not collectable, or we already have it
                continue;
            }
            let Some(players) = equipment.get(eq) else {
                continue;
            };
            // Everybody, that only carried items we just "found", is worthless
            // for the next step
            for player in players {
                if let Some(ppc) = per_player_counts.get_mut(player) {
                    *ppc = ppc.saturating_sub(1);
                }
            }
        }
        per_player_counts.remove(&target.info.uid);
        plan.push(target);
    }
    plan
}

fn top_bar(
    center: Element<Message>,
    back: Option<Message>,
) -> Element<Message> {
    let back_button: Element<Message> = if let Some(back) = back {
        button(text(i18n::t("Back")).size(12))
            .padding([4, 9])
            .style(ui::style::brass(false))
            .on_press(back)
            .into()
    } else {
        text("").into()
    };

    let back_button = container(back_button).width(Length::Fixed(100.0));

    let settings = container(
        row!(
            button(text(i18n::t("Settings")).size(12))
                .padding([4, 9])
                .style(ui::style::brass(false))
                .on_press(Message::ViewSettings),
            ui::coffee_icon(),
        )
        .spacing(6)
        .align_items(Alignment::Center),
    )
    .width(Length::Fixed(130.0))
    .align_x(iced::alignment::Horizontal::Right);

    // The same black strip the account pages wear, so stepping out to the
    // overview is not stepping into a different application
    container(
        row!(
            back_button,
            horizontal_space(),
            center,
            horizontal_space(),
            settings
        )
        .align_items(Alignment::Center)
        .padding([8, 14]),
    )
    .width(Length::Fill)
    .style(ui::style::frame())
    .into()
}

pub fn handle_new_char_info(
    char: CharacterInfo,
    equipment: &mut HashMap<
        EquipmentIdent,
        HashSet<u32, ahash::RandomState>,
        ahash::RandomState,
    >,
    player_info: &mut IntMap<u32, CharacterInfo>,
    naked: &mut BTreeMap<u16, IntSet<u32>>,
) {
    let player_entry = player_info.entry(char.uid);

    const EQ_CUTOFF: usize = 4;

    match player_entry {
        Entry::Occupied(mut old) => {
            // We have already seen this player. We have to remove the old info
            // and add the updated info
            let old_info = old.get();
            for eq in &old_info.equipment {
                if let Some(x) = equipment.get_mut(eq) {
                    x.remove(&old_info.uid);
                }
            }
            for eq in char.equipment.clone() {
                equipment
                    .entry(eq)
                    .and_modify(|a| {
                        a.insert(char.uid);
                    })
                    .or_insert_with(|| HashSet::from_iter([char.uid]));
            }
            if old_info.equipment.len() < EQ_CUTOFF {
                naked.entry(old_info.level).and_modify(|a| {
                    a.remove(&old_info.uid);
                });
            }

            if char.equipment.len() < EQ_CUTOFF {
                naked.entry(char.level).or_default().insert(char.uid);
            }
            old.insert(char);
        }
        Entry::Vacant(v) => {
            for eq in char.equipment.clone() {
                equipment
                    .entry(eq)
                    .and_modify(|a| {
                        a.insert(char.uid);
                    })
                    .or_insert_with(|| HashSet::from_iter([char.uid]));
            }
            if char.equipment.len() < EQ_CUTOFF && char.level >= 100 {
                naked.entry(char.level).or_default().insert(char.uid);
            }
            v.insert(char);
        }
    }
}

fn get_log_config(is_headless: bool) -> log4rs::Config {
    let pattern = PatternEncoder::new(
        "{d(%Y-%m-%d %H:%M:%S)} | {({l}):5.5} | {M}:{L} | {m}{n}",
    );
    let stderr = ConsoleAppender::builder()
        .target(Target::Stderr)
        .encoder(Box::new(pattern.clone()))
        .build();

    // Rolled at 20 MB into helper.1.log .. helper.3.log, the oldest
    // dropped. It used to be one file that only ever grew: 650 MB, most of
    // it the crawler logging in again every few seconds. An old oversized
    // helper.log is rolled away on the first write
    let log_path = app_path("helper.log");
    let roll_pattern = app_path("helper.{}.log");
    let logfile = FixedWindowRoller::builder()
        .base(1)
        .build(&roll_pattern.to_string_lossy(), 3)
        .ok()
        .and_then(|roller| {
            RollingFileAppender::builder()
                .encoder(Box::new(pattern.clone()))
                .build(
                    &log_path,
                    Box::new(CompoundPolicy::new(
                        Box::new(SizeTrigger::new(20 * 1024 * 1024)),
                        Box::new(roller),
                    )),
                )
                .ok()
        });
    let logfile: Box<dyn log4rs::append::Append> = match logfile {
        Some(rolling) => Box::new(rolling),
        None => Box::new(
            FileAppender::builder()
                .encoder(Box::new(pattern.clone()))
                .build("helper.log")
                .unwrap(),
        ),
    };

    let mut logger = log4rs::Config::builder()
        .appender(Appender::builder().build("logfile", logfile));
    let mut root = Root::builder();

    if !is_headless {
        logger = logger
            .appender(Appender::builder().build("stderr", Box::new(stderr)));
        root = root.appender("stderr");
    }

    logger
        // The crate is called sf-commander, so its log target is
        // "sf_commander". It used to say "sf_scrapbook_helper" - the name
        // before the fork - so the filter never matched anything and not a
        // single line the app writes about itself ended up in the file
        .logger(
            Logger::builder()
                .appender("logfile")
                .build("sf_commander", log::LevelFilter::Info),
        )
        // The library warns about every field of the game protocol it does
        // not parse. That is dozens of lines per update and it says nothing
        // about what went wrong here, so only real errors are kept
        .logger(
            Logger::builder()
                .appender("logfile")
                .build("sf_api", log::LevelFilter::Error),
        )
        .build(root.build(log::LevelFilter::Error))
        .unwrap()
}

pub fn set_full_bar(bar: &ProgressBar, title: &str, length: usize) {
    let style = ProgressStyle::default_spinner()
        .template(
            "{spinner} {prefix:17.red} - {msg:25.blue} {wide_bar:.green} \
             [{elapsed_precise}/{duration_precise}] [{pos:6}/{len:6}]",
        )
        .unwrap_or_else(|_| ProgressStyle::default_spinner());

    bar.set_style(style);
    bar.reset_elapsed();
    bar.set_message(title.to_string());
    bar.set_length(length as u64);
    bar.set_position(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn char_info(uid: u32, level: u16, stats: u32) -> CharacterInfo {
        CharacterInfo {
            equipment: vec![],
            name: format!("player{uid}"),
            uid,
            level,
            stats: Some(stats),
            fetch_date: Some(Utc::now().date_naive()),
            class: None,
            no_guild: false,
        }
    }

    fn setup() -> (IntMap<u32, usize>, IntMap<u32, CharacterInfo>) {
        let mut counts: IntMap<u32, usize> = Default::default();
        let mut info: IntMap<u32, CharacterInfo> = Default::default();
        // uid, missing items, level, attributes
        for (uid, missing, level, stats) in [
            (1, 25, 400, 900_000),
            (2, 12, 300, 200_000),
            (3, 12, 250, 100_000),
            (4, 1, 100, 10_000),
            (5, 0, 100, 10_000),
        ] {
            counts.insert(uid, missing);
            info.insert(uid, char_info(uid, level, stats));
        }
        (counts, info)
    }

    #[test]
    fn exact_missing_count_above_ten() {
        let (counts, info) = setup();
        let best = find_best(
            &counts,
            &info,
            10,
            &HashSet::new(),
            TargetOrder::MostItems,
            500_000,
        );
        // The old bucket based implementation clamped this to 10
        assert_eq!(best[0].missing, 25);
        assert_eq!(best[0].info.uid, 1);
        // Same amount of items -> the weaker one has to come first
        assert_eq!(best[1].info.uid, 3);
        assert_eq!(best[2].info.uid, 2);
        // Players without any new item are never a target
        assert!(best.iter().all(|a| a.info.uid != 5));
    }

    #[test]
    fn top_k_is_respected() {
        let (counts, info) = setup();
        let best = find_best(
            &counts,
            &info,
            2,
            &HashSet::new(),
            TargetOrder::MostItems,
            500_000,
        );
        assert_eq!(best.len(), 2);
        assert_eq!(best[0].info.uid, 1);
        assert_eq!(best[1].info.uid, 3);
    }

    #[test]
    fn value_order_avoids_the_dangerous_target() {
        let (counts, info) = setup();
        let best = find_best(
            &counts,
            &info,
            10,
            &HashSet::new(),
            TargetOrder::BestValue,
            500_000,
        );
        // 12 items against a much weaker opponent beat 25 items against
        // somebody, that is almost twice as strong as we are
        assert_eq!(best[0].info.uid, 3);
    }

    #[test]
    fn weakest_order_picks_the_weakest_with_items() {
        let (counts, info) = setup();
        let best = find_best(
            &counts,
            &info,
            10,
            &HashSet::new(),
            TargetOrder::Weakest,
            500_000,
        );
        assert_eq!(best[0].info.uid, 4);
    }

    #[test]
    fn invalid_accounts_are_skipped() {
        let (counts, info) = setup();
        let mut invalid = HashSet::new();
        invalid.insert("player1");
        let best = find_best(
            &counts,
            &info,
            10,
            &invalid,
            TargetOrder::MostItems,
            500_000,
        );
        assert!(best.iter().all(|a| a.info.uid != 1));
    }

    #[test]
    fn app_icon_is_loadable() {
        // The icon is loaded at startup - a broken file would only show up
        // as a missing window icon, so it gets checked here instead
        let raw = include_bytes!("../assets/icon.ico");
        let img =
            image::load_from_memory_with_format(raw, image::ImageFormat::Ico)
                .expect("icon.ico has to be a valid ico");
        assert!(img.width() >= 64 && img.height() >= 64);
    }

    /// The live feed hides a line only for the character whose success was
    /// just written - never somebody else's, never an old one
    #[test]
    fn a_success_written_just_now_marks_the_console_line_a_duplicate() {
        let line = |who: &str, ago_ms: i64| ConsoleLine {
            at: Local::now() - chrono::Duration::milliseconds(ago_ms),
            who: who.into(),
            text: "won".into(),
            ok: Some(true),
            dup: false,
        };
        let mut successes = std::collections::VecDeque::new();
        assert!(!ConsoleLine::just_succeeded(&successes, "Hero"));
        successes.push_back(line("Hero", 0));
        assert!(ConsoleLine::just_succeeded(&successes, "hero"));
        assert!(!ConsoleLine::just_succeeded(&successes, "Other"));
        successes.push_back(line("Hero", 5000));
        assert!(!ConsoleLine::just_succeeded(&successes, "Hero"));
    }

    #[test]
    fn retry_delay_grows_and_is_capped() {
        let first = retry_delay(0);
        let later = retry_delay(4);
        assert!(first < later);
        assert!(retry_delay(50) <= std::time::Duration::from_secs(451));
    }
}
