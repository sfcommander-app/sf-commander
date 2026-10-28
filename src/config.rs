use iced::Theme;
use num_format::CustomFormat;
use serde::{Deserialize, Serialize};
use sf_api::session::PWHash;

use crate::{ServerID, server::ServerIdent};

/// How the app plays, derived from the one slider (`Config::safety`) -
/// never set by hand any more. Kept as a type because a fair amount of
/// code asks "are we in sessions at all"
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum SessionsLevel {
    Off,
    NoShame,     // No care about stealth
    Normal,      // Balanced
    FullStealth, // Maximum stealth
}

impl SessionsLevel {
    pub fn label(self) -> &'static str {
        match self {
            SessionsLevel::Off => "Off",
            SessionsLevel::NoShame => "No Shame",
            SessionsLevel::Normal => "Normal",
            SessionsLevel::FullStealth => "Full Stealth",
        }
    }
}

impl std::fmt::Display for SessionsLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

impl Default for SessionsLevel {
    fn default() -> Self {
        SessionsLevel::Off
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub accounts: Vec<AccountConfig>,
    pub theme: AvailableTheme,
    pub base_name: String,
    pub auto_fetch_newest: bool,
    /// Dead since the background poll was removed. It sent an `Update`
    /// per character every minute and a half, day and night - more than
    /// everything else the app sends put together, straight through the
    /// quiet hours - and bought nothing: every command the automation
    /// sends comes back with a fresh game state, so the panels follow the
    /// bot's own work. Kept only so an old `helper.toml` still loads
    #[serde(default)]
    #[allow(dead_code)]
    pub auto_poll: bool,
    /// Play a sound on a level up, a cleared dungeon, a finished tavern and
    /// a sent guild message. Off unless switched on - the bot runs all night
    #[serde(default)]
    pub sounds: bool,
    /// Write the guild chat to a text file on the desktop as it happens,
    /// one file per guild (see `archive`). Off unless switched on
    #[serde(default)]
    pub chat_archive: bool,
    /// The window follows the bot: whenever a job goes out, the page (and
    /// the character, with several logged in) it belongs to is shown - the
    /// dungeon page for a dungeon fight, the tavern for an expedition. The
    /// mouse stays the player's; only what is shown changes, and only while
    /// a character page is open (Settings and the overview stay put)
    #[serde(default = "default_true")]
    pub follow_bot: bool,
    /// A random number this install made up for itself on first start. It is
    /// sent with the update check so the author can see how many people use
    /// the app - nothing else goes with it: no character, no server, no
    /// login. Clear the line and a new one is drawn
    #[serde(default)]
    pub install_id: String,
    /// The licence code as it was pasted in. Kept verbatim, because what is
    /// stored has to be exactly what was signed
    /// The app in the guild leader's own lettering, or in the machine's.
    /// Off by default since 6.0 - plain type reads better across a whole
    /// screen; the "Aa" button brings the lettering back
    #[serde(default)]
    pub drawn_font: bool,
    /// Set once 6.0 switched every file to the plain type
    #[serde(default)]
    pub font_60_migrated: bool,
    #[serde(default)]
    pub font_aurora_migrated: bool,
    /// The guild's member table in two squeezed columns (fits the screen)
    /// rather than one readable one (scrolls). One since 6.1
    #[serde(default)]
    pub guild_two_columns: bool,
    #[serde(default)]
    pub guild_aurora_migrated: bool,
    /// The guild page's leadership tools (kick advice, officer seats,
    /// marks, grading, raid button) for a character that is neither the
    /// leader nor an officer. Off by default (6.1.2)
    #[serde(default)]
    pub guild_leader_tools: bool,
    /// The explanations under the controls (6.1.4) - off by default
    #[serde(default)]
    pub show_help: bool,
    #[serde(default)]
    pub licence: String,
    /// The latest moment the app has ever seen. A licence is never judged
    /// by a clock earlier than this, so winding the machine's clock back
    /// does not buy a second year (see `licence::effective_now`)
    #[serde(default)]
    pub licence_clock_mark: i64,
    #[serde(default = "default_threads")]
    pub max_threads: usize,
    #[serde(default = "default_start_threads")]
    pub start_threads: usize,
    #[serde(default)]
    pub show_crawling_restrict: bool,
    #[serde(default = "default_class_icons")]
    pub show_class_icons: bool,
    #[serde(default = "default_blacklist_threshhold")]
    pub blacklist_threshold: usize,
    /// How targets are ranked by default for newly logged in characters
    #[serde(default)]
    pub target_order: TargetOrder,
    /// Auto battle disables itself after this many losses in a row.
    /// 0 turns the safety net off
    #[serde(default = "default_auto_battle_max_losses")]
    pub auto_battle_max_losses: usize,
    /// Set once the old default theme has been swapped for the app's own one
    #[serde(default)]
    pub theme_migrated: bool,
    /// Set once the old "60 % minimum" dungeon default and the off-by-default
    /// guild sign-ups have been reset (both were wrong)
    #[serde(default)]
    pub defaults_migrated: bool,
    /// Set once the old "1 crawler thread" default has been raised
    #[serde(default)]
    pub threads_migrated: bool,
    /// Set once auto poll was switched on by default
    #[serde(default)]
    pub poll_migrated: bool,
    /// Set once the crawler was raised from four threads to ten
    #[serde(default)]
    pub threads_ten_migrated: bool,
    /// Set once beer was switched on for every character already in the
    /// file ("beer is free" stays the player's own tick)
    #[serde(default)]
    pub beer_default_migrated: bool,
    /// Set once a session length of 31 (one click on the spinner too many)
    /// was put back to the intended 30
    #[serde(default)]
    pub sessions_v3_migrated: bool,
    /// Set once "sell the replaced piece" was switched off for every
    /// character in the file (it used to be on by default)
    #[serde(default)]
    pub sell_old_off_migrated: bool,
    /// UI language
    #[serde(default)]
    pub lang: crate::i18n::Lang,
    /// Human-like pauses or full speed
    #[serde(default)]
    pub pace: crate::pace::Pace,
    /// Sessions: the queues only run for `burst_minutes` every `burst_hours`,
    /// at whatever pace is selected. Used to be a third pace of its own,
    /// which made "sessions at full power" impossible to say
    #[serde(default)]
    pub sessions_level: SessionsLevel,
    #[serde(default = "default_burst_hours")]
    pub burst_hours: u8,
    /// A session lasts somewhere between these two, drawn anew every time -
    /// a person does not log off after exactly the same half hour
    /// The old fixed length limits. Since 2.0 the length is drawn by
    /// `stealth::session_length_minutes` (3 min to 1-2 h); these stay only
    /// so an old file still loads
    #[serde(default = "default_burst_minutes")]
    pub burst_minutes: u8,
    #[allow(dead_code)]
    #[serde(default = "default_burst_minutes_max")]
    pub burst_minutes_max: u8,
    /// Set once sessions were switched on for everybody: the app decides
    /// for itself when to farm and when to rest (see `stealth`)
    #[serde(default)]
    pub sessions_auto_migrated: bool,
    /// Set once the old "Sessions" pace was split into pace + sessions and
    /// the schedule reset to the intended 30 min every 6 h
    #[serde(default)]
    pub sessions_migrated: bool,
    /// Set once the schedule moved on to 30-60 min every 3 h
    #[serde(default)]
    pub sessions_v2_migrated: bool,
    /// The three controls (pace, sessions level, safety slider) folded
    /// into the one slider, once
    #[serde(default)]
    pub one_slider_migrated: bool,
    /// The slider turned round and shrank to 0-10, once
    #[serde(default)]
    pub levels_migrated: bool,
    /// Quiet hours: a stretch of the night with no traffic at all, shifted
    /// by a fresh three quarters of an hour every day (see `stealth`)
    #[serde(default = "default_true")]
    pub quiet_hours: bool,
    #[serde(default = "default_quiet_from")]
    pub quiet_from: u8,
    #[serde(default = "default_quiet_to")]
    pub quiet_to: u8,
    /// Requests a minute one character may send at human pace
    #[serde(default = "default_requests_per_min")]
    pub requests_per_min: u8,
    /// The one control over how the app plays: 0 to 10, in whole steps.
    ///
    /// **0 is off** - nothing goes out at all. 1 is a short look-in about
    /// three times a day; 10 is a sitting of ordinary length that comes
    /// round again every half hour or so, all day. The far end is not a
    /// trade: an account that plays once and then vanishes for eight hours
    /// misses most of what the game hands out on a clock *and* looks like
    /// nothing a person does, while one that keeps coming back collects
    /// everything and looks exactly like a player. So towards 10 the
    /// visits come round far more often - and no single one of them ever
    /// runs longer than a person's would.
    ///
    /// The quiet hours are their own tick in Settings and this does not
    /// touch them. Neither does it touch the pauses between clicks: there
    /// is no "full power" any more, because sending with no pauses at all
    /// is the one pattern no player has
    #[serde(default = "default_safety")]
    pub safety: u8,
    /// How many officer seats the guild's leadership is built around -
    /// the leader is not one of them. The seats go to whoever contributes
    /// most (see `guild_report::contribution`), so this number decides
    /// how many promotions and demotions the advice comes to
    #[serde(default = "default_officer_seats")]
    pub officer_seats: u8,
    /// Queue order, job keys first to last (see `Job::key`)
    #[serde(default)]
    pub job_order: Vec<String>,
    /// Settings of characters without an account entry (see
    /// `get_char_conf_mut`)
    #[serde(default)]
    pub extra_characters: Vec<ExtraCharacter>,
    /// Set once gem socketing was switched on for everybody (2.1)
    #[serde(default)]
    pub gem_socket_on_migrated: bool,
    /// Set once recycling of the replaced piece was switched on for
    /// everybody (2.1): a raised or epic piece is dismantled for its
    /// materials, the rest sold - the gem comes out first either way
    #[serde(default)]
    pub recycle_on_migrated: bool,
    /// Set once every file was put on the guild leader's day order (2.1):
    /// dailies, dice, tavern, dungeon, fortress, underworld, arena, guild,
    /// shopping, pets, guard
    #[serde(default)]
    pub job_order_v2_migrated: bool,
    /// How harsh the member grades are: 0 lenient .. 100 strict
    #[serde(default = "default_strictness")]
    pub grade_strictness: u8,
    /// Set once the untouched 50 was moved to the 40 the guild's roster
    /// asked for
    #[serde(default)]
    pub strictness_v2_migrated: bool,
    /// Set once 5.1.6 switched room-making in the backpack on for every
    /// character with at least two free slots: a full bag had stopped the
    /// dungeons, the gem mine, the free spin and the daily reward for days
    #[serde(default)]
    pub backpack_516_migrated: bool,
    /// Set once 5.1.6 folded the extra characters together. Their server
    /// id used to change on every start (see `server::stable_hash`), so a
    /// new default entry was added each time and the saved one was never
    /// found again. The newest entry per name is kept and marked as not yet
    /// tied to a server (`server == 0`); the first lookup adopts it
    #[serde(default)]
    pub extras_516_migrated: bool,
    /// Set once 5.2 put every file on the night rest of 03:00-07:00 (it
    /// was 01:00-07:00 and widened by the slider - too much of the day)
    #[serde(default)]
    pub quiet_52_migrated: bool,
    /// How many times a day the fortress is looked after - collect, gems,
    /// build, recruit (3-10). It used to be every three minutes
    #[serde(default = "default_fortress_visits")]
    pub fortress_visits_per_day: u8,
    /// Set once 5.4 switched off "recruit a soldier after every fortress
    /// attack": the soldiers there are are used up first, then the
    /// barracks fill on their own
    #[serde(default)]
    pub train_54_migrated: bool,
    /// How many of the best / the worst members the table marks (0 = none)
    #[serde(default = "default_five")]
    pub guild_top_mark: u8,
    #[serde(default = "default_five")]
    pub guild_bottom_mark: u8,
    /// Owner features unlocked with the admin password
    #[serde(default)]
    pub admin_unlocked: bool,
    /// Periodic guild chat announcement (owner characters only)
    #[serde(default)]
    pub announce_enabled: bool,
    #[serde(default)]
    pub announce_text: String,
    #[serde(default = "default_announce_hours")]
    pub announce_hours: u8,
    #[serde(default)]
    pub announce_last: Option<chrono::DateTime<chrono::Local>>,
    /// Kick members the grading marks as 5 (offline over 48 h and the
    /// rest of the rules), one per guild refresh, from an owner character
    /// that is leader or officer. Behind the password, off by default
    #[serde(default)]
    pub auto_kick: bool,
    /// The Discord invite once a day, at a random time between 10:00 and
    /// midnight, from every owner character (one per guild)
    #[serde(default)]
    pub discord_daily: bool,
    /// The day and time drawn for today's post, and the last day it went
    /// out - so a restart neither repeats it nor redraws the time
    #[serde(default)]
    pub discord_daily_at: Option<chrono::NaiveDateTime>,
    #[serde(default)]
    pub discord_daily_last: Option<chrono::NaiveDate>,
    /// Greet whoever joins the guild, a minute or two after they show up in
    /// the member list - from an owner character, behind the password
    #[serde(default)]
    pub greet_enabled: bool,
    /// What the greeting says. `{name}` is the newcomer's name
    #[serde(default = "default_greet_text")]
    pub greet_text: String,
    /// The same for whoever leaves (or is kicked - the chat does not tell
    /// the two apart)
    #[serde(default)]
    pub farewell_enabled: bool,
    #[serde(default = "default_farewell_text")]
    pub farewell_text: String,

    #[serde(default = "default_locale", skip)]
    pub num_format: CustomFormat,
}

fn default_announce_hours() -> u8 {
    6
}

fn default_quiet_from() -> u8 {
    3
}

fn default_quiet_to() -> u8 {
    7
}

fn default_requests_per_min() -> u8 {
    12
}

fn default_officer_seats() -> u8 {
    crate::guild_report::OFFICER_SEATS
}

fn default_safety() -> u8 {
    5
}

/// The guild leader's own words - what they typed by hand every time
fn default_greet_text() -> String {
    "hello :) and welcome".to_string()
}

fn default_farewell_text() -> String {
    "F".to_string()
}

fn default_threads() -> usize {
    10
}

fn default_start_threads() -> usize {
    // The crawler is what feeds the arena its targets. Four threads means a
    // fifty thousand player Hall of Fame takes days, and until it is read
    // the arena has nobody worth attacking - so it starts at the maximum
    10
}

fn default_locale() -> CustomFormat {
    let mut cfb = CustomFormat::builder();
    cfb = cfb.separator(",");
    cfb.build().unwrap_or_default()
}

fn default_blacklist_threshhold() -> usize {
    2
}

fn default_class_icons() -> bool {
    true
}

fn default_auto_battle_max_losses() -> usize {
    5
}

/// The way possible targets get ranked
#[derive(
    Debug, Serialize, Deserialize, Default, Clone, Copy, PartialEq, Eq, Hash,
)]
pub enum TargetOrder {
    /// The most new items, ties broken by the weakest opponent
    #[default]
    MostItems,
    /// New items relative to how dangerous the opponent is. Great for
    /// unattended auto battling, as it avoids risky fights for a single item
    BestValue,
    /// The weakest opponent, that still has at least one new item
    Weakest,
}

impl TargetOrder {
    pub const ALL: [TargetOrder; 3] =
        [Self::MostItems, Self::BestValue, Self::Weakest];
}

#[allow(clippy::to_string_trait_impl)]
impl ToString for TargetOrder {
    fn to_string(&self) -> String {
        crate::i18n::t(match self {
            TargetOrder::MostItems => "Most items",
            TargetOrder::BestValue => "Best value",
            TargetOrder::Weakest => "Weakest first",
        })
        .to_string()
    }
}

impl Default for Config {
    fn default() -> Self {
        let mut rng = fastrand::Rng::new();
        let mut base_name = rng.alphabetic().to_ascii_uppercase().to_string();
        for _ in 0..rng.u32(6..8) {
            let c = if rng.bool() {
                rng.alphabetic()
            } else {
                rng.digit(10)
            };
            base_name.push(c)
        }

        Self {
            accounts: vec![],
            theme: AvailableTheme::Commander,
            base_name,
            drawn_font: false,
            font_60_migrated: true,
            font_aurora_migrated: true,
            guild_two_columns: true,
            guild_aurora_migrated: true,
            guild_leader_tools: false,
            show_help: false,
            licence: String::new(),
            licence_clock_mark: 0,
            auto_fetch_newest: true,
            max_threads: default_threads(),
            auto_poll: true,
            sounds: false,
            chat_archive: false,
            follow_bot: true,
            install_id: crate::update::new_install_id(),
            show_crawling_restrict: false,
            show_class_icons: true,
            blacklist_threshold: default_blacklist_threshhold(),
            num_format: default_locale(),
            start_threads: default_start_threads(),
            target_order: TargetOrder::default(),
            auto_battle_max_losses: default_auto_battle_max_losses(),
            theme_migrated: true,
            defaults_migrated: true,
            threads_migrated: true,
            poll_migrated: true,
            threads_ten_migrated: true,
            beer_default_migrated: true,
            sessions_v3_migrated: true,
            sell_old_off_migrated: true,
            lang: crate::i18n::Lang::default(),
            pace: crate::pace::Pace::default(),
            sessions_level: SessionsLevel::Off,
            sessions_auto_migrated: true,
            burst_hours: default_burst_hours(),
            burst_minutes: default_burst_minutes(),
            burst_minutes_max: default_burst_minutes_max(),
            sessions_migrated: true,
            sessions_v2_migrated: true,
            one_slider_migrated: true,
            levels_migrated: true,
            officer_seats: default_officer_seats(),
            job_order: Vec::new(),
            extra_characters: Vec::new(),
            job_order_v2_migrated: true,
            recycle_on_migrated: true,
            gem_socket_on_migrated: true,
            quiet_hours: true,
            quiet_from: default_quiet_from(),
            quiet_to: default_quiet_to(),
            requests_per_min: default_requests_per_min(),
            safety: default_safety(),
            grade_strictness: default_strictness(),
            strictness_v2_migrated: true,
            backpack_516_migrated: true,
            extras_516_migrated: true,
            quiet_52_migrated: true,
            fortress_visits_per_day: default_fortress_visits(),
            train_54_migrated: true,
            guild_top_mark: default_five(),
            guild_bottom_mark: default_five(),
            admin_unlocked: false,
            announce_enabled: false,
            announce_text: String::new(),
            announce_hours: default_announce_hours(),
            announce_last: None,
            auto_kick: false,
            discord_daily: false,
            discord_daily_at: None,
            discord_daily_last: None,
            greet_enabled: false,
            greet_text: default_greet_text(),
            farewell_enabled: false,
            farewell_text: default_farewell_text(),
        }
    }
}

impl Config {
    /// The slider decides the rest. `pace` and `sessions_level` are kept
    /// as fields because a great deal of code reads them, but nothing
    /// sets them by hand any more - they are what `safety` says they are,
    /// so the three can never contradict each other again.
    ///
    /// The quiet hours stay the player's own tick: the slider turns them
    /// off below 15 through `stealth::Profile::quiet_off` and widens them
    /// as it goes up, but whether they exist at all is a decision, not a
    /// consequence
    pub fn apply_safety(&mut self) {
        self.safety = self.safety.min(crate::stealth::MAX_LEVEL);
        // There is no "full power" any more. The old bottom of the scale
        // sent requests with no pauses at all, which is the one thing no
        // player ever does; the new bottom is off, and everything above
        // it plays at a human rhythm
        self.pace = crate::pace::Pace::Human;
        crate::pace::set(self.pace);
        crate::pace::set_speed(crate::stealth::Profile::speed_pct(self.safety));
        // 5.2: no more sessions. When the bot is on it plays - continuously,
        // at a human rhythm, under the per-minute request ceiling and with
        // the night rest - instead of logging off for hours between drawn
        // sessions. "The bot should always go through when it is on" was
        // the guild leader's word; the slider now only picks how carefully
        // (STEALTH .. NO SHAME: request ceiling, deliberate misses)
        self.sessions_level = SessionsLevel::Off;
    }

    /// Minutes between sessions for the slider's position - eight hours
    /// at 1, a bit over half an hour at 10 (see `stealth::gap_minutes`)
    #[must_use]
    pub fn gap_minutes(&self) -> u32 {
        crate::stealth::gap_minutes(self.safety)
    }

    /// Is the app switched off at the slider's left stop?
    #[must_use]
    pub fn off(&self) -> bool {
        self.safety == 0
    }

    /// What the slider currently means, in one line: the word and the
    /// average stretch away it picked - "BALANCED 4 h". The number is the
    /// thing people actually set the slider by, so it belongs on it
    #[must_use]
    pub fn safety_line(&self) -> String {
        // 6.0: no sessions, so no gap to print - just the profile
        self.safety_label().to_string()
    }

    /// The word under the slider - the same three the profile uses, with
    /// full power at the bottom end
    #[must_use]
    pub fn safety_label(&self) -> &'static str {
        crate::stealth::Profile::label(self.safety)
    }

    pub fn get_sso_accounts_mut(
        &mut self,
        name: &str,
    ) -> Option<&mut Vec<SFAccCharacter>> {
        let lower_name = name.to_lowercase();
        self.accounts
            .iter_mut()
            .flat_map(|a| match a {
                AccountConfig::SF {
                    name, characters, ..
                } if name.to_lowercase().trim() == lower_name.trim() => {
                    Some(characters)
                }
                _ => None,
            })
            .next()
    }

    pub fn get_char_conf(
        &self,
        name: &str,
        og_server: ServerID,
    ) -> Option<&CharacterConfig> {
        let mut res = None;

        let lower_name = name.to_lowercase();
        for acc in &self.accounts {
            match acc {
                AccountConfig::Regular {
                    name,
                    server,
                    config,
                    ..
                } => {
                    if ServerIdent::new(server).id != og_server {
                        continue;
                    }
                    if name.to_lowercase().trim() != lower_name.trim() {
                        continue;
                    }
                    res = Some(config);
                    break;
                }
                AccountConfig::SF { characters, .. } => {
                    for c in characters {
                        if ServerIdent::new(&c.ident.server).id != og_server {
                            continue;
                        }
                        if c.ident.name.to_lowercase().trim()
                            != lower_name.trim()
                        {
                            continue;
                        }
                        res = Some(&c.config);
                    }
                }
            }
        }
        // A character without an account entry keeps its settings in the
        // extra list (see `get_char_conf_mut`)
        res.or_else(|| {
            let lower = lower_name.trim();
            self.extra_characters
                .iter()
                .find(|c| c.server == og_server.raw() && c.name == lower)
                // An entry carried over from before 5.1.6, not yet tied to
                // a server - see `extras_516_migrated`
                .or_else(|| {
                    self.extra_characters
                        .iter()
                        .find(|c| c.server == 0 && c.name == lower)
                })
                .map(|c| &c.config)
        })
    }

    /// Where a character's settings live: the account entry (regular, or
    /// the character of an SF account), else the extra list
    fn locate_char(
        &self,
        name: &str,
        og_server: ServerID,
    ) -> Option<(usize, Option<usize>)> {
        let lower_name = name.to_lowercase();
        let lower_name = lower_name.trim();
        for (i, acc) in self.accounts.iter().enumerate() {
            match acc {
                AccountConfig::Regular { name, server, .. } => {
                    if ServerIdent::new(server).id == og_server
                        && name.to_lowercase().trim() == lower_name
                    {
                        return Some((i, None));
                    }
                }
                AccountConfig::SF { characters, .. } => {
                    for (j, c) in characters.iter().enumerate() {
                        if ServerIdent::new(&c.ident.server).id == og_server
                            && c.ident.name.to_lowercase().trim() == lower_name
                        {
                            return Some((i, Some(j)));
                        }
                    }
                }
            }
        }
        None
    }

    /// The settings of a character, made on the spot when the character
    /// has no account entry - one logged in this session without
    /// "remember", one carried over by an update, or one whose SF account
    /// lists it under a different spelling. Its settings used to go
    /// nowhere and every box had to be set again after a restart
    pub fn get_char_conf_mut(
        &mut self,
        name: &str,
        og_server: ServerID,
    ) -> Option<&mut CharacterConfig> {
        match self.locate_char(name, og_server) {
            Some((i, None)) => match &mut self.accounts[i] {
                AccountConfig::Regular { config, .. } => Some(config),
                AccountConfig::SF { .. } => None,
            },
            Some((i, Some(j))) => match &mut self.accounts[i] {
                AccountConfig::SF { characters, .. } => {
                    Some(&mut characters[j].config)
                }
                AccountConfig::Regular { .. } => None,
            },
            None => {
                let lower = name.to_lowercase().trim().to_string();
                let exact = self.extra_characters.iter().position(|c| {
                    c.server == og_server.raw() && c.name == lower
                });
                // A carried-over entry is adopted by the first server that
                // asks for that name, and keeps that server from then on
                let legacy = || {
                    self.extra_characters
                        .iter()
                        .position(|c| c.server == 0 && c.name == lower)
                };
                let k = match exact.or_else(legacy) {
                    Some(k) => {
                        self.extra_characters[k].server = og_server.raw();
                        k
                    }
                    None => {
                        self.extra_characters.push(ExtraCharacter {
                            name: lower,
                            server: og_server.raw(),
                            config: CharacterConfig::default(),
                        });
                        self.extra_characters.len() - 1
                    }
                };
                Some(&mut self.extra_characters[k].config)
            }
        }
    }

    pub fn write(&self) -> Result<(), Box<dyn std::error::Error>> {
        if WRITES_BLOCKED.load(std::sync::atomic::Ordering::SeqCst) {
            // The file on disk could not be read and the user has not said
            // yet what to do with it. Writing the empty in-memory config
            // over it would be the one way to really lose every account
            return Err("helper.toml is unreadable - not overwriting it \
                        until the recovery choice is made"
                .into());
        }
        let str = toml::to_string_pretty(self)?;
        // Always next to the exe, so it survives every update
        crate::storage::write_atomic(&Self::path(), str.as_bytes())?;
        Ok(())
    }

    pub fn path() -> std::path::PathBuf {
        crate::app_path("helper.toml")
    }

    /// Stops every `write()` until `unblock_writes()`. Set when the file
    /// on disk is unreadable, so that the default config the app runs with
    /// in the meantime never replaces it
    pub fn block_writes() {
        WRITES_BLOCKED.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn unblock_writes() {
        WRITES_BLOCKED.store(false, std::sync::atomic::Ordering::SeqCst);
    }

    fn parse(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|e| e.to_string())
    }

    /// Reads `helper.toml`. `Ok(None)` is a fresh install - no file at all
    /// and no backup either. `Err` is a file that is there but does not
    /// read, with the backup no better: the caller must not treat that as
    /// "no accounts" - it used to, via `unwrap_or_default()`, and a single
    /// broken byte looked like a wiped installation
    pub fn restore() -> Result<Option<(Self, crate::storage::Source)>, String> {
        let path = Self::path();
        let backup = crate::storage::sibling(&path, ".bak");
        if !path.exists() && !backup.exists() {
            // The pre-1.6x working-directory copy, so nobody loses their
            // login on the update that moved the file beside the exe
            return match std::fs::read_to_string("helper.toml") {
                Ok(text) => Self::parse(&text).map(|c| {
                    Some((Self::migrate(c), crate::storage::Source::Main))
                }),
                Err(_) => Ok(None),
            };
        }
        let (config, source) =
            crate::storage::read_with_backup(&path, Self::parse)?;
        if source == crate::storage::Source::Backup {
            // The main file is broken. Move it out of the way now: the
            // next save copies whatever sits at the main path over the
            // backup, and that must not be the broken one
            match crate::storage::set_aside(&path) {
                Ok(aside) => log::warn!(
                    "helper.toml was unreadable - loaded the backup, the \
                     broken file is kept as {}",
                    aside.display()
                ),
                Err(e) => log::warn!(
                    "helper.toml was unreadable and could not be moved \
                     aside ({e}) - loaded the backup"
                ),
            }
        }
        Ok(Some((Self::migrate(config), source)))
    }

    /// The one-off changes of defaults, each remembered by its flag
    fn migrate(mut config: Self) -> Self {
        // "Dark" was the old default nobody consciously picked. This swaps it
        // exactly once - picking Dark in the settings afterwards sticks
        if !config.theme_migrated {
            if config.theme == AvailableTheme::Dark {
                config.theme = AvailableTheme::Commander;
            }
            config.theme_migrated = true;
            _ = config.write();
        }
        // The migration that once switched the background poll on for
        // everybody. There is no poll any more, so it only marks itself
        // done and stops touching a dead field
        if !config.poll_migrated {
            config.poll_migrated = true;
            _ = config.write();
        }
        if !config.threads_migrated {
            // One crawler thread finds arena targets far too slowly
            if config.start_threads <= 1 {
                config.start_threads = 4;
            }
            config.threads_migrated = true;
            _ = config.write();
        }
        if !config.sessions_migrated {
            // "Sessions" was a pace; now it is a switch next to the pace.
            // Whoever had it on keeps it on, at human pace as before. The
            // schedule goes to the one that was meant all along - 30 min
            // every 6 h - the old default of 20 min and whatever got typed
            // in around it were never what the guild leader asked for
            if config.pace == crate::pace::Pace::Burst {
                config.pace = crate::pace::Pace::Human;
                config.sessions_level = SessionsLevel::Normal;
            }
            config.burst_hours = default_burst_hours();
            config.burst_minutes = default_burst_minutes();
            config.sessions_migrated = true;
            _ = config.write();
        }
        if !config.sessions_v2_migrated {
            // Second thought from the guild leader: three sessions' worth
            // of gaps a day is too few, and a fixed length looks scripted -
            // 30 to 60 min every 3 h from now on
            config.burst_hours = default_burst_hours();
            config.burst_minutes = default_burst_minutes();
            config.burst_minutes_max = default_burst_minutes_max();
            config.sessions_v2_migrated = true;
            _ = config.write();
        }
        if !config.threads_ten_migrated {
            // Four was still not enough: the Hall of Fame runs to tens of
            // thousands of players and the arena is blind until it is read
            if config.start_threads <= 4 {
                // Never above the user's own ceiling
                config.start_threads = config.max_threads.clamp(1, 10);
            }
            config.threads_ten_migrated = true;
            _ = config.write();
        }
        if !config.gem_socket_on_migrated {
            // Socketing found gems is what the program does, not a choice
            let fix = |c: &mut CharacterConfig| c.gem_socket = true;
            for acc in &mut config.accounts {
                match acc {
                    AccountConfig::Regular { config, .. } => fix(config),
                    AccountConfig::SF { characters, .. } => {
                        for c in characters {
                            fix(&mut c.config);
                        }
                    }
                }
            }
            config.gem_socket_on_migrated = true;
            _ = config.write();
        }
        if !config.recycle_on_migrated {
            // The guild leader's word: the replaced piece is recycled -
            // the gem out first, the raised piece dismantled, the rest sold
            let fix = |c: &mut CharacterConfig| c.shop_sell_old = true;
            for acc in &mut config.accounts {
                match acc {
                    AccountConfig::Regular { config, .. } => fix(config),
                    AccountConfig::SF { characters, .. } => {
                        for c in characters {
                            fix(&mut c.config);
                        }
                    }
                }
            }
            config.recycle_on_migrated = true;
            _ = config.write();
        }
        if !config.job_order_v2_migrated {
            // The order of the day, once, for everybody - an empty list
            // means the built-in default; "Default" in Settings brings it
            // back should somebody rearrange it again
            config.job_order.clear();
            config.job_order_v2_migrated = true;
            _ = config.write();
        }
        if !config.sessions_auto_migrated {
            // 2.0: the app farms in sessions of its own choosing - on for
            // everybody once; the switch in Settings still turns it off
            config.sessions_level = SessionsLevel::Normal;
            config.sessions_auto_migrated = true;
            _ = config.write();
        }
        if !config.strictness_v2_migrated {
            // Only a slider nobody touched moves; a chosen value stays
            if config.grade_strictness == 50 {
                config.grade_strictness = default_strictness();
            }
            config.strictness_v2_migrated = true;
            _ = config.write();
        }
        if !config.sell_old_off_migrated {
            // A sale cannot be undone, so it is nobody's default any more
            let fix = |c: &mut CharacterConfig| c.shop_sell_old = false;
            for acc in &mut config.accounts {
                match acc {
                    AccountConfig::Regular { config, .. } => fix(config),
                    AccountConfig::SF { characters, .. } => {
                        for c in characters {
                            fix(&mut c.config);
                        }
                    }
                }
            }
            config.sell_old_off_migrated = true;
            _ = config.write();
        }
        if !config.sessions_v3_migrated {
            // "31-60 min" on the button: the spinner had been nudged once
            // past the intended 30. Put back exactly that case
            if config.burst_minutes == 31 {
                config.burst_minutes = default_burst_minutes();
            }
            // Wheel spins written as 0 by an older build's derive default:
            // the page showed 1 and never remembered anything else
            let fix = |c: &mut CharacterConfig| {
                if c.wheel_spins == 0 {
                    c.wheel_spins = default_wheel_spins();
                }
            };
            for acc in &mut config.accounts {
                match acc {
                    AccountConfig::Regular { config, .. } => fix(config),
                    AccountConfig::SF { characters, .. } => {
                        for c in characters {
                            fix(&mut c.config);
                        }
                    }
                }
            }
            config.sessions_v3_migrated = true;
            _ = config.write();
        }
        if !config.beer_default_migrated {
            // The serde default only reaches fields missing from the file;
            // every saved character has it written out as `false`
            let fix = |c: &mut CharacterConfig| {
                c.beer_auto = true;
            };
            for acc in &mut config.accounts {
                match acc {
                    AccountConfig::Regular { config, .. } => fix(config),
                    AccountConfig::SF { characters, .. } => {
                        for c in characters {
                            fix(&mut c.config);
                        }
                    }
                }
            }
            config.beer_default_migrated = true;
            _ = config.write();
        }
        if !config.one_slider_migrated {
            // Three controls became one. Whatever the file said, the
            // slider now carries it: full power is the bottom of the
            // scale, sessions switched off is the next thing to it, and
            // anybody who was playing in sessions keeps the position the
            // slider already had
            config.safety = if config.pace == crate::pace::Pace::FullPower {
                0
            } else if config.sessions_level == SessionsLevel::Off {
                10
            } else {
                config.safety.clamp(1, 100)
            };
            config.one_slider_migrated = true;
            _ = config.write();
        }
        if !config.levels_migrated {
            // The slider used to run 0-100 with 0 meaning "full power",
            // i.e. the most play, and 100 the least. It now runs 0-10 with
            // 0 meaning off and 10 the most play, so the scale is both
            // shorter and the other way round. A saved position is carried
            // across rather than reset: whoever was at 100 (the quietest)
            // lands on 1, whoever was at 0 lands on 10
            let old = u32::from(config.safety.min(100));
            config.safety = (1 + ((100 - old) * 9).div_ceil(100)) as u8;
            config.levels_migrated = true;
            _ = config.write();
        }
        // The slider is the source of truth for the pace, the sessions
        // level and the length of a stretch away - always, not only when
        // it is moved, so a file written by an older build self-corrects
        config.apply_safety();
        if !config.defaults_migrated {
            let fix = |c: &mut CharacterConfig| {
                if c.dungeon_min_chance == 60 {
                    c.dungeon_min_chance = 0;
                }
                c.guild_attack = true;
                c.guild_defense = true;
            };
            for acc in &mut config.accounts {
                match acc {
                    AccountConfig::Regular { config, .. } => fix(config),
                    AccountConfig::SF { characters, .. } => {
                        for c in characters {
                            fix(&mut c.config);
                        }
                    }
                }
            }
            config.defaults_migrated = true;
            _ = config.write();
        }
        // `install_id` is only ever drawn in `Default`, which runs when there
        // is no config file at all. Every install that already had a
        // helper.toml therefore got `#[serde(default)]` - the empty string -
        // and `ping` refuses to send an empty id, so those installs were
        // invisible to the counter for good. Which is why the count showed
        // question marks even with the author's own machine running
        if config.install_id.is_empty() {
            config.install_id = crate::update::new_install_id();
            _ = config.write();
        }
        if !config.extras_516_migrated {
            config.extra_characters =
                fold_extras(std::mem::take(&mut config.extra_characters));
            config.extras_516_migrated = true;
            _ = config.write();
        }
        if !config.guild_aurora_migrated {
            config.guild_two_columns = true;
            config.guild_aurora_migrated = true;
            _ = config.write();
        }
        if !config.font_aurora_migrated {
            config.drawn_font = false;
            config.font_aurora_migrated = true;
            _ = config.write();
        }
        if !config.font_60_migrated {
            config.drawn_font = false;
            config.font_60_migrated = true;
            _ = config.write();
        }
        if !config.train_54_migrated {
            let fix = |c: &mut CharacterConfig| c.fortress_train = false;
            for acc in &mut config.accounts {
                match acc {
                    AccountConfig::Regular { config, .. } => fix(config),
                    AccountConfig::SF { characters, .. } => {
                        for c in characters {
                            fix(&mut c.config);
                        }
                    }
                }
            }
            for c in &mut config.extra_characters {
                fix(&mut c.config);
            }
            config.train_54_migrated = true;
            _ = config.write();
        }
        if !config.quiet_52_migrated {
            config.quiet_hours = true;
            config.quiet_from = 3;
            config.quiet_to = 7;
            config.quiet_52_migrated = true;
            _ = config.write();
        }
        if !config.backpack_516_migrated {
            let fix = |c: &mut CharacterConfig| {
                c.shop_clear_backpack = true;
                c.shop_keep_free = c.shop_keep_free.max(2);
            };
            for acc in &mut config.accounts {
                match acc {
                    AccountConfig::Regular { config, .. } => fix(config),
                    AccountConfig::SF { characters, .. } => {
                        for c in characters {
                            fix(&mut c.config);
                        }
                    }
                }
            }
            for c in &mut config.extra_characters {
                fix(&mut c.config);
            }
            config.backpack_516_migrated = true;
            _ = config.write();
        }
        config
    }
}

/// See `Config::block_writes`
static WRITES_BLOCKED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum AccountCreds {
    Regular {
        name: String,
        pw_hash: PWHash,
        server: String,
    },
    SF {
        name: String,
        pw_hash: PWHash,
    },
}

impl From<AccountConfig> for AccountCreds {
    fn from(value: AccountConfig) -> Self {
        match value {
            AccountConfig::Regular {
                name,
                pw_hash,
                server,
                ..
            } => AccountCreds::Regular {
                name,
                pw_hash,
                server,
            },
            AccountConfig::SF { name, pw_hash, .. } => {
                AccountCreds::SF { name, pw_hash }
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum AccountConfig {
    Regular {
        name: String,
        pw_hash: PWHash,
        server: String,
        #[serde(flatten)]
        config: CharacterConfig,
    },
    SF {
        name: String,
        pw_hash: PWHash,
        #[serde(default)]
        characters: Vec<SFAccCharacter>,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SFAccCharacter {
    pub ident: SFCharIdent,
    pub config: CharacterConfig,
}

/// One entry per name, the newest one (the list only ever grew by `push`,
/// so the last is the newest), not yet tied to a server - see
/// `extras_516_migrated`
fn fold_extras(extras: Vec<ExtraCharacter>) -> Vec<ExtraCharacter> {
    let mut kept: Vec<ExtraCharacter> = Vec::new();
    for mut c in extras.into_iter().rev() {
        if kept.iter().any(|k| k.name == c.name) {
            continue;
        }
        c.server = 0;
        kept.push(c);
    }
    kept.reverse();
    kept
}

/// The settings of a character that has no account entry of its own -
/// kept by lowercase name and the server's id, so they survive a restart
/// like everybody else's
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExtraCharacter {
    pub name: String,
    pub server: u64,
    pub config: CharacterConfig,
}

// Not `derive(Default)`: that gave `false` to every `default_true` field, so
// a character added at runtime started with the portal, raids, dailies and
// the mount switched off, while the same character read from an old
// helper.toml got them on. An empty table goes through the serde defaults,
// which are the one list of what "new character" means
impl Default for CharacterConfig {
    fn default() -> Self {
        toml::from_str("")
            .expect("every CharacterConfig field has a serde default")
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CharacterConfig {
    #[serde(default)]
    pub login: bool,
    /// A proxy this character's requests go through, "http://user:pass@\
    /// host:port" or "socks5://host:port"; empty is the machine's own
    /// address. Several characters from one address are the plainest sign
    /// of multi-accounting there is, and the app can not change that -
    /// only a different route per character can. Takes effect at the next
    /// login; password logins only, an SSO character's route is the
    /// login provider's
    #[serde(default)]
    pub proxy: String,
    #[serde(default)]
    pub auto_battle: bool,
    #[serde(default)]
    pub auto_lure: bool,
    /// Collect gold and souls in the underworld when a production building
    /// is nearly full - production stops once the store is full, so anything
    /// left standing there is thrown away
    #[serde(default)]
    pub underworld_collect: bool,
    /// Keep upgrading the underworld buildings
    #[serde(default)]
    pub underworld_build: bool,
    /// In which order - souls first, or the character's bonuses
    #[serde(default)]
    pub underworld_phase: crate::mechanics::UnderworldPhase,
    /// How many times a day the shops may be rerolled when they hold nothing
    /// worth buying for the character or any of the three companions
    #[serde(default = "default_shop_rolls")]
    pub shop_rolls_per_day: u8,
    /// Take the daily swing at the guild portal
    #[serde(default = "default_true")]
    pub guild_portal: bool,
    /// Automatically battle the fortress opponent
    #[serde(default)]
    pub auto_fortress: bool,
    /// Seconds between two fortress attacks
    #[serde(default = "default_fortress_interval")]
    pub fortress_interval: u64,
    /// The most the raid waits after the barracks fill, in minutes - the
    /// wait is drawn between three minutes and this. It replaces the
    /// "about every x minutes" clock above, which is kept so an older
    /// `helper.toml` still loads
    #[serde(default = "default_fortress_notice_max_min")]
    pub fortress_notice_max_min: u32,
    /// Kept only so an older helper.toml still parses. A free new opponent
    /// is never worse than the one standing there, so it stopped being a
    /// choice - the app just takes it
    #[serde(default, skip_serializing)]
    pub fortress_reroll: bool,
    /// Queue one new soldier after every fortress attack. Off since 5.4:
    /// nobody plays like that - the soldiers there are are used up, then
    /// the barracks fill
    #[serde(default)]
    pub fortress_train: bool,
    /// Collect wood/stone/experience when the producing building is full
    #[serde(default = "default_true")]
    pub fortress_collect: bool,
    /// Keep the gem mine searching
    #[serde(default = "default_true")]
    pub fortress_gems: bool,
    /// Upgrade buildings on their own (spends wood, stone and silver)
    #[serde(default)]
    pub fortress_build: bool,
    /// Join every guild attack automatically (on by default - missing a
    /// raid hurts the whole guild)
    #[serde(default = "default_true")]
    pub guild_attack: bool,
    /// Join every guild defense automatically
    #[serde(default = "default_true")]
    pub guild_defense: bool,
    /// Start a guild raid whenever none is running and one is possible
    /// (costs the guild 10 000 gold) - for a leader or officer character
    #[serde(default)]
    pub guild_raid_auto: bool,
    /// Without a guild: read the game's guild finder every half hour, look
    /// up the bonuses of the first dozen and ask the best one to let us in
    #[serde(default)]
    pub guild_finder_auto: bool,
    /// Use the free dungeon fight on the dungeon with the best odds
    #[serde(default)]
    pub dungeon_auto: bool,
    /// Only auto-fight dungeons with at least this win chance (percent).
    /// 0 = always take the one with the best odds
    #[serde(default)]
    pub dungeon_min_chance: u8,
    /// After the free fight, spend mushrooms on extra dungeon fights
    #[serde(default)]
    pub dungeon_mushroom: bool,
    /// Lowest win chance (percent) worth a mushroom
    #[serde(default = "default_mushroom_min")]
    pub dungeon_mushroom_min: u8,
    /// Most mushroom dungeon fights per day
    #[serde(default = "default_mushroom_max")]
    pub dungeon_mushroom_max: u8,
    /// Fight the daily demonic portal on its own
    #[serde(default = "default_true")]
    pub dungeon_portal: bool,
    /// After the free arena fight, spend mushrooms on extra attacks
    #[serde(default)]
    pub arena_mushroom: bool,
    /// Most mushroom arena fights per day
    #[serde(default = "default_mushroom_max")]
    pub arena_mushroom_max: u8,
    /// Feed the pets with the fruit you have, strongest first
    #[serde(default)]
    pub pets_feed: bool,
    /// Fight the daily pet arena opponent as well
    #[serde(default = "default_true")]
    pub pets_arena: bool,
    /// Recruit soldiers, archers and mages until every barracks is full
    #[serde(default)]
    pub fortress_recruit_all: bool,
    /// Send only as many soldiers as the game recommends for the opponent
    /// (the green marker in the game). Falls back to the manual count
    #[serde(default = "default_true")]
    pub fortress_smart_soldiers: bool,
    /// Send a single soldier at an opponent we cannot beat, so the next
    /// attack gets a new one instead of waiting for the barracks
    #[serde(default = "default_true")]
    pub fortress_cheap_skip: bool,
    /// Extra soldiers on top of the recommendation, for safety
    #[serde(default)]
    pub fortress_soldier_margin: u16,
    /// How many soldiers to send per attack. 0 = all of them
    #[serde(default)]
    pub fortress_attack_soldiers: u16,
    /// Keep buying the Arena Manager upgrade with the best return
    #[serde(default)]
    pub idle_upgrade: bool,
    /// Try to collect the finished Arena Manager buildings (experimental)
    #[serde(default)]
    pub idle_collect: bool,
    /// Run expeditions on their own
    #[serde(default)]
    pub expedition_auto: bool,
    /// Optimize expeditions for gold instead of XP
    #[serde(default)]
    pub expedition_gold: bool,
    /// Skip expedition waits with quicksand glasses
    #[serde(default = "default_true")]
    pub expedition_skip_glass: bool,
    /// Skip expedition waits with mushrooms
    #[serde(default)]
    pub expedition_skip_mushroom: bool,
    /// Spend the leftover thirst for adventure on tavern quests - when no
    /// expedition event is running, or when no expedition fits what is left
    #[serde(default = "default_true")]
    pub quest_auto: bool,
    /// Collect the daily login reward and the goblin gleeman's task chests
    #[serde(default = "default_true")]
    pub dailies_auto: bool,
    /// Keep a mount rented (the gold-only horse) so it never lapses - a
    /// lapsed mount is a silent loss on every expedition, quest and guard
    /// shift. Renewed as part of the daily upkeep
    #[serde(default = "default_true")]
    pub mount_auto: bool,
    /// Which gems to keep. Empty = keep every one of them
    #[serde(default)]
    pub gem_wanted: Vec<GemWanted>,
    /// Put found gems into empty gem slots of the character and the three
    /// tower companions, matched to what each of them fights with. On for
    /// everybody since 2.1 - the box is gone, the protector list stays
    #[serde(default = "default_true")]
    pub gem_socket: bool,
    /// Work as city guard whenever the thirst for adventure is used up
    #[serde(default)]
    pub guard_auto: bool,
    /// Hours per shift (1-10)
    #[serde(default = "default_guard_hours")]
    pub guard_hours: u8,
    /// Shifts are cut so that they end before this time of day ("23:30");
    /// between that time and midnight nothing is started
    #[serde(default = "default_true")]
    pub guard_cutoff_enabled: bool,
    #[serde(default = "default_guard_cutoff")]
    pub guard_cutoff: String,
    /// Use the free fight on the pet habitat dungeons first (they share the
    /// cooldown with the normal dungeons)
    #[serde(default = "default_true")]
    pub dungeon_pets_first: bool,
    /// Sacrifice (reset) the Arena Manager once a reset would add at least
    /// `idle_sacrifice_gain_pct` per cent to the earnings the runes already
    /// buy
    #[serde(default)]
    pub idle_sacrifice: bool,
    /// Dead. It was a count of runes - "reset once the offer reaches N" -
    /// which is not a quantity anything can be decided from: the same N is
    /// everything to a new account and a rounding error to an old one.
    /// Kept only so a `helper.toml` written before the change still loads
    #[serde(default = "default_idle_sacrifice_step")]
    #[allow(dead_code)]
    pub idle_sacrifice_step: u32,
    /// How much a reset has to add, as a percentage of what the runes
    /// already earn. This scales by construction: ten per cent of a
    /// million runes is a hundred thousand, ten per cent of a thousand is
    /// a hundred
    #[serde(default = "default_idle_sacrifice_gain_pct")]
    pub idle_sacrifice_gain_pct: u32,
    /// The rune-unit boundary at the time of our last sacrifice
    #[serde(default)]
    pub idle_sacrificed_units: u64,
    /// Runes the last automatic sacrifice was worth; informational only
    #[serde(default)]
    pub idle_last_sacrifice: u64,
    /// When this guild was founded, as the leader typed it ("21.8.2026").
    ///
    /// The app's diary only knows what it has watched, so for anybody who
    /// was already a member on its first day it can say nothing better
    /// than "at least since the day I started looking" - which is not a
    /// fact about that person at all. The one member who *does* know is
    /// the leader, about his own guild, so he types it here and it is
    /// used for his own row and for the guild's age. Everybody else's
    /// tenure stays an honest question mark
    #[serde(default)]
    pub guild_founded: String,
    /// Drink beer (costs mushrooms) when the thirst is used up. On by
    /// default - the guild leader's call: a day without the beer thirst is
    /// a day's XP left on the table
    #[serde(default = "default_true")]
    pub beer_auto: bool,
    /// At most this many beers a day
    #[serde(default = "default_beer_max")]
    pub beer_max: u8,
    /// Mushrooms the automation never touches - beer, shop rerolls, epics
    /// only spend what is above this. The guild leader's box, replacing the
    /// "one more than the morning" rule that nobody could see working
    #[serde(default = "default_mushroom_reserve")]
    pub mushroom_reserve: u32,
    /// The beer costs nothing for this character (a VIP package). The game
    /// does not tell the app, so the player confirms it once by ticking the
    /// box, and it stays in the file. Never on by default: for a character
    /// without VIP it would buy beer for mushrooms straight past the reserve
    #[serde(default)]
    pub beer_free: bool,
    /// Buy better gear in the weapon / magic shop
    #[serde(default)]
    pub shop_auto: bool,
    /// Only buy when the item is at least this much better (percent)
    #[serde(default = "default_shop_min_gain")]
    pub shop_min_gain: u8,
    /// Keep this share of the gold untouched (percent)
    #[serde(default = "default_gold_reserve")]
    pub gold_reserve: u8,
    #[serde(default = "default_true")]
    pub shop_protect_epics: bool,
    /// A worn legendary gives way to a piece this many percent better
    /// (0 = never); the old one goes into the bag, never sold
    #[serde(default = "default_legendary_swap")]
    pub legendary_swap_pct: u8,
    /// Items with a gem of at least this strength are never replaced
    /// (0 = gems do not protect)
    #[serde(default = "default_one")]
    pub shop_protect_gem_min: u32,
    #[serde(default = "default_true")]
    pub shop_protect_enchanted: bool,
    #[serde(default = "default_true")]
    pub shop_protect_upgraded: bool,
    /// Sell the replaced piece back to the shop. Off unless asked: a sale
    /// is the one step that cannot be undone
    #[serde(default)]
    pub shop_sell_old: bool,
    /// Buy epics for mushrooms too
    #[serde(default)]
    pub shop_mushrooms: bool,
    /// Buy attributes with spare gold
    #[serde(default)]
    pub attr_auto: bool,
    /// Weights: how much main attribute / constitution / luck relative to
    /// each other (0 = never)
    #[serde(default = "default_ten")]
    pub attr_w_main: u8,
    #[serde(default = "default_ten")]
    pub attr_w_con: u8,
    #[serde(default = "default_five")]
    pub attr_w_luck: u8,
    /// Spin the wheel of fortune
    #[serde(default)]
    pub wheel_auto: bool,
    /// Spins per day (the free one, then lucky coins)
    #[serde(default = "default_wheel_spins")]
    pub wheel_spins: u8,
    /// Play the free games of dice with the cup player
    #[serde(default)]
    pub dice_auto: bool,
    /// Skip the wait between dice games with a quicksand glass
    #[serde(default = "default_true")]
    pub dice_skip: bool,
    /// Sell the weakest unwanted piece when the backpack fills up. A full
    /// backpack stops the dungeons, so this is on unless it is turned off
    #[serde(default = "default_true")]
    pub shop_clear_backpack: bool,
    /// Backpack slots to keep free
    #[serde(default = "default_keep_free")]
    pub shop_keep_free: u8,
    /// Equip the shadow dungeon companions from the backpack
    #[serde(default = "default_true")]
    pub equip_companions: bool,
    /// Raise the worn gear (the character's and the companions') at the
    /// blacksmith, up to +20, never a piece with something better on offer
    #[serde(default = "default_true")]
    pub smith_auto: bool,
    /// Enchant the worn gear at the witch when the enchantment is unlocked
    /// and the gold above the reserve covers it
    #[serde(default = "default_true")]
    pub witch_auto: bool,
    /// Mushrooms the bot may spend in one server day, on anything: beer,
    /// paid fights, rerolls, epics, skipped waits. The one number a player
    /// has to set; 0 keeps every mushroom
    #[serde(default = "default_mushroom_budget")]
    pub mushroom_day_budget: u32,
    /// Mail the daily top/bottom rating to the whole guild
    #[serde(default)]
    pub guild_report_auto: bool,
    /// Hour of day (local) to send it
    #[serde(default = "default_report_hour")]
    pub guild_report_hour: u8,
    /// Last day it went out, so a restart does not send it twice
    #[serde(default)]
    pub guild_report_last: Option<chrono::NaiveDate>,
}

fn default_burst_hours() -> u8 {
    3
}

fn default_burst_minutes() -> u8 {
    30
}

fn default_burst_minutes_max() -> u8 {
    60
}

/// 40 for a guild of fifty: the top fifth gets a 1, the bottom seventh a
/// 5 - and a 5 is only a kick with a hard reason behind it (see
/// `guild_report`). 50 flagged nine of fifty at once, which is a wall,
/// not a list an officer can work through
fn default_strictness() -> u8 {
    40
}

/// Nothing until the player says so - mushrooms are the premium currency,
/// and a number they typed is the only mandate to spend it
fn default_mushroom_budget() -> u32 {
    // Five beers and a few rerolls - the "eco" day the guild leader plays
    // by. 0 kept every mushroom, so a new character never drank a beer
    10
}

fn default_shop_min_gain() -> u8 {
    5
}

fn default_gold_reserve() -> u8 {
    30
}

fn default_one() -> u32 {
    1
}

fn default_ten() -> u8 {
    10
}

fn default_five() -> u8 {
    5
}

fn default_idle_sacrifice_step() -> u32 {
    20
}

fn default_idle_sacrifice_gain_pct() -> u32 {
    // One of `mechanics::SACRIFICE_STEPS` - the middle one. It was 10,
    // from when this meant "add at least N per cent", and 10 is not a
    // setting the picker offers
    50
}

fn default_beer_max() -> u8 {
    11
}

fn default_mushroom_reserve() -> u32 {
    10
}

fn default_fortress_visits() -> u8 {
    6
}

fn default_legendary_swap() -> u8 {
    50
}

fn default_keep_free() -> u8 {
    2
}

/// All twenty a day - the day's limit: the free one and the lucky-coin
/// ones. It used to be 1
pub fn default_wheel_spins() -> u8 {
    20
}

/// Three a day. A reroll is only ever spent when both shops hold nothing
/// worth buying for anybody, so this is a ceiling, not a schedule
fn default_shop_rolls() -> u8 {
    3
}

/// A dungeon has to look at least this winnable before a mushroom is spent
fn default_mushroom_min() -> u8 {
    50
}

/// A safety ceiling so auto mushroom fights can never drain a stash
fn default_mushroom_max() -> u8 {
    5
}

fn default_guard_cutoff() -> String {
    "23:30".to_string()
}

fn default_guard_hours() -> u8 {
    10
}

fn default_report_hour() -> u8 {
    20
}

fn default_fortress_notice_max_min() -> u32 {
    60
}

fn default_fortress_interval() -> u64 {
    180
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize, Deserialize, Clone, Hash, PartialEq, Eq)]
pub struct SFCharIdent {
    pub name: String,
    pub server: String,
}

impl AccountConfig {
    pub fn new(creds: AccountCreds) -> AccountConfig {
        match creds {
            AccountCreds::Regular {
                name,
                pw_hash,
                server,
            } => AccountConfig::Regular {
                name,
                pw_hash,
                server,
                config: Default::default(),
            },
            AccountCreds::SF { name, pw_hash } => AccountConfig::SF {
                name,
                pw_hash,
                characters: Default::default(),
            },
        }
    }
}

#[derive(
    Debug, Serialize, Deserialize, Default, Clone, Copy, PartialEq, Eq,
)]
pub enum AvailableTheme {
    /// The app's own look - dark navy with gold accents, matching the icon
    #[default]
    Commander,
    Light,
    Dark,
    Dracula,
    Nord,
    SolarizedLight,
    SolarizedDark,
    GruvboxLight,
    GruvboxDark,
    CatppuccinLatte,
    CatppuccinFrappe,
    CatppuccinMacchiato,
    CatppuccinMocha,
    TokyoNight,
    TokyoNightStorm,
    TokyoNightLight,
    KanagawaWave,
    KanagawaDragon,
    KanagawaLotus,
    Moonfly,
    Nightfly,
    Oxocarbon,
}

#[allow(clippy::to_string_trait_impl)]
impl ToString for AvailableTheme {
    fn to_string(&self) -> String {
        use AvailableTheme::*;
        if matches!(self, Commander) {
            return "Commander".to_string();
        }
        match self {
            Commander => unreachable!(),
            Light => Theme::Light,
            Dark => Theme::Dark,
            Dracula => Theme::Dracula,
            Nord => Theme::Nord,
            SolarizedLight => Theme::SolarizedLight,
            SolarizedDark => Theme::SolarizedDark,
            GruvboxLight => Theme::GruvboxLight,
            GruvboxDark => Theme::GruvboxDark,
            CatppuccinLatte => Theme::CatppuccinLatte,
            CatppuccinFrappe => Theme::CatppuccinFrappe,
            CatppuccinMacchiato => Theme::CatppuccinMacchiato,
            CatppuccinMocha => Theme::CatppuccinMocha,
            TokyoNight => Theme::TokyoNight,
            TokyoNightStorm => Theme::TokyoNightStorm,
            TokyoNightLight => Theme::TokyoNightLight,
            KanagawaWave => Theme::KanagawaWave,
            KanagawaDragon => Theme::KanagawaDragon,
            KanagawaLotus => Theme::KanagawaLotus,
            Moonfly => Theme::Moonfly,
            Nightfly => Theme::Nightfly,
            Oxocarbon => Theme::Oxocarbon,
        }
        .to_string()
    }
}

impl AvailableTheme {
    pub fn theme(&self) -> Theme {
        use AvailableTheme::*;

        match self {
            Commander => commander_theme(),
            Light => Theme::Light,
            Dark => Theme::Dark,
            Dracula => Theme::Dracula,
            Nord => Theme::Nord,
            SolarizedLight => Theme::SolarizedLight,
            SolarizedDark => Theme::SolarizedDark,
            GruvboxLight => Theme::GruvboxLight,
            GruvboxDark => Theme::GruvboxDark,
            CatppuccinLatte => Theme::CatppuccinLatte,
            CatppuccinFrappe => Theme::CatppuccinFrappe,
            CatppuccinMacchiato => Theme::CatppuccinMacchiato,
            CatppuccinMocha => Theme::CatppuccinMocha,
            TokyoNight => Theme::TokyoNight,
            TokyoNightStorm => Theme::TokyoNightStorm,
            TokyoNightLight => Theme::TokyoNightLight,
            KanagawaWave => Theme::KanagawaWave,
            KanagawaDragon => Theme::KanagawaDragon,
            KanagawaLotus => Theme::KanagawaLotus,
            Moonfly => Theme::Moonfly,
            Nightfly => Theme::Nightfly,
            Oxocarbon => Theme::Oxocarbon,
        }
    }
}

/// The app's own theme. Built once and cloned afterwards, because `theme()`
/// is asked for on every redraw
fn commander_theme() -> Theme {
    static THEME: std::sync::OnceLock<Theme> = std::sync::OnceLock::new();
    THEME
        .get_or_init(|| {
            Theme::custom(
                "S&F Commander".to_string(),
                // Gold is reserved for the icon. As the UI accent it ended
                // up on every button, checkbox and progress bar at once,
                // which was far too loud - so the accent is a calm steel blue
                iced::theme::Palette {
                    background: crate::ui::style::FRAME,
                    text: crate::ui::style::GOLD,
                    primary: crate::ui::style::ACCENT,
                    success: crate::ui::style::GOOD,
                    danger: crate::ui::style::BAD,
                },
            )
        })
        .clone()
}

/// The gem the mine should look for. The game does not let you choose, so the
/// only way to steer it is to throw a running search away and start another -
/// which costs the search price again, hence the daily cap next to it.
///
/// The colours are the ones on the gem in the game; the names are what the
/// game library calls them.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default,
)]
pub enum GemWanted {
    /// Blue
    #[default]
    Strength,
    /// Yellow
    Dexterity,
    /// Green
    Intelligence,
    /// Purple
    Constitution,
    Luck,
    /// One gem for every attribute
    All,
    Legendary,
}

impl GemWanted {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            GemWanted::Strength => "Strength (blue)",
            GemWanted::Dexterity => "Dexterity (yellow)",
            GemWanted::Intelligence => "Intelligence (green)",
            GemWanted::Constitution => "Constitution (purple)",
            GemWanted::Luck => "Luck",
            GemWanted::All => "All attributes",
            GemWanted::Legendary => "Legendary",
        }
    }

    #[must_use]
    pub const fn every() -> [GemWanted; 7] {
        [
            GemWanted::Strength,
            GemWanted::Dexterity,
            GemWanted::Intelligence,
            GemWanted::Constitution,
            GemWanted::Luck,
            GemWanted::All,
            GemWanted::Legendary,
        ]
    }

    /// Whether a gem the mine turned up is the one that was asked for
    #[must_use]
    pub fn matches(self, found: sf_api::gamestate::items::GemType) -> bool {
        use sf_api::gamestate::items::GemType;
        matches!(
            (self, found),
            (GemWanted::Strength, GemType::Strength)
                | (GemWanted::Dexterity, GemType::Dexterity)
                | (GemWanted::Intelligence, GemType::Intelligence)
                | (GemWanted::Constitution, GemType::Constitution)
                | (GemWanted::Luck, GemType::Luck)
                | (GemWanted::All, GemType::All)
                | (GemWanted::Legendary, GemType::Legendary)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A helper.toml written before the switch existed must open in the
    /// app's own lettering, not in the plain type. A missing line means
    /// "never chose", and never having chosen is not the same as having
    /// chosen the other one
    #[test]
    fn an_older_config_reads_in_plain_type() {
        // A real file with the one line taken out, rather than a hand-typed
        // fragment - that way the test keeps working as other fields come
        // and go, and it is testing the line it means to
        let written = toml::to_string_pretty(&Config::default()).unwrap();
        assert!(written.contains("drawn_font"), "the line is not written");
        let old: String = written
            .lines()
            .filter(|l| !l.trim_start().starts_with("drawn_font"))
            .collect::<Vec<_>>()
            .join("\n");
        let c: Config = toml::from_str(&old).expect("an old file must load");
        assert!(!c.drawn_font, "6.0: an old config reads in plain type");
        assert!(!Config::default().drawn_font);
    }

    /// `Default` goes through serde, so a runtime-added character and one
    /// read from an old file start the same - and the beer is on for both
    #[test]
    fn character_default_is_the_serde_default() {
        let c = CharacterConfig::default();
        assert!(c.beer_auto);
        // VIP is the player's own confirmation, never assumed
        assert!(!c.beer_free);
        assert_eq!(c.beer_max, 11);
        assert!(c.guild_portal);
        assert!(c.dailies_auto);
        assert!(!c.guard_auto);
        assert!(!c.shop_auto);
        // A sale cannot be undone - never a default
        assert!(!c.shop_sell_old);
    }

    /// An old file that spelled beer out as `false` keeps it false until
    /// the one-off migration in `restore` flips it; a confirmed VIP tick
    /// survives a reload
    #[test]
    fn beer_flags_round_trip() {
        let c: CharacterConfig =
            toml::from_str("beer_auto = false\nbeer_free = true").unwrap();
        assert!(!c.beer_auto);
        assert!(c.beer_free);
        let back: CharacterConfig =
            toml::from_str(&toml::to_string(&c).unwrap()).unwrap();
        assert!(back.beer_free);
    }
}

#[cfg(test)]
mod extras_516_tests {
    use super::*;

    fn extra(name: &str, server: u64, keep_free: u8) -> ExtraCharacter {
        let mut config = CharacterConfig::default();
        config.shop_keep_free = keep_free;
        ExtraCharacter {
            name: name.to_string(),
            server,
            config,
        }
    }

    #[test]
    fn folding_keeps_the_newest_entry_per_name() {
        let folded = fold_extras(vec![
            extra("petr macinka", 11, 1),
            extra("other", 12, 3),
            extra("petr macinka", 13, 4),
            extra("petr macinka", 14, 5),
        ]);
        assert_eq!(folded.len(), 2);
        let petr = folded.iter().find(|c| c.name == "petr macinka").unwrap();
        assert_eq!(petr.config.shop_keep_free, 5, "the newest one stays");
        assert!(folded.iter().all(|c| c.server == 0));
    }

    #[test]
    fn a_carried_over_entry_is_adopted_by_its_server() {
        let mut config = Config::default();
        config.extra_characters = fold_extras(vec![extra("petr macinka", 99, 4)]);
        let server = ServerIdent::new("https://s29.sfgame.eu/").id;
        let found = config
            .get_char_conf("Petr Macinka", server)
            .map(|c| c.shop_keep_free);
        assert_eq!(found, Some(4));
        let c = config.get_char_conf_mut("Petr Macinka", server).unwrap();
        assert_eq!(c.shop_keep_free, 4);
        assert_eq!(config.extra_characters.len(), 1, "no new entry");
        assert_eq!(config.extra_characters[0].server, server.raw());
    }
}
