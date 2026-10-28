use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use chrono::{DateTime, Local};
use nohash_hasher::IntMap;
use sf_api::{
    gamestate::{
        GameState, dungeons::Dungeon, underworld::Underworld,
        unlockables::ScrapBook,
    },
    session::Session,
};
use tokio::time::sleep;

use crate::util::LockExt;
use crate::{
    AccountIdent, AttackTarget, CharacterInfo,
    config::{CharacterConfig, TargetOrder},
    login::PlayerAuth,
    message::Message,
};

pub struct AccountInfo {
    pub name: String,
    pub ident: AccountIdent,
    pub auth: PlayerAuth,
    pub last_updated: DateTime<Local>,
    /// A session that died while the app was on STOP (the player logged
    /// in elsewhere). Nothing logs in again until RESUME - a login here is
    /// exactly what throws the player out of the game they are playing
    pub parked_session: Option<Box<Session>>,
    pub status: Arc<Mutex<AccountStatus>>,
    pub scrapbook_info: Option<ScrapbookInfo>,
    pub underworld_info: Option<UnderworldInfo>,
    pub fortress_info: Option<FortressInfo>,
    /// Calculated win chances for all dungeons
    pub dungeon_info: DungeonInfo,
    /// UI state for the guild chat and the mail tab
    pub social: SocialInfo,
    pub pet_info: PetInfo,
    pub idle_info: IdleInfo,
    pub expedition_info: ExpeditionInfo,
    pub dailies_info: DailiesInfo,
    pub guard_info: GuardInfo,
    pub tavern_info: TavernInfo,
    pub shop_info: ShopInfo,
    /// Automatic actions waiting for their turn, with the time they got in
    pub queue: Vec<(Job, DateTime<Local>)>,
    /// When the last queued action was started
    pub last_dispatch: Option<DateTime<Local>>,
    /// Nothing from the queue before this: the character's own delay into
    /// a session, so several of them never start together (see `stealth`)
    pub not_before: Option<DateTime<Local>>,
    /// When the last requests went out, newest last - the ceiling on
    /// requests a minute is measured against this
    pub request_times: std::collections::VecDeque<DateTime<Local>>,
    /// The last job that was started and when. The side panel shows this
    /// instead of the "Working: ..." line that only exists while a request
    /// is in flight - at full speed that is a few hundred milliseconds, so
    /// it only ever flashed past
    pub last_job_done: Option<(&'static str, DateTime<Local>)>,
    /// Seconds to wait after it before the next one (drawn per job)
    pub gap: i64,
    /// The level this character was on when we last looked. The game does
    /// not announce a level up anywhere the app can see, so it is noticed by
    /// the number changing
    pub last_level: Option<u16>,
    /// How many epic items this character had the last time anybody looked -
    /// worn plus everything in the bag. `None` is the first look, which is
    /// never worth an alarm: the epics already there are not news
    pub epics_seen: Option<usize>,
    /// Potions active on the last look (6.1) - one more is a potion drunk,
    /// and the assistant has a word for it. `None` is the first look
    pub potions_seen: Option<usize>,
    /// The idents of every epic seen so far (worn and in the bag), so a
    /// new one is told apart from the old ones around it
    pub epic_idents: std::collections::HashSet<String>,
    /// A level up worth celebrating: the new level and when it happened
    pub level_up: Option<(u16, DateTime<Local>)>,
    /// How many dungeons were finished when we last looked. The tenth enemy
    /// of a dungeon is its boss, so a dungeon going from open to finished is
    /// a boss that just went down
    pub dungeons_cleared: Option<usize>,
    /// The mushroom count at the start of today (the date it was taken on).
    /// The rule: every day ends with at least one mushroom more than it
    /// began with, so the automation may only spend what is above that
    pub mushroom_day: Option<(chrono::NaiveDate, u32)>,
    /// Switches handed over by the previous version, applied once the
    /// login step has put the config defaults in place (see `AutoFlags`)
    pub pending_flags: Option<AutoFlags>,
}

/// Every automation switch of a character, as one plain value. An update
/// hands it to the next version: a Steam or Google character has no entry
/// in `helper.toml`, so its switches live in memory only - the version that
/// took over read "nothing configured" and came up with everything off,
/// and the player had to press ON again after every update
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AutoFlags {
    pub auto_battle: bool,
    pub uw_lure: bool,
    pub uw_collect: bool,
    pub uw_build: bool,
    pub fort_attack: bool,
    pub fort_train: bool,
    pub fort_collect: bool,
    pub fort_gems: bool,
    pub fort_build: bool,
    pub fort_recruit_all: bool,
    pub dungeon: bool,
    pub dungeon_portal: bool,
    pub expedition: bool,
    pub quest: bool,
    pub pets_feed: bool,
    pub pets_arena: bool,
    pub idle_upgrade: bool,
    #[serde(default)]
    pub idle_collect: bool,
    pub idle_sacrifice: bool,
    /// Not a switch, but it lives in memory only for a Steam character and
    /// the sacrifice ladder must not start over after every update
    #[serde(default)]
    pub idle_last_sacrifice: u64,
    pub guard: bool,
    pub beer: bool,
    #[serde(default)]
    pub mushroom_reserve: u32,
    #[serde(default)]
    pub beer_free: bool,
    #[serde(default)]
    pub beer_unaffordable_at: Option<(u32, bool)>,
    pub wheel: bool,
    pub dice: bool,
    pub shop: bool,
    pub attr: bool,
    pub dailies: bool,
    pub mount: bool,
    pub guild_attack: bool,
    pub guild_defense: bool,
    pub guild_portal: bool,
    #[serde(default)]
    pub guild_raid: bool,
    #[serde(default)]
    pub guild_finder: bool,
}

impl AutoFlags {
    pub fn of(a: &AccountInfo) -> Self {
        let uw = a.underworld_info.as_ref();
        let fi = a.fortress_info.as_ref();
        AutoFlags {
            auto_battle: a
                .scrapbook_info
                .as_ref()
                .is_some_and(|s| s.auto_battle),
            uw_lure: uw.is_some_and(|u| u.auto_lure),
            uw_collect: uw.is_some_and(|u| u.auto_collect),
            uw_build: uw.is_some_and(|u| u.auto_build),
            fort_attack: fi.is_some_and(|f| f.auto_attack),
            fort_train: fi.is_some_and(|f| f.auto_train),
            fort_collect: fi.is_some_and(|f| f.auto_collect),
            fort_gems: fi.is_some_and(|f| f.auto_gems),
            fort_build: fi.is_some_and(|f| f.auto_build),
            fort_recruit_all: fi.is_some_and(|f| f.recruit_all),
            dungeon: a.dungeon_info.auto,
            dungeon_portal: a.dungeon_info.portal_auto,
            expedition: a.expedition_info.auto,
            quest: a.expedition_info.quest_auto,
            pets_feed: a.pet_info.auto_feed,
            pets_arena: a.pet_info.auto_arena,
            idle_upgrade: a.idle_info.auto_upgrade,
            idle_collect: a.idle_info.auto_collect,
            idle_sacrifice: a.idle_info.auto_sacrifice,
            idle_last_sacrifice: a.idle_info.last_sacrifice_runes,
            guard: a.guard_info.auto,
            beer: a.tavern_info.beer_auto,
            mushroom_reserve: a.tavern_info.mushroom_reserve,
            beer_free: a.tavern_info.beer_free,
            beer_unaffordable_at: a.expedition_info.beer_unaffordable_at,
            wheel: a.tavern_info.wheel_auto,
            dice: a.tavern_info.dice_auto,
            shop: a.shop_info.auto,
            attr: a.shop_info.attr_auto,
            dailies: a.dailies_info.auto,
            mount: a.dailies_info.mount_auto,
            guild_attack: a.social.auto_attack,
            guild_defense: a.social.auto_defense,
            guild_portal: a.social.portal_auto,
            guild_raid: a.social.raid_auto,
            guild_finder: a.social.finder_auto,
        }
    }

    /// Puts the switches back, after the config defaults were applied
    pub fn apply(&self, a: &mut AccountInfo) {
        if let Some(s) = &mut a.scrapbook_info {
            s.auto_battle = self.auto_battle;
        }
        if let Some(u) = &mut a.underworld_info {
            u.auto_lure = self.uw_lure;
            u.auto_collect = self.uw_collect;
            u.auto_build = self.uw_build;
        }
        if let Some(f) = &mut a.fortress_info {
            f.auto_attack = self.fort_attack;
            f.auto_train = self.fort_train;
            f.auto_collect = self.fort_collect;
            f.auto_gems = self.fort_gems;
            f.auto_build = self.fort_build;
            f.recruit_all = self.fort_recruit_all;
            f.auto_since = self.fort_attack.then(Local::now);
        }
        a.dungeon_info.auto = self.dungeon;
        a.dungeon_info.portal_auto = self.dungeon_portal;
        a.expedition_info.auto = self.expedition;
        // Quests are never automated any more - see the login defaults
        a.expedition_info.quest_auto = false;
        a.pet_info.auto_feed = self.pets_feed;
        a.pet_info.auto_arena = self.pets_arena;
        a.idle_info.auto_upgrade = self.idle_upgrade;
        // Never on: the Arena Manager has nothing to collect (see login)
        a.idle_info.auto_collect = false;
        a.idle_info.auto_sacrifice = self.idle_sacrifice;
        a.idle_info.last_sacrifice_runes = a
            .idle_info
            .last_sacrifice_runes
            .max(self.idle_last_sacrifice);
        a.guard_info.auto = self.guard;
        a.tavern_info.beer_auto = self.beer;
        a.tavern_info.mushroom_reserve = self.mushroom_reserve;
        a.tavern_info.beer_free = self.beer_free;
        a.expedition_info.beer_unaffordable_at = self.beer_unaffordable_at;
        a.tavern_info.wheel_auto = self.wheel;
        a.tavern_info.dice_auto = self.dice;
        a.shop_info.auto = self.shop;
        a.shop_info.attr_auto = self.attr;
        a.dailies_info.auto = self.dailies;
        a.dailies_info.mount_auto = self.mount;
        a.social.auto_attack = self.guild_attack;
        a.social.auto_defense = self.guild_defense;
        a.social.portal_auto = self.guild_portal;
        a.social.raid_auto = self.guild_raid;
        a.social.finder_auto = self.guild_finder;
    }
}

/// Another guild we looked at, boiled down to what a battle needs
#[derive(Debug, Clone)]
pub struct EnemyGuild {
    /// Guild id from the planned battle, 0 for a manual comparison
    pub id: u32,
    pub name: String,
    pub power: f64,
    pub desc: String,
}

/// The job order in use, shared by every queue
static ORDER: std::sync::RwLock<Vec<Job>> = std::sync::RwLock::new(Vec::new());

/// One automatic action. Timers do not fire commands directly any more -
/// they put a job here and the queue runs them one after another, with a
/// pause in between, so the game never sees everything at once
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Job {
    FortressCollect,
    FortressAttack,
    Expedition,
    Arena,
    Dungeon,
    GuildSign,
    Pets,
    ArenaManager,
    Lure,
    /// Collecting gold and souls in the underworld and keeping the buildings
    /// going up. The luring itself stays in `Lure`
    Underworld,
    Guard,
    Wheel,
    Shopping,
    Attributes,
    /// Daily login reward and the goblin gleeman's task chests
    Dailies,
    /// The dice game with the cup player in the tavern
    Dice,
}

impl Job {
    /// The page a job works on - what the window shows while it follows
    /// the bot
    #[must_use]
    pub fn page(self) -> crate::AccountPage {
        use crate::AccountPage as P;
        match self {
            Job::Expedition | Job::Wheel | Job::Dice | Job::Dailies => {
                P::Tavern
            }
            Job::Arena => P::Scrapbook,
            Job::Dungeon => P::Dungeons,
            Job::Pets => P::Pets,
            Job::GuildSign => P::Guild,
            Job::ArenaManager => P::ArenaManager,
            Job::Lure | Job::Underworld => P::Underworld,
            Job::Guard => P::Guard,
            Job::Shopping | Job::Attributes => P::Shops,
            Job::FortressCollect | Job::FortressAttack => P::Fortress,
        }
    }

    /// Every job, in the order the guild leader walks through the game
    /// on a normal day: the daily reward and the dice on the way in, the
    /// tavern (the thirst is the one resource that ticks away whether it
    /// is used or not), the dungeon, the fortress, the underworld, the
    /// arena and its manager, the guild, then the rest of the shopping,
    /// the pets - and the guard last, because it blocks the character for
    /// hours and may only start when nothing else wants its time
    pub const DEFAULT_ORDER: [Job; 16] = [
        Job::Dailies,
        Job::Dice,
        Job::Expedition,
        Job::Dungeon,
        Job::FortressAttack,
        Job::FortressCollect,
        Job::Underworld,
        Job::Lure,
        Job::Arena,
        Job::ArenaManager,
        Job::GuildSign,
        Job::Attributes,
        Job::Wheel,
        Job::Shopping,
        Job::Pets,
        Job::Guard,
    ];

    /// What goes into the queue the moment the full bot is switched on - and
    /// again right after an update took the character over, so the
    /// automation carries on within seconds instead of waiting for every
    /// checker's first tick. Shopping is in since the guild leader asked
    /// for the gear to be kept up on its own; the protections (epics, gems,
    /// enchanted, upgraded) hold by default. Disabled jobs are dropped by
    /// the queue itself, so this list is safe for everybody
    pub const FULL_BOT_START: [Job; 15] = [
        Job::FortressCollect,
        Job::FortressAttack,
        Job::Expedition,
        Job::Arena,
        Job::Dungeon,
        Job::GuildSign,
        Job::Pets,
        Job::ArenaManager,
        Job::Wheel,
        Job::Dice,
        Job::Shopping,
        Job::Attributes,
        Job::Lure,
        Job::Underworld,
        Job::Guard,
    ];

    /// Stable name for the config file
    pub fn key(self) -> &'static str {
        match self {
            Job::FortressCollect => "fortress_collect",
            Job::FortressAttack => "fortress_attack",
            Job::Expedition => "expedition",
            Job::Arena => "arena",
            Job::Dungeon => "dungeon",
            Job::GuildSign => "guild",
            Job::Pets => "pets",
            Job::ArenaManager => "arena_manager",
            Job::Lure => "lure",
            Job::Underworld => "underworld",
            Job::Guard => "guard",
            Job::Wheel => "wheel",
            Job::Shopping => "shopping",
            Job::Attributes => "attributes",
            Job::Dailies => "dailies",
            Job::Dice => "dice",
        }
    }

    pub fn from_key(key: &str) -> Option<Job> {
        Job::DEFAULT_ORDER.into_iter().find(|j| j.key() == key)
    }

    /// One walk through the tavern, in the order a player does it: the
    /// daily reward on the way in, the dice with the cup player, the
    /// thirst (beer, expeditions, quests), the wheel on the way out. The
    /// queue keeps these together instead of letting the expedition chain
    /// hold the character while the rest of the tavern waits
    pub const TAVERN: [Job; 4] =
        [Job::Dailies, Job::Dice, Job::Expedition, Job::Wheel];

    /// What this job is worth right now, higher first. The queue used to
    /// be a fixed list - position one to sixteen, the same at four in the
    /// morning as at the moment a free arena fight has been sitting there
    /// for nine minutes. That is a running order, not a decision.
    ///
    /// The score is three things multiplied:
    ///
    /// * **where the player put it** - the configured order still counts,
    ///   as a preference between two jobs that are equally due
    /// * **how close it is to being lost** - a cooldown that has already
    ///   expired is worth several times one that has an hour to run; a
    ///   thirst near its hundred-minute ceiling, a fortress store near
    ///   its limit, a guard shift waiting to be collected
    /// * **how long it has waited** - so nothing starves behind a job
    ///   that is always slightly more urgent
    ///
    /// Anything the app cannot read scores a flat one, so a job whose
    /// state is unknown behaves exactly as it did before
    #[must_use]
    pub fn score(
        self,
        gs: &sf_api::gamestate::GameState,
        waited_secs: i64,
    ) -> f64 {
        let now = Local::now();
        let mins_to = |t: Option<DateTime<Local>>| {
            t.map_or(0.0, |t| (t - now).num_seconds() as f64 / 60.0)
        };
        // A window that has already closed is due; one still running is
        // worth less the longer it has to run
        let due = |left_min: f64| {
            if left_min <= 0.0 {
                3.0
            } else {
                (1.0 / (1.0 + left_min / 10.0)).max(0.15)
            }
        };
        let urgency = match self {
            Job::Arena => due(mins_to(gs.arena.next_free_fight)),
            Job::Dungeon => due(mins_to(gs.dungeons.next_free_fight)),
            // The thirst ticks away at its ceiling: the fuller it is, the
            // more of the day is being thrown away by not spending it
            Job::Expedition => {
                let thirst = f64::from(gs.tavern.thirst_for_adventure_sec);
                if thirst <= 0.0 {
                    0.3
                } else {
                    1.0 + 2.0 * (thirst / 6000.0).min(1.0)
                }
            }
            // A full store means the rest of the day's production is lost
            Job::FortressCollect => gs.fortress.as_ref().map_or(1.0, |f| {
                let fullest = f
                    .resources
                    .values()
                    .filter(|r| r.limit > 0)
                    .map(|r| r.current as f64 / r.limit as f64)
                    .fold(0.0_f64, f64::max);
                1.0 + 3.0 * fullest.min(1.0)
            }),
            Job::Dice => {
                if gs.tavern.dice_game.remaining > 0 {
                    1.5
                } else {
                    0.2
                }
            }
            _ => 1.0,
        };
        // Where the player put it: first in the order counts fully, last
        // a little over half
        let n = Job::order().len().max(1) as f64;
        let place = 1.0 - 0.45 * (f64::from(self.priority()) / n);
        // Nothing starves: ten minutes in the queue is worth as much as
        // being first in the order
        let waited = 1.0 + (waited_secs as f64 / 600.0).min(1.0);
        place * urgency * waited
    }

    /// Lower runs first when several are waiting. The order comes from the
    /// settings (see `set_order`); unknown jobs go last
    pub fn priority(self) -> u8 {
        let order = ORDER.read().map(|o| o.clone()).unwrap_or_default();
        order
            .iter()
            .position(|j| *j == self)
            .map(|p| p as u8)
            .unwrap_or(u8::MAX)
    }

    /// Installs the order from the config. Jobs missing in the list are
    /// appended in default order, so a new job never disappears
    pub fn set_order(keys: &[String]) {
        let mut order: Vec<Job> =
            keys.iter().filter_map(|k| Job::from_key(k)).collect();
        for j in Job::DEFAULT_ORDER {
            if !order.contains(&j) {
                order.push(j);
            }
        }
        if let Ok(mut o) = ORDER.write() {
            *o = order;
        }
    }

    pub fn order() -> Vec<Job> {
        ORDER.read().map(|o| o.clone()).unwrap_or_default()
    }

    pub fn label(self) -> &'static str {
        match self {
            Job::FortressCollect => "Fortress collect / gems / build",
            Job::FortressAttack => "Fortress attack",
            Job::Expedition => "Tavern (expedition / quest)",
            Job::Arena => "Arena attack",
            Job::Dungeon => "Dungeon",
            Job::GuildSign => "Guild sign-up",
            Job::Pets => "Pets (feeding / pet arena)",
            Job::ArenaManager => "Arena Manager",
            Job::Lure => "Underworld lure",
            Job::Underworld => "Underworld collect / build",
            Job::Guard => "City guard",
            Job::Wheel => "Wheel of fortune",
            Job::Shopping => "Shopping",
            Job::Attributes => "Attributes",
            Job::Dailies => "Daily reward / gleeman chests",
            Job::Dice => "Game of dice",
        }
    }

    /// The message that actually does the work
    pub fn message(self, ident: AccountIdent) -> Message {
        match self {
            Job::FortressCollect => Message::FortressMaintainTimer { ident },
            Job::FortressAttack => Message::AutoFortressPossible { ident },
            Job::Expedition => Message::ExpeditionTimer { ident },
            Job::Arena => Message::AutoBattlePossible { ident },
            Job::Dungeon => Message::AutoDungeonTick { ident },
            Job::GuildSign => Message::GuildAutoTick { ident },
            Job::Pets => Message::PetFeedTimer { ident },
            Job::ArenaManager => Message::IdleTimer { ident },
            Job::Lure => Message::AutoLurePossible { ident },
            Job::Underworld => Message::UnderworldMaintainTimer { ident },
            Job::Guard => Message::GuardTimer { ident },
            Job::Wheel => Message::WheelTimer { ident },
            Job::Shopping => Message::ShopTimer { ident },
            Job::Attributes => Message::AttrTimer { ident },
            Job::Dailies => Message::DailiesTimer { ident },
            Job::Dice => Message::DiceTimer { ident },
        }
    }
}

/// One dungeon, the enemy waiting there and how likely we are to win
#[derive(Debug, Clone)]
pub struct DungeonChance {
    pub dungeon: Dungeon,
    pub name: String,
    pub enemy: String,
    pub enemy_level: u16,
    /// How many enemies of this dungeon are already done
    pub stage: u16,
    /// 0.0 - 1.0, NaN when there is nothing to simulate. May instead hold an
    /// empirical estimate filled in from past fights when `no_data` is set
    pub win: f64,
    pub shadow: bool,
    /// Whether the dungeon can be entered right now
    pub open: bool,
    /// The library has no damage figures for this enemy, so the fight can not
    /// be simulated. Such a fight is only ever taken with the free fight (a
    /// probe to learn its odds), never a mushroom, and its `win` - if finite -
    /// is an empirical estimate, not a simulation
    pub no_data: bool,
    /// The enemy's level less the character's.
    ///
    /// This is what makes a record of past fights worth anything for a
    /// dungeon you walk through: every win moves you to the next floor,
    /// so a tally kept per floor has nothing in it for the floor you are
    /// about to fight - it only ever fills up when you *lose*. The gap
    /// generalises: "enemies about twenty levels under me go my way four
    /// times in five" is a sentence that survives moving on
    pub level_gap: i16,
}

#[derive(Debug, Default)]
pub struct DungeonInfo {
    pub chances: Vec<DungeonChance>,
    pub computed: Option<DateTime<Local>>,
    /// When the dungeon screen was last asked for. A burst of triggers
    /// after a relogin sent "open dungeons" twelve times in three seconds;
    /// the list does not change on that timescale, so a fresh answer is
    /// reused unless the refresh is forced (a fight just ended, the button)
    pub refreshed_at: Option<DateTime<Local>>,
    /// Hide dungeons, that are locked or already finished
    pub only_open: bool,
    pub status: Option<String>,
    /// Time the last simulation took, just so the user knows why it lags
    pub sim_millis: u128,
    /// Everything that happened on this tab, newest last
    pub log: Vec<(DateTime<Local>, String, bool)>,
    /// Spend the free fight automatically on the best dungeon
    pub auto: bool,
    /// Minimum simulated win chance (percent) for an automatic fight
    pub min_chance: u8,
    /// Pet habitat dungeons before the normal ones
    pub pets_first: bool,
    /// Guards against fighting in a tight loop, should the server not move
    /// the free-fight timer for some reason
    pub last_auto_fight: Option<DateTime<Local>>,
    /// The pet habitat fights turn out to have their own timer, which the
    /// game does not tell us about. After the server refuses one, they are
    /// left alone until this time and the normal dungeons get the turn
    pub pets_blocked_until: Option<DateTime<Local>>,
    /// The pet selected (opened) for the coming habitat fight - the game
    /// fights with the clicked pet, so the strongest one is clicked first
    pub pet_opened_for: Option<u32>,
    /// Once the free fight is used, keep fighting by spending a mushroom -
    /// but only on a dungeon whose odds clear `mushroom_min`, and only up to
    /// `mushroom_max` a day. Off by default: mushrooms are premium currency
    /// and this is released to a whole guild, so nobody spends one without
    /// turning this on themselves
    pub mushroom: bool,
    /// Lowest win chance (percent) worth a mushroom
    pub mushroom_min: u8,
    /// A hard daily ceiling on mushroom dungeon fights, so a runaway loop can
    /// never drain a stash
    pub mushroom_max: u8,
    /// How many mushroom fights already spent today, and on which day
    pub mushroom_today: u8,
    pub mushroom_day: Option<chrono::NaiveDate>,
    /// Fight the daily demonic portal on its own
    pub portal_auto: bool,
    /// The day the demonic portal was last attempted, so it is tried once a
    /// day and not on every tick
    pub portal_tried: Option<chrono::NaiveDate>,
    /// The day we last spent the free fight probing an enemy the simulation
    /// can not rate (the twister), so it happens at most once a day
    pub probe_tried: Option<chrono::NaiveDate>,
    /// The dungeon and floor of a fight on such an enemy that is in flight,
    /// captured at dispatch so its win/loss can be credited to the right
    /// floor once the server answers
    pub pending_probe: Option<(sf_api::gamestate::dungeons::Dungeon, u16, i16)>,
}

/// Automatic expeditions
#[derive(Debug, Default)]
pub struct ExpeditionInfo {
    pub auto: bool,
    /// When the "done for today" hand-over to the guard was last queued
    pub guard_handoff: Option<DateTime<Local>>,
    /// The server day the character's state was read on. The poll does not
    /// carry the tavern (thirst, beers), so after midnight the state still
    /// showed yesterday's empty tavern: beers got refused with "not
    /// thirsty" and the guard started a ten hour shift on top of a full
    /// day's thirst. A new day means one relogin, which reads everything
    pub day_read: Option<chrono::NaiveDate>,
    /// No beer before this - set when the game refused one as "not
    /// thirsty", which is the state lying, not the beer
    pub beer_blocked_until: Option<DateTime<Local>>,
    /// Do not repeat an affordability refusal until funds increase or the VIP setting changes.
    pub beer_unaffordable_at: Option<(u32, bool)>,
    /// When the tavern step last found the day finished - no thirst, no
    /// beer left to buy (or afford), no expedition running. The guard only
    /// ever starts on this verdict, never on a snapshot of the state at
    /// the moment ON was pressed, which could be yesterday's
    pub tavern_done_at: Option<DateTime<Local>>,
    /// A run of beers is in progress: started when nothing fitted into
    /// the thirst, kept up until the thirst is full again (five beers from
    /// empty). Beers are never sipped one at a time between expeditions
    pub beer_batch: bool,
    /// The last tavern step found nothing to send (waiting on a timer,
    /// quests off, thirst gone). While it is false the tavern has work in
    /// hand and the queue lets nothing else of this character in between -
    /// the leader's rule: the thirst first, everything else after
    pub stalled: bool,
    pub goal: crate::expedition::ExpeditionGoal,
    pub skip_glass: bool,
    pub skip_mushroom: bool,
    /// Spend leftover thirst for adventure on tavern quests
    pub quest_auto: bool,
    pub status: Option<String>,
    pub log: Vec<(DateTime<Local>, String, bool)>,
    /// Requests sent since the last timer tick (loop guard)
    pub steps_this_round: u8,
    /// Polls in a row that changed nothing. A poll is the right answer when
    /// the expedition screen is stale, but repeating it forty times because
    /// the answer stays the same is just noise
    pub checks_in_a_row: u8,
}

/// Collecting the free daily things: the login calendar and the three task
/// chests the goblin gleeman hands out
#[derive(Debug, Default)]
pub struct DailiesInfo {
    pub auto: bool,
    /// Keep the gold-only horse mount rented so it never lapses. Handled by
    /// the same daily-upkeep step, so it needs `auto` on as well
    pub mount_auto: bool,
    pub status: Option<String>,
    pub log: Vec<(DateTime<Local>, String, bool)>,
    /// Requests sent since the last timer tick (loop guard)
    pub steps_this_round: u8,
    /// Vouchers (coupons, deliveries, Twitch drops) already asked for by
    /// message id - one try each, so a refusal is not repeated every round
    pub claim_tried: Vec<i64>,
}

/// Automatic city guard shifts
#[derive(Debug, Default)]
pub struct GuardInfo {
    pub auto: bool,
    pub hours: u8,
    pub cutoff_enabled: bool,
    /// "HH:MM" as typed; parsed on use
    pub cutoff: String,
    pub status: Option<String>,
    pub log: Vec<(DateTime<Local>, String, bool)>,
}

/// Automatic Arena Manager (idle game) upgrades
#[derive(Debug, Default)]
pub struct IdleInfo {
    pub auto_upgrade: bool,
    /// Collect the money of finished buildings. The command is not part of
    /// the library and is sent "raw", so it may turn out to be unsupported
    pub auto_collect: bool,
    /// Refusals of the collect command in a row. After a few the collecting
    /// switches itself off, so a wrong guess does not spam the server
    pub collect_refusals: u8,
    pub status: Option<String>,
    pub log: Vec<(DateTime<Local>, String, bool)>,
    /// Upgrades bought since the last timer tick (loop guard)
    pub bought_this_round: u8,
    pub pending_upgrade: Option<(sf_api::gamestate::idle::IdleBuildingType, u32)>,
    pub auto_sacrifice: bool,
    /// What a reset has to add, in per cent of what the runes already earn
    pub sacrifice_gain_pct: u32,
    /// The game's `resets` counter at the moment we last asked for a
    /// sacrifice. The game raises it when a reset actually happens, so a
    /// request that changed nothing (it refused, or the answer was lost)
    /// leaves this equal to the current count and we do not ask again.
    ///
    /// This used to be a rune-unit boundary - `current_runes / step` - and
    /// that was the bug behind "Sacrificed for 87711 runes: the game does
    /// not allow this right now" on repeat: `current_runes` is the runes
    /// already *owned*, so with 87 711 of them in the bank the boundary
    /// moved every twenty runes for ever and the app kept asking the game
    /// to reset a game that had nothing left to give
    pub sacrificed_at_resets: Option<u32>,
    /// What the last sacrifice was worth in runes. Informational only,
    /// carried over so the activity log can say what the last one was
    pub last_sacrifice_runes: u64,
    /// The game's reset counter and the offer, as they stood on the last
    /// look. A reset the app did not make - by hand, in the browser -
    /// shows up as the counter moving on its own, and the offer seen just
    /// before it is what that reset handed over. Without this the ladder
    /// only ever learned from the app's own resets, and an account with
    /// fifty of them by hand had nothing on record
    pub seen_resets: Option<u32>,
    pub seen_offer: u64,
    /// No purse check (one level of the cheapest building, see
    /// `mechanics::idle_cheapest_level`) before this. Set when the game
    /// refuses a purchase, so "not enough money" costs one request every
    /// five minutes and not one a second
    pub probe_paused_until: Option<DateTime<Local>>,
}

impl IdleInfo {
    /// Always derived from the selected setting, including after reconnect.
    pub fn sacrifice_target(&self) -> crate::mechanics::SacrificeBar {
        crate::mechanics::sacrifice_bar(self.sacrifice_gain_pct)
    }
}

/// One purchase in flight: the new item sits in the bag, the old one has
/// to go back to the shop
#[derive(Debug, Clone)]
pub struct PendingBuy {
    pub bag_pos: sf_api::gamestate::items::BagPosition,
    pub slot: sf_api::gamestate::items::EquipmentSlot,
    pub new_ident: sf_api::gamestate::items::ItemCommandIdent,
    /// The piece that was worn when we bought - the only thing we ever sell
    pub old_ident: Option<sf_api::gamestate::items::ItemCommandIdent>,
    /// Set once the new item was moved onto the character
    pub equipped: bool,
    pub started: DateTime<Local>,
}

/// A piece on its way onto Bert, Mark or Kunigunde - checked against the
/// companion's slot once the answer is in, the way the character's own
/// purchases are. "Bert gets a FootWear" used to be the request going out,
/// not proof that he wears it
#[derive(Debug, Clone)]
pub struct PendingCompanion {
    pub companion: sf_api::gamestate::dungeons::CompanionClass,
    pub slot: sf_api::gamestate::items::EquipmentSlot,
    pub new_ident: sf_api::gamestate::items::ItemCommandIdent,
    pub had_before: bool,
    pub what: String,
}

/// One blacksmith level on its way - checked against the wearer's slot
/// once the answer is in, the way a purchase is: the line in the
/// successes says the piece is a level higher, not that we asked
#[derive(Debug, Clone)]
pub struct PendingSmith {
    /// `None` is the character
    pub wearer: Option<sf_api::gamestate::dungeons::CompanionClass>,
    pub slot: sf_api::gamestate::items::EquipmentSlot,
    /// The level before
    pub from: u8,
    /// The metal before: a companion's piece may not come back in the
    /// answer, the purse always does
    pub metal_before: u64,
    pub what: String,
}

/// Shopping for gear and attributes
#[derive(Debug, Default)]
pub struct ShopInfo {
    /// Raise the worn gear at the blacksmith - see `crate::smith`
    pub smith_auto: bool,
    pub smith_status: Option<String>,
    /// Only this replaced item may be recycled after a confirmed equip.
    pub pending_recycle: Option<PendingBuy>,
    /// Until when the backpack's emergency tier sells instead of taking a
    /// piece apart - set when the blacksmith refuses a dismantle (most
    /// likely the day's dismantles are used up)
    pub dismantle_blocked_until: Option<DateTime<Local>>,
    /// Enchant the worn gear at the witch (see `witch_step`)
    pub witch_auto: bool,
    pub witch_status: Option<String>,
    /// Pieces the game itself refused to enchant ("cannot enchant this
    /// itemtype"), by `full_model_id`. The witch's own list says which
    /// *slots* have an enchantment unlocked, and it has nothing to say
    /// about the piece actually worn in one - so the only way to know is
    /// to be told, and then not to ask again. Keyed on the piece, not the
    /// slot, so the next pair of gloves gets its own try. Not saved: a
    /// restart is cheap and the game may change its mind
    pub witch_refused: std::collections::HashSet<u32>,
    /// The "% better" step recommended for this character from how fast
    /// it levels (see `shopping::recommended_min_gain`)
    pub recommended_gain: u8,
    /// The last "thinking" line the shop and the blacksmith put in the
    /// feed, so the same weighing is not repeated every few seconds. A
    /// cell, because the shop step holds a shared borrow of this struct
    /// while it weighs
    pub last_thought: std::cell::RefCell<Option<String>>,
    pub pending_smith: Option<PendingSmith>,
    /// Always on since the guild leader's word - OFF is the only thing
    /// that stops it
    pub auto: bool,
    pub min_gain: u8,
    pub gold_reserve: u8,
    pub protect_epics: bool,
    pub protect_enchanted: bool,
    pub protect_upgraded: bool,
    /// See `shopping::Protection::legendary_pct`; 0 = never
    pub legendary_swap_pct: u8,
    /// The Sacrifice window: the bag slot of the legendary the player
    /// pressed "let it go" on, waiting for the F that confirms it
    pub sacrifice_ask: Option<usize>,
    /// A sacrifice whose gem is out and whose dismantle comes next
    pub sacrifice_then: Option<usize>,
    pub sell_old: bool,
    /// Sell the weakest unwanted piece when the backpack runs out of room
    pub clear_backpack: bool,
    /// How many slots to keep empty
    pub keep_free: u8,
    /// Put the pieces the character does not want on Bert, Mark and
    /// Kunigunde instead of leaving them in the bag
    pub equip_companions: bool,
    pub mushrooms: bool,
    pub pending: Option<PendingBuy>,
    pub pending_companion: Option<PendingCompanion>,
    pub status: Option<String>,
    pub log: Vec<(DateTime<Local>, String, bool)>,
    pub steps_this_round: u8,
    pub attr_auto: bool,
    pub attr_w_main: u8,
    pub attr_w_con: u8,
    pub attr_w_luck: u8,
    pub attr_status: Option<String>,
    /// Last known price per attribute (silver), learned from purchases
    pub attr_last_cost: Option<u64>,
    pub attr_bought_this_round: u8,
    /// Silver price of the best piece in the shops that is worth buying
    /// but over the budget right now. While it is set, the attributes
    /// wait: gear first, the game's gear beats a single attribute point
    pub gear_wants: Option<u64>,
    /// How many times a day the shops may be rerolled when they hold
    /// nothing worth buying. 0 switches rerolling off
    pub rolls_per_day: u8,
    pub rolls_today: u8,
    /// The day `rolls_today` belongs to, so it resets at midnight
    pub roll_day: Option<chrono::NaiveDate>,
}

impl ShopInfo {
    /// Is there a reroll left today? `rolls_today` is kept in step with
    /// the on-disk ledger by the shop step, by the server's day
    #[must_use]
    pub fn may_reroll(&self) -> bool {
        self.rolls_today < self.rolls_per_day
    }

    pub fn protection(&self) -> crate::shopping::Protection {
        crate::shopping::Protection {
            epics: self.protect_epics,
            enchanted: self.protect_enchanted,
            // With the blacksmith raising the gear on its own, every worn
            // piece is soon "upgraded" - as a hard stop that would freeze
            // the whole set. The levels are weighed instead (see
            // `shopping::UPGRADE_HOLD`); the box only holds while the
            // blacksmith is left to the player
            upgraded: self.protect_upgraded && !self.smith_auto,
            legendary_pct: f64::from(self.legendary_swap_pct),
        }
    }
}

/// Beer and the wheel of fortune
#[derive(Debug, Default)]
pub struct TavernInfo {
    pub beer_auto: bool,
    pub beer_max: u8,
    /// Mushrooms kept untouched, see `CharacterConfig::mushroom_reserve`
    pub mushroom_reserve: u32,
    /// The bot's mushrooms for one server day, see
    /// `CharacterConfig::mushroom_day_budget`
    pub mushroom_budget: u32,
    /// Gone from the purse today, from the on-disk ledger
    pub mushroom_spent_today: u32,
    /// VIP: beer is free, so the mushroom reserve does not gate it
    pub beer_free: bool,
    pub wheel_auto: bool,
    pub wheel_spins: u8,
    pub wheel_status: Option<String>,
    pub wheel_log: Vec<(DateTime<Local>, String, bool)>,
    /// Spins sent since the last timer tick (loop guard)
    pub spins_this_round: u8,
    /// Play the dice game in the tavern
    pub dice_auto: bool,
    pub dice_status: Option<String>,
    pub dice_log: Vec<(DateTime<Local>, String, bool)>,
    /// How far into the current game we are: 0 nothing rolled yet, 1 the
    /// dice are on the table, 2 we re-rolled and now take what is there
    pub dice_stage: u8,
    /// The game said "cannot roll for free yet" although our timer said
    /// free - the state was stale. The next start pays with a glass
    pub dice_force_glass: bool,
    /// Commands sent since the last timer tick (loop guard)
    pub dice_this_round: u8,
    /// Pay a quicksand glass instead of waiting for the next free game
    pub dice_skip: bool,
}

/// Automatic pet feeding and the pet arena
#[derive(Debug, Default)]
pub struct PetInfo {
    pub auto_feed: bool,
    pub status: Option<String>,
    pub log: Vec<(DateTime<Local>, String, bool)>,
    /// Requests sent since the last timer tick, refused ones included. The
    /// one guard that holds no matter what else goes wrong
    pub fed_this_round: u16,
    /// The pet the last request was about, by its global id. Only ever the
    /// global id - an earlier version stored whatever id went on the wire,
    /// so a pet that was skipped never matched the pet that was tried and
    /// the same feed was sent hundreds of times
    pub last_feed: Option<(u32, u8)>,
    /// Everything the game tells us about pets, added into one number: the
    /// fruit lying in every habitat and what every pet has eaten today.
    ///
    /// A `PetFeed` answer carries no pet data to prove anything with, so
    /// until now any non-error answer was written down as a feed. That is
    /// how the log filled with "Fed Water pet #82 (3/3 today)" while the
    /// fruit never left the habitat and no pet ever gained a level. If a
    /// feed the game accepted does not move this number by a single unit,
    /// nothing happened - whatever the answer said
    pub feed_fingerprint: Option<u64>,
    /// How many accepted feeds in a row changed nothing at all
    pub phantom_feeds: u8,
    /// Hand a habitat's fruit round to the weaker pets too, once the
    /// strongest has had its three. Off by default, and deliberately so:
    /// three fruits a day is the game's limit per pet, so what the lesser
    /// ones eat today is what the strongest one does not get tomorrow
    pub spread_fruit: bool,
    /// The pet whose screen was opened and not yet fed. The official client
    /// opens a pet before it feeds it, the same way it opens the dungeon
    /// screen before a fight - and the game refuses what it was never shown
    pub opened_for: Option<u32>,
    /// The last request came back refused. A refusal is an honest "no", not
    /// a silent lie, so it must never count towards the phantom detector -
    /// doing so made the detector give up before the second convention had
    /// had a single try
    pub last_was_refusal: bool,
    /// How many fruits we have handed each pet today, by global id. The
    /// game only reports `fruits_today` when it sends the whole pet list,
    /// which a feed answer does not do - so this is what says when a pet
    /// has had its share
    pub fed_today: std::collections::HashMap<u32, u16>,
    /// Pets the server turned down today. It refuses honestly ("no fruit of
    /// that kind left"), so one refusal is the end of it
    pub done_today: std::collections::HashSet<u32>,
    /// Fruit handed out per habitat since the game last told us how much is
    /// there: (the count we were told, what we have spent since). A feed
    /// answer does not carry the new number, so it has to be tracked - but
    /// a poll in between does carry it, and then the counting starts over
    /// instead of subtracting the same fruit twice
    pub fruit_used: std::collections::HashMap<u8, (u16, u16)>,
    pub done_day: Option<chrono::NaiveDate>,
    /// Fight the pet arena opponent as well
    pub auto_arena: bool,
    pub arena_status: Option<String>,
    /// The pet arena opponent as typed in for the simulator - the game
    /// does not hand over the other side's pet
    pub sim: PetSimInput,
    /// What the simulator last worked out, and the fingerprint of the
    /// state it was worked out from. Re-done on the UI tick when that
    /// changes, never on every redraw - a fight is simulated ten thousand
    /// times
    pub sim_report: Option<(u64, PetSimReport)>,
}

/// The other side of the pet arena, as typed in. Text, not numbers: a
/// field must take whatever is typed, and the number is read off it when
/// it is used
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PetSimInput {
    pub habitat: sf_api::gamestate::unlockables::HabitatType,
    /// The opponent's pet, 0-based number in the habitat
    pub their_index: usize,
    pub their_level: String,
    pub their_pack: String,
    pub their_at100: String,
    pub their_at150: String,
    pub their_at200: String,
    pub their_gladiator: String,
}

impl Default for PetSimInput {
    fn default() -> Self {
        PetSimInput {
            habitat: sf_api::gamestate::unlockables::HabitatType::Shadow,
            their_index: 0,
            their_level: "1".to_string(),
            their_pack: "20".to_string(),
            their_at100: "0".to_string(),
            their_at150: "0".to_string(),
            their_at200: "0".to_string(),
            their_gladiator: "0".to_string(),
        }
    }
}

impl PetSimInput {
    /// The opponent as a fighter spec, the way the fields read right now.
    /// A field that does not read as a number counts as zero, and a level
    /// as at least one
    #[must_use]
    pub fn theirs(&self) -> crate::pet_sim::PetSpec {
        let num = |s: &str| s.trim().parse::<u32>().unwrap_or(0);
        let pack = crate::pet_sim::Pack {
            owned: num(&self.their_pack).min(20),
            at100: num(&self.their_at100).min(20),
            at150: num(&self.their_at150).min(20),
            at200: num(&self.their_at200).min(20),
        };
        crate::pet_sim::PetSpec {
            habitat: self.habitat,
            index: self.their_index.min(crate::pet_sim::PER_HABITAT - 1),
            level: u16::try_from(num(&self.their_level).clamp(1, 500))
                .unwrap_or(1),
            bonus_pct: pack.bonus_pct(),
            gladiator: num(&self.their_gladiator).min(15),
            boss: false,
        }
    }
}

/// What the pet simulator worked out: per habitat, our pets against the
/// next boss, best first; and our strongest against the typed-in arena
/// opponent
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PetSimReport {
    /// Per habitat, in the game's order: the boss and our pets against
    /// it, best first. Empty when the habitat is explored or we have no
    /// pet there
    pub bosses: Vec<(
        sf_api::gamestate::unlockables::HabitatType,
        Option<crate::pet_sim::PetSpec>,
        Vec<crate::pet_sim::Matchup>,
    )>,
    /// Our strongest of the chosen habitat against the typed opponent
    pub arena: Option<crate::pet_sim::Matchup>,
    /// The gladiator level the figures were worked out with
    pub gladiator: u32,
}

impl PetInfo {
    /// Wipes today's bookkeeping when a new day started
    pub fn roll_day(&mut self) {
        let today = Local::now().date_naive();
        if self.done_day != Some(today) {
            self.done_day = Some(today);
            self.done_today.clear();
            self.fed_today.clear();
            self.fruit_used.clear();
        }
    }
}

/// Whose turn it is on the queues: of the characters with something
/// queued, those past their own pause (`gap` seconds since
/// `last_dispatch`) and free for a request (`idle`), the one that has
/// waited longest - never having run at all counts as longest
pub fn next_turn<T: Copy>(
    now: DateTime<Local>,
    candidates: &[(Option<DateTime<Local>>, i64, bool, T)],
) -> Option<T> {
    candidates
        .iter()
        .filter(|(last, gap, idle, _)| {
            *idle && last.is_none_or(|t| (now - t).num_seconds() >= *gap)
        })
        .min_by_key(|(last, ..)| *last)
        .map(|c| c.3)
}

#[cfg(test)]
mod queue_tests {
    use super::next_turn;
    use chrono::{Duration, Local};

    /// A: busy with a request, B: idle and rested, C: idle but still in
    /// its pause. B runs. It used to be A - picked for having waited
    /// longest, then found busy, and the tick ended with nobody running
    #[test]
    fn a_busy_character_does_not_block_a_ready_one() {
        let now = Local::now();
        let candidates = vec![
            (None, 5, false, 'a'),
            (Some(now - Duration::seconds(10)), 5, true, 'b'),
            (Some(now - Duration::seconds(1)), 5, true, 'c'),
        ];
        assert_eq!(next_turn(now, &candidates), Some('b'));
        // Once A is free it goes first: it never ran
        let mut free = candidates.clone();
        free[0].2 = true;
        assert_eq!(next_turn(now, &free), Some('a'));
        // Nobody rested and free: nobody runs
        let waiting = vec![(Some(now), 5, true, 'x'), (None, 5, false, 'y')];
        assert_eq!(next_turn(now, &waiting), None);
    }
}

#[cfg(test)]
mod pet_tests {
    use super::PetInfo;

    /// The loop that filled the log with hundreds of identical lines: the
    /// id that went on the wire was stored, but the skip list was read with
    /// the pet's real id, so a refused pet was picked again immediately.
    /// Both sides have to be the same number
    #[test]
    fn a_refused_pet_is_really_skipped_afterwards() {
        let mut info = PetInfo::default();
        info.roll_day();
        let global_id = 42;
        info.last_feed = Some((global_id, 2));
        // what the refusal handler does
        if let Some((id, _)) = info.last_feed {
            info.done_today.insert(id);
        }
        info.last_feed = None;
        assert!(
            info.done_today.contains(&global_id),
            "the pet that was refused has to be the pet that gets skipped"
        );
    }

    #[test]
    fn a_new_day_clears_both_lists() {
        let mut info = PetInfo::default();
        info.roll_day();
        info.done_today.insert(7);
        info.fed_today.insert(7, 3);
        info.done_day =
            Some(chrono::Local::now().date_naive() - chrono::Duration::days(1));
        info.roll_day();
        assert!(info.done_today.is_empty());
        assert!(info.fed_today.is_empty());
    }
}

/// How the guild member table is sorted
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MemberSort {
    /// The order the game's own member list has - what the leader is
    /// used to reading, so the table opens on it
    #[default]
    Game,
    Advice,
    /// Who should hold the nine officer seats: most contribution first,
    /// so the ones to promote and the ones to demote sit next to each
    /// other at the cut line
    Officers,
    Activity,
    /// Lowest Hall of Knights + instructor first - who is still waiting to be
    /// raised. Once the guild is maxed out this is flat and says nothing,
    /// which is exactly what it should look like
    ToRaise,
    Level,
    Name,
    LastOnline,
}

impl MemberSort {
    pub const ALL: [MemberSort; 8] = [
        Self::Game,
        Self::Advice,
        Self::Officers,
        Self::Activity,
        Self::ToRaise,
        Self::Level,
        Self::Name,
        Self::LastOnline,
    ];
}

impl std::fmt::Display for MemberSort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(crate::i18n::t(match self {
            MemberSort::Game => "As in the game",
            MemberSort::Advice => "Kick advice",
            MemberSort::Officers => "Officers",
            MemberSort::Activity => "Activity",
            MemberSort::ToRaise => "Needs raising",
            MemberSort::Level => "Level",
            MemberSort::Name => "Name",
            MemberSort::LastOnline => "Last online",
        }))
    }
}

#[derive(Debug, Default)]
pub struct SocialInfo {
    /// Show the chat instead of the member table. The table is what the
    /// tab opens on - it is what a leader comes for; the chat is a click
    pub show_chat: bool,
    /// The summary drawer (raids, bonuses, kick advice) is pulled out
    pub drawer_open: bool,
    /// Local roster detail; opening it never sends a game request.
    pub selected_member: Option<String>,
    /// How many times the chat refresh has found the character busy. With a
    /// full queue there is a request in flight most of the time, so a single
    /// try every fifteen seconds could miss for good and the chat stayed
    /// empty
    pub chat_attempts: u8,
    pub member_sort: MemberSort,
    /// Tight rows, so that a full guild fits on one screen
    pub compact_members: bool,
    /// Daily rating mail
    pub report_auto: bool,
    pub report_hour: u8,
    /// Members still waiting for the report mail
    pub report_queue: Vec<String>,
    pub report_text: String,
    pub report_sent: usize,
    pub report_failed: usize,
    pub report_status: Option<String>,
    /// Which auto sign-up is waiting for the "really switch off?" answer
    /// (Some(true) = defense, Some(false) = attack)
    pub confirm_off: Option<bool>,
    /// The guild we attack / that attacks us, once looked up
    pub enemies: Vec<EnemyGuild>,
    /// Guild names already in the lookup before the last `ViewGuild`, so
    /// the new one can be told apart
    pub lookup_before: Vec<String>,
    /// "Compare with guild" box
    pub compare_input: String,
    pub compare_result: Option<String>,
    /// Join guild attacks on their own
    pub auto_attack: bool,
    /// Join guild defenses on their own
    pub auto_defense: bool,
    /// Start a raid whenever none is running - see `guild_raid_auto`
    pub raid_auto: bool,
    /// Players (lowercase) already invited this session, so the recruiting
    /// list does not offer them again
    pub invited: Vec<String>,
    /// How often we signed up since the last refresh. Guards against a loop,
    /// should the server report the state differently than expected
    pub sign_attempts: u8,
    /// The guild pet's level or hydra was refused - not before this
    pub pet_blocked_until: Option<DateTime<Local>>,
    /// (battle time, when we will sign up) for the attack. Signing up the
    /// second a battle is declared looks like a bot, so it happens at a
    /// random moment within the following hour
    pub attack_sign_at: Option<(DateTime<Local>, DateTime<Local>)>,
    /// Same for the defense
    pub defense_sign_at: Option<(DateTime<Local>, DateTime<Local>)>,
    /// What the user typed into the guild chat box
    pub chat_input: String,
    /// The archive read back into the chat tab: the older lines the game
    /// has already let go of, oldest first. Empty until asked for
    pub archive_lines: Vec<String>,
    /// Whether the older lines are shown above the live chat
    pub show_archive: bool,
    /// Whether the game's own lines (level ups and the like) are left
    /// out of the older lines - they are what pushes the real messages
    /// out of the game's window in the first place
    pub archive_people_only: bool,
    /// The guild's founding date as the leader typed it, mirrored from
    /// the config so the field can be edited while it is being typed
    pub guild_founded: String,
    /// The inbox position of the message, that is currently open
    pub open_msg: Option<i32>,
    /// The content of that message. Kept here, because the game state
    /// forgets fight reports on the very next poll
    pub open_text: Option<String>,
    /// Who the reply goes to (prefilled from the opened message)
    pub reply_to: String,
    /// The reply text being typed
    pub reply_text: String,
    pub status: Option<String>,
    /// The game's guild finder, for a character without a guild
    pub finder: Vec<crate::guild_finder::FinderGuild>,
    /// Treasure / instructor sums per guild (lowercase name), from one
    /// `ViewGuild` each
    pub finder_bonus:
        std::collections::HashMap<String, crate::guild_finder::Bonus>,
    /// Guilds still waiting for their lookup
    pub finder_pending: Vec<String>,
    /// Guilds (lowercase) that already got a join request this session,
    /// so the automatic pick moves on instead of knocking twice
    pub finder_tried: Vec<String>,
    pub finder_status: Option<String>,
    /// Ask the best guild on its own - see `guild_finder_auto`
    pub finder_auto: bool,
    /// When the list was last read; the automatic pass runs every half hour
    pub finder_at: Option<DateTime<Local>>,
    /// Take the daily swing at the guild portal on its own
    pub portal_auto: bool,
    /// The day the portal was last tried. The game state has no "already
    /// fought today" flag anywhere - only the enemy's life and how often it
    /// has been beaten - so the only way not to ask the server the same
    /// question every minute is to remember that we asked
    pub portal_tried: Option<chrono::NaiveDate>,
}

impl SocialInfo {
    /// Is today's portal fight still untried?
    #[must_use]
    pub fn portal_due(&self) -> bool {
        self.portal_auto && self.portal_tried != Some(Local::now().date_naive())
    }
}

/// The three things we ever do in the fortress
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FortressActionKind {
    Attack,
    /// Get a different (random) opponent
    Reroll,
    /// The same, knowingly paying silver for it - the raid run's second
    /// way past an opponent that is not worth the soldiers
    PaidReroll,
    /// Queue up one new soldier
    Train,
}

impl FortressActionKind {
    pub fn busy_reason(self) -> &'static str {
        match self {
            FortressActionKind::Attack => "Fortress",
            FortressActionKind::Reroll | FortressActionKind::PaidReroll => {
                "New enemy"
            }
            FortressActionKind::Train => "Recruiting",
        }
    }
}

/// Everything needed to automatically battle the fortress opponent
pub struct FortressInfo {
    pub auto_attack: bool,
    /// Queue one new soldier after every attack
    pub auto_train: bool,
    /// Collect resources once the producing building is (nearly) full
    pub auto_collect: bool,
    /// Keep the gem mine busy
    pub auto_gems: bool,
    /// Upgrade buildings following a fixed priority
    pub auto_build: bool,
    /// Recruit every unit type until full
    pub recruit_all: bool,
    /// Use the game's own soldier recommendation for the opponent
    pub smart_soldiers: bool,
    /// Extra soldiers on top of that recommendation
    pub soldier_margin: u16,
    /// When the opponent needs more soldiers than we have, send one and take
    /// the loss so the next attack gets a different opponent
    pub cheap_skip: bool,
    /// Soldier count decided by the scouting step, consumed by the attack
    pub scouted_soldiers: Option<(u32, String)>,
    /// Set after a "need more units" refusal was answered with a full
    /// attack, so it happens once per refusal and not in a loop
    pub retried_full: bool,
    /// Soldiers per attack, 0 = all
    pub attack_soldiers: u16,
    /// What the maintenance tick did last
    pub maintain_status: Option<String>,
    /// Housekeeping requests since the last timer (loop guard)
    pub maint_steps: u8,
    /// Gem searching is refused while something is being built. After such a
    /// refusal no new search is attempted until this time
    pub gems_blocked_until: Option<DateTime<Local>>,
    /// The game refused the Hall of Knights (level cap, resources read
    /// wrong) - not asked again before this
    pub hok_blocked_until: Option<DateTime<Local>>,
    /// Which gems to keep. Empty = keep every one of them
    pub gem_wanted: Vec<crate::config::GemWanted>,
    /// Socket found gems into empty slots, matched to what each wearer
    /// fights with
    pub gem_socket: bool,
    /// About how many seconds between two attacks - the average of the
    /// draw, never the exact gap (see `stealth::fortress_gap_secs`)
    pub interval: u64,
    /// When the next automatic attack is due - drawn after every attack
    /// and on switching the attacks on; none until then
    pub next_due: Option<DateTime<Local>>,
    pub log: Vec<(DateTime<Local>, String, bool)>,
    pub status: Option<String>,
    pub last_attack: Option<DateTime<Local>>,
    /// When the automatic attacks were switched on. Together with
    /// `last_attack` this is what the countdown in the UI is based on
    pub auto_since: Option<DateTime<Local>>,
    /// Where the raid cycle stands: filling the barracks, waiting out the
    /// notice delay with them full, or in the middle of a run of attacks
    pub raid: RaidPhase,
    /// The most the raid waits after the barracks fill, in minutes. The
    /// actual wait is drawn between `mechanics::NOTICE_MIN_SECS` and this
    pub notice_max_min: u32,
}

/// The raid cycle, one phase at a time. See `mechanics::raid_step`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RaidPhase {
    /// Soldiers being trained, or waiting for the stock to train them
    #[default]
    Filling,
    /// Barracks full; the run starts at `FortressInfo::next_due`
    Noticed,
    /// Attacking one opponent after another until the soldiers run out
    Raiding {
        /// Attacks sent in this run - a loop guard, nothing more
        attacks: u8,
        /// Whether the one paid new opponent this run allows was bought
        paid_reroll: bool,
    },
}

impl FortressInfo {
    /// Whether any of the periodic housekeeping is switched on
    pub fn maintains(&self) -> bool {
        self.auto_collect
            || self.auto_gems
            || self.auto_build
            || self.recruit_all
    }

    /// When the next automatic attack is going to happen - the drawn
    /// moment, or the plain average until the first draw
    pub fn next_attack(&self) -> Option<DateTime<Local>> {
        if !self.auto_attack {
            return None;
        }
        // Only a drawn moment counts. There used to be a fallback of
        // "last attack plus the interval", which now would promise an
        // attack the cycle has no intention of making while the barracks
        // are still filling
        self.next_due
    }
}

impl FortressInfo {
    pub fn new(
        gs: &GameState,
        config: Option<&CharacterConfig>,
    ) -> Option<Self> {
        gs.fortress.as_ref()?;
        let interval = config
            .map(|a| a.fortress_interval)
            .filter(|a| *a >= 30)
            .unwrap_or(180);
        Some(Self {
            auto_attack: config.map(|a| a.auto_fortress).unwrap_or(false),
            auto_train: config.map(|a| a.fortress_train).unwrap_or(false),
            auto_collect: config.map(|a| a.fortress_collect).unwrap_or(true),
            auto_gems: config.map(|a| a.fortress_gems).unwrap_or(true),
            auto_build: config.map(|a| a.fortress_build).unwrap_or(false),
            recruit_all: config
                .map(|a| a.fortress_recruit_all)
                .unwrap_or(false),
            smart_soldiers: config
                .map(|a| a.fortress_smart_soldiers)
                .unwrap_or(true),
            soldier_margin: config
                .map(|a| a.fortress_soldier_margin)
                .unwrap_or(0),
            cheap_skip: config.map(|a| a.fortress_cheap_skip).unwrap_or(true),
            scouted_soldiers: None,
            retried_full: false,
            attack_soldiers: config
                .map(|a| a.fortress_attack_soldiers)
                .unwrap_or(0),
            maintain_status: None,
            maint_steps: 0,
            gems_blocked_until: None,
            hok_blocked_until: None,
            gem_wanted: Vec::new(),
            gem_socket: false,
            interval,
            next_due: None,
            log: Vec::new(),
            status: None,
            last_attack: None,
            auto_since: config
                .map(|a| a.auto_fortress)
                .unwrap_or(false)
                .then(Local::now),
            raid: RaidPhase::Filling,
            notice_max_min: config
                .map(|a| a.fortress_notice_max_min)
                .unwrap_or(60),
        })
    }
}

pub struct UnderworldInfo {
    pub underworld: Underworld,
    pub best: Vec<CharacterInfo>,
    pub max_level: u16,
    pub attack_log: Vec<(DateTime<Local>, String, bool)>,
    pub auto_lure: bool,
    /// Collect gold and souls once a production building is nearly full
    pub auto_collect: bool,
    /// Keep the buildings going up, by the order in `underworld_priority`
    pub auto_build: bool,
    /// What the last housekeeping step did - the page shows this
    pub maintain_status: Option<String>,
    /// Steps taken in this turn. One job may not hold the character for
    /// minutes, the rest of the queue is waiting
    pub maint_steps: u8,
    /// When each resource (souls / silver / thirst, indexed by its number) was
    /// last collected. The game's collect reply does not always reset the
    /// stored amount, so without this the estimate stays "nearly full" and the
    /// collect fires again on every tick - dozens a second. A short cooldown
    /// per resource makes it the one click it should be
    pub collected_at: [Option<DateTime<Local>>; 3],
    /// How full (0.35 - 0.9) each store has to be before it is collected,
    /// drawn anew after every collect - so the pickups are frequent but
    /// never at the same fill level or on the same clock twice
    pub collect_frac: [f32; 3],
    /// The day the Gold Pit was last raised. The guild leader's rule:
    /// it only goes up when there is nothing else to build, and at most
    /// once a day - it is the one building down here that pays in silver
    /// rather than in anything the character carries into a fight, so a
    /// run of cheap levels on it is souls that the other three wanted.
    /// Carried in the config so that an update, which restarts the whole
    /// app, does not hand out a second one
    pub gold_pit_day: Option<chrono::NaiveDate>,
    /// Which build order is followed - see `mechanics::underworld_order`
    pub phase: crate::mechanics::UnderworldPhase,
    /// What the store held right before a collect went out (the souls, or
    /// the silver), so the answer can say how much actually came in - a
    /// "Collected Souls" line with no number proved nothing for weeks
    pub collect_before:
        Option<(sf_api::gamestate::underworld::UnderworldResourceType, u64)>,
}

impl UnderworldInfo {
    pub fn new(
        gs: &GameState,
        config: Option<&CharacterConfig>,
    ) -> Option<Self> {
        let underworld = gs.underworld.as_ref()?.clone();
        let avg_lvl = underworld
            .units
            .as_array()
            .iter()
            .map(|a| a.level as u64)
            .sum::<u64>() as f32
            / 3.0;
        Some(Self {
            underworld,
            gold_pit_day: None,
            best: Default::default(),
            max_level: avg_lvl as u16 + 20,
            attack_log: Vec::new(),
            auto_lure: config.map(|a| a.auto_lure).unwrap_or(false),
            auto_collect: config.map(|a| a.underworld_collect).unwrap_or(false),
            auto_build: config.map(|a| a.underworld_build).unwrap_or(false),
            maintain_status: None,
            maint_steps: 0,
            collected_at: [None; 3],
            collect_frac: [
                Self::draw_frac(),
                Self::draw_frac(),
                Self::draw_frac(),
            ],
            phase: config.map(|a| a.underworld_phase).unwrap_or_default(),
            collect_before: None,
        })
    }

    /// A fresh "collect when this full" share. Never below 35 % - a pickup
    /// of a few souls is pointless - and never above 90 %, where the old
    /// fixed rule sat and a full store would start to waste production
    pub fn draw_frac() -> f32 {
        fastrand::f32() * 0.55 + 0.35
    }
}

/// One of the three opponents the game itself puts up in the arena. What
/// the arena screen shows: who, how strong, and how they compare to us
#[derive(Debug, Clone)]
pub struct ArenaOpponent {
    pub pid: u32,
    pub name: String,
    pub level: u16,
    pub class: sf_api::gamestate::character::Class,
    pub guild: Option<String>,
    /// Strength, dexterity, intelligence, constitution, luck - base plus
    /// gear, the figures the arena screen prints
    pub attrs: [u32; 5],
    /// The five added up - the same measure the target list is judged by
    pub stats: u32,
    /// Simulated chance to win, 0.0 - 1.0: the same battle simulation the
    /// dungeons use, run on their gear and class against ours. A sum of
    /// attributes says nothing about a mage against a warrior; this does
    pub win: f64,
}

impl ArenaOpponent {
    /// `us` is our own fighter, built once per offer from the game state
    pub fn from_other(
        o: &sf_api::gamestate::social::OtherPlayer,
        us: &sf_api::simulate::Fighter,
    ) -> Self {
        use sf_api::command::AttributeType as A;
        use sf_api::simulate::{Fighter, UpgradeableFighter, simulate_battle};
        let at = |a: A| o.attribute_basis[a] + o.attribute_additions[a];
        let attrs = [
            at(A::Strength),
            at(A::Dexterity),
            at(A::Intelligence),
            at(A::Constitution),
            at(A::Luck),
        ];
        let them = Fighter::from(&UpgradeableFighter::from_other(o));
        let win = simulate_battle(
            std::slice::from_ref(us),
            &[them],
            crate::dungeon::SIM_ITERATIONS,
            false,
        )
        .win_ratio;
        Self {
            pid: o.player_id,
            name: o.name.clone(),
            level: o.level,
            class: o.class,
            guild: o.guild.clone(),
            attrs,
            stats: attrs.iter().sum(),
            win,
        }
    }
}

pub struct ScrapbookInfo {
    pub scrapbook: ScrapBook,
    pub best: Vec<AttackTarget>,
    /// The game's own offer: the three arena opponents, fetched with
    /// CheckArena + a look at each. Empty until the first refresh
    pub arena_offer: Vec<ArenaOpponent>,
    pub arena_offer_at: Option<DateTime<Local>>,
    /// True while CheckArena + the three look-ups are in flight
    pub arena_offer_busy: bool,
    /// Free re-rolls of the game's arena offer used in this attempt.
    /// `CheckArena` puts up three new opponents and costs nothing, so a
    /// trio nobody can beat is worth throwing back - but only a couple of
    /// times, and never in a circle (see `offer_seen_ids`)
    pub offer_rerolls: u8,
    /// The opponents of the offer we last threw back, so a game that
    /// hands the same three straight back ends it instead of being asked
    /// for ever
    pub offer_seen_ids: Vec<u32>,
    /// When that started. A fetch that ran into a relogin never reported
    /// back and left the panel on "fetching..." for good, so an old flag is
    /// not trusted - see `offer_stuck()`
    pub arena_offer_started: Option<DateTime<Local>>,
    /// The fight that is on the wire: who, and the chance the app believed
    /// when it sent it. The result handler takes it out and files the pair
    /// in the fight log, which is what calibrates the next estimate (see
    /// `fight_log`). `None` for a fight nobody predicted
    pub pending_fight: Option<(String, Option<f64>)>,
    /// A greedily calculated sequence of fights. Every entry only counts the
    /// items, that the fights before it did not already provide
    pub plan: Vec<AttackTarget>,
    pub show_plan: bool,
    pub max_level: u16,
    pub max_attributes: u32,
    /// Our own total attributes. Used to judge how risky a fight is
    pub own_stats: u32,
    pub order: TargetOrder,
    /// Free text filter for the target list
    pub filter: String,
    pub blacklist: IntMap<u32, (String, usize)>,
    pub attack_log: Vec<(DateTime<Local>, AttackTarget, bool)>,
    pub auto_battle: bool,
    /// Why auto battle is currently not doing anything
    pub auto_status: Option<String>,
    pub losses_in_a_row: usize,
    /// When the last automatic attack was sent. The free fight is once every
    /// ten minutes, so if the game's own timer does not move - a refused
    /// fight, a stale game state - this stops the app from attacking in a
    /// tight loop and hammering the server
    pub last_auto_attack: Option<DateTime<Local>>,
    /// Once the free arena fight is on cooldown, keep attacking by spending a
    /// mushroom - up to `mushroom_max` a day. Off by default: mushrooms are
    /// premium currency and this ships to a whole guild
    pub mushroom: bool,
    pub mushroom_max: u8,
    pub mushroom_today: u8,
    pub mushroom_day: Option<chrono::NaiveDate>,
}

impl ScrapbookInfo {
    pub fn new(
        gs: &GameState,
        config: Option<&CharacterConfig>,
        order: TargetOrder,
    ) -> Option<Self> {
        let own_stats = total_attributes(gs);
        let expected_battle_luck = 1.2f32;

        Some(Self {
            scrapbook: gs.character.scrapbook.as_ref()?.clone(),
            best: Default::default(),
            arena_offer: Vec::new(),
            arena_offer_at: None,
            arena_offer_busy: false,
            offer_rerolls: 0,
            offer_seen_ids: Vec::new(),
            arena_offer_started: None,
            pending_fight: None,
            plan: Default::default(),
            show_plan: false,
            max_level: gs.character.level,
            max_attributes: (own_stats as f32 * expected_battle_luck) as u32,
            own_stats,
            order,
            filter: String::new(),
            blacklist: Default::default(),
            attack_log: Default::default(),
            auto_battle: config.map(|a| a.auto_battle).unwrap_or(false),
            auto_status: None,
            losses_in_a_row: 0,
            last_auto_attack: None,
            mushroom: config.map(|a| a.arena_mushroom).unwrap_or(false),
            mushroom_max: config.map(|a| a.arena_mushroom_max).unwrap_or(5),
            mushroom_today: 0,
            mushroom_day: None,
        })
    }

    /// Keeps everything, that the game state owns, up to date
    pub fn refresh(&mut self, gs: &GameState) {
        self.own_stats = total_attributes(gs);
    }

    /// The opponent in the game's offer we are most likely to beat - at
    /// least an even fight, the attribute limit only the tie-breaker.
    ///
    /// `None` when all three are under fifty, and that is not the end of
    /// it: `CheckArena` puts up three new opponents for nothing, so the
    /// caller throws the trio back and asks again (see `offer_rerolls`).
    ///
    /// The chance is the simulation's, bent by what really happened in
    /// fights it was this confident about and by this opponent's own
    /// record (see `fight_log`). Somebody the simulation likes and you
    /// have lost to twice is no longer "most likely to beat"
    pub fn best_offer(
        &self,
        log: &crate::fight_log::FightLog,
        key: &str,
    ) -> Option<&ArenaOpponent> {
        let chance = |o: &ArenaOpponent| log.calibrated(key, &o.name, o.win);
        self.arena_offer
            .iter()
            .filter(|o| chance(o) >= 0.5)
            .max_by(|a, b| {
                chance(a)
                    .partial_cmp(&chance(b))
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(b.stats.cmp(&a.stats))
            })
    }

    /// A fetch older than this never came back - four requests take
    /// seconds, not minutes
    pub fn offer_stuck(&self) -> bool {
        self.arena_offer_busy
            && self.arena_offer_started.is_none_or(|t| {
                Local::now() - t > chrono::Duration::seconds(90)
            })
    }

    /// Fresh enough to act on: the offer changes after every fight, and the
    /// free fight comes every ten minutes
    pub fn offer_fresh(&self) -> bool {
        self.arena_offer_at
            .is_some_and(|t| Local::now() - t < chrono::Duration::minutes(12))
    }

    /// (wins, losses, new items) of this session
    pub fn session_stats(&self) -> (usize, usize, usize) {
        let mut wins = 0;
        let mut losses = 0;
        let mut items = 0;
        for (_, target, won) in &self.attack_log {
            if *won {
                wins += 1;
                items += target.missing;
            } else {
                losses += 1;
            }
        }
        (wins, losses, items)
    }
}

impl AccountInfo {
    /// Mushrooms the automation may spend on beer, shop rerolls, epics and
    /// the like: everything above the reserve the player typed in - and
    /// above the dragon's 25 while its renewal is within three days, so
    /// they are there on renewal day (the renewal itself pays from the
    /// whole balance). The "one more than the morning" rule this replaces
    /// held the beers back with 14 mushrooms in the bag and nobody could
    /// tell why; a number on the tavern page says exactly what is kept
    #[must_use]
    pub fn free_mushrooms(&self, gs: &sf_api::gamestate::GameState) -> u32 {
        use sf_api::gamestate::character::Mount;
        let renewal_near = gs
            .character
            .mount_end
            .is_none_or(|end| end <= Local::now() + chrono::Duration::days(3));
        let dragon = if self.dailies_info.mount_auto && renewal_near {
            u32::from(Mount::Dragon.cost().mushrooms)
        } else {
            0
        };
        let above_reserve = gs
            .character
            .mushrooms
            .saturating_sub(dragon.max(self.tavern_info.mushroom_reserve));
        // The day's budget is the one number the player sets for the
        // bot: beer, fights, rerolls, epics, skipped waits all come out of
        // it, and once it is gone the bot spends nothing until the
        // server's next day
        let budget_left = self
            .tavern_info
            .mushroom_budget
            .saturating_sub(self.tavern_info.mushroom_spent_today);
        above_reserve.min(budget_left)
    }

    /// The beers the day's plan still allows. The plan's share
    /// (`mechanics::mushroom_plan`) is for a normal day. Late in the server
    /// day whatever is left of the budget goes into beer instead: a
    /// mushroom that is not spent by midnight is not saved, it is a
    /// mushroom the day's mandate threw away - and a beer is the best
    /// thing one turns into. The game's own daily beer limit still holds
    /// on top of this
    #[must_use]
    pub fn beer_allowance(&self, gs: &sf_api::gamestate::GameState) -> u8 {
        use chrono::Timelike;
        let planned = self.tavern_info.beer_max;
        let left = self
            .tavern_info
            .mushroom_budget
            .saturating_sub(self.tavern_info.mushroom_spent_today);
        if left == 0 || gs.server_time().current().time().hour() < 21 {
            return planned;
        }
        planned.max(u8::try_from(left).unwrap_or(u8::MAX))
    }

    /// A request is going out at `at` - noted for the per-minute ceiling.
    /// `at` may lie a little ahead: a request held back for the ceiling
    /// is booked for the moment it will actually leave
    pub fn note_request(&mut self, at: DateTime<Local>) {
        self.request_times.push_back(at);
        // Newest last, whatever order they were booked in
        let mut v: Vec<_> = self.request_times.drain(..).collect();
        v.sort();
        self.request_times = v.into_iter().collect();
        while self
            .request_times
            .front()
            .is_some_and(|t| (at - *t).num_seconds() > 120)
        {
            self.request_times.pop_front();
        }
    }

    /// How long a request has to wait so that no more than `cap` go out in
    /// any one minute: until the oldest of the last `cap` requests is a
    /// minute old. Zero when there is room
    #[must_use]
    pub fn wait_for_cap(&self, now: DateTime<Local>, cap: usize) -> i64 {
        let cap = cap.max(1);
        if self.request_times.len() < cap {
            return 0;
        }
        let oldest = self.request_times[self.request_times.len() - cap];
        let free_at = oldest + chrono::Duration::seconds(61);
        (free_at - now).num_milliseconds().max(0)
    }

    /// Requests sent in the last minute
    #[must_use]
    pub fn requests_last_minute(&self, now: DateTime<Local>) -> usize {
        self.request_times
            .iter()
            .filter(|t| (now - **t).num_seconds() < 60)
            .count()
    }

    /// Takes today's starting mushroom count if the day has changed since
    /// the last look. Called before any job runs, so the floor is in place
    /// before anything is spent
    pub fn roll_mushroom_day(&mut self, gs: &sf_api::gamestate::GameState) {
        let today = Local::now().date_naive();
        if self.mushroom_day.is_none_or(|(d, _)| d != today) {
            self.mushroom_day = Some((today, gs.character.mushrooms));
        }
    }

    pub fn new(
        name: &str,
        auth: PlayerAuth,
        ident: AccountIdent,
    ) -> AccountInfo {
        AccountInfo {
            name: name.to_string(),
            auth,
            scrapbook_info: None,
            underworld_info: None,
            fortress_info: None,
            dungeon_info: DungeonInfo::default(),
            social: SocialInfo::default(),
            // 5.4: as many pets fed as the fruit allows, not the strongest
            // of each habitat first and the rest only from a surplus
            pet_info: PetInfo {
                spread_fruit: true,
                ..PetInfo::default()
            },
            idle_info: IdleInfo::default(),
            guard_info: GuardInfo::default(),
            tavern_info: TavernInfo::default(),
            shop_info: ShopInfo::default(),
            queue: Vec::new(),
            last_dispatch: None,
            not_before: None,
            request_times: std::collections::VecDeque::new(),
            last_job_done: None,
            last_level: None,
            mushroom_day: None,
            parked_session: None,
            pending_flags: None,
            epics_seen: None,
            potions_seen: None,
            epic_idents: std::collections::HashSet::new(),
            level_up: None,
            dungeons_cleared: None,
            gap: 5,
            expedition_info: ExpeditionInfo::default(),
            dailies_info: DailiesInfo::default(),
            last_updated: Local::now(),
            status: Arc::new(Mutex::new(AccountStatus::LoggingIn)),
            ident,
        }
    }
}

pub enum AccountStatus {
    LoggingIn,
    Idle(Box<Session>, Box<GameState>),
    Busy(Box<GameState>, Box<str>),
    FatalError(String),
    LoggingInAgain,
}

impl AccountStatus {
    pub fn take_session<T: Into<Box<str>>>(
        &mut self,
        reason: T,
    ) -> Option<Box<Session>> {
        let mut res = None;
        *self = match std::mem::replace(self, AccountStatus::LoggingInAgain) {
            AccountStatus::Idle(a, b) => {
                res = Some(a);
                AccountStatus::Busy(b, reason.into())
            }
            x => x,
        };
        res
    }

    pub fn put_session(&mut self, session: Box<Session>) {
        *self = match std::mem::replace(self, AccountStatus::LoggingInAgain) {
            AccountStatus::Busy(a, _) => AccountStatus::Idle(session, a),
            AccountStatus::LoggingInAgain => {
                log::warn!(
                    "put_session in LoggingInAgain state - session not stored"
                );
                // Session se vrací do Idle s empty GameState - bude se přihlašovat znova
                AccountStatus::Idle(session, Box::new(GameState::default()))
            }
            x => x,
        };
    }
}

pub struct AutoAttackChecker {
    pub player_status: Arc<Mutex<AccountStatus>>,
    pub ident: AccountIdent,
}

impl AutoAttackChecker {
    pub async fn check(&self) -> Message {
        let next_fight: Option<DateTime<Local>> = {
            match &*self.player_status.lock_safe() {
                AccountStatus::Idle(_, session) => {
                    session.arena.next_free_fight
                }
                _ => None,
            }
        };
        match next_fight.map(|next| (next - Local::now()).to_std()) {
            // The timer is in the future - wake up when it runs out
            Some(Ok(remaining)) => {
                tokio::time::sleep(remaining).await;
                tokio::time::sleep(Duration::from_millis(fastrand::u64(
                    1000..=3000,
                )))
                .await;
            }
            // The timer has passed, or the game state has none yet. Asking
            // again in a second or two is how the app used to end up sending
            // attack after attack when the timer did not move - a refused
            // fight, or a game state that was not updated. The free fight is
            // once every ten minutes, so a slow retry loses nothing
            _ => {
                crate::pace::sleep_checker(45, 90).await;
            }
        }

        Message::Enqueue {
            ident: self.ident,
            job: Job::Arena,
        }
    }
}

pub struct AutoLureChecker {
    pub player_status: Arc<Mutex<AccountStatus>>,
    pub ident: AccountIdent,
}

impl AutoLureChecker {
    pub async fn check(&self) -> Message {
        // The daily lure limit is NOT five - it is the Gate's level
        // (Gate building level, capped at 5). Assuming five is why a
        // character whose gate is level 1 kept being told it had four lures
        // left: it had already used its one for the day, and the checker
        // spun on forever looking for a fight the game would never allow
        let still_to_lure = {
            match &*self.player_status.lock_safe() {
                AccountStatus::Idle(_, session) => session
                    .underworld
                    .as_ref()
                    .map(|u| crate::mechanics::remaining_lures(u) > 0),
                _ => None,
            }
        };
        let Some(true) = still_to_lure else {
            // No underworld, or every lure the gate allows is already used
            // today - nothing to do until tomorrow, so look back rarely
            tokio::time::sleep(Duration::from_millis(fastrand::u64(
                5000..=10_000,
            )))
            .await;
            return Message::AutoLureIdle;
        };

        // Five lures a day is the game's hard limit, so checking every few
        // seconds only spams the queue with a job that then bails - which is
        // exactly what filled the live feed with "Underworld lure" 200 times
        // a session. A minute or more between looks is still far more often
        // than five a day could ever need
        tokio::time::sleep(Duration::from_millis(fastrand::u64(
            60_000..=120_000,
        )))
        .await;

        Message::Enqueue {
            ident: self.ident,
            job: Job::Lure,
        }
    }
}

/// Looks in every minute or so whether the drawn moment for the next
/// fortress attack has come. The moment itself is drawn in `message.rs`
/// (`stealth::fortress_gap_secs`), so the safety slider and the "about
/// every x minutes" box take effect without restarting the checker
pub struct AutoFortressChecker {
    pub ident: AccountIdent,
}

/// Fires the periodic fortress housekeeping (collect / gems / buildings)
pub struct FortressMaintainChecker {
    pub ident: AccountIdent,
    pub player_status: Arc<Mutex<AccountStatus>>,
    /// Visits a day the player asked for (3-10)
    pub per_day: u8,
}

impl FortressMaintainChecker {
    /// A few visits a day instead of one every three minutes (5.4): no
    /// player sits at the fortress all day clicking it. The gap is the
    /// day split into `per_day` parts, give or take a fifth - but it is cut
    /// short for what will not wait: a store about to overflow, a building
    /// or a gem search about to finish
    pub async fn check(&self) -> Message {
        let per_day = u64::from(self.per_day.clamp(3, 10));
        let base = 24 * 3600 / per_day;
        let mut secs = fastrand::u64(base * 4 / 5..=base * 6 / 5);
        if let Some(due) = self.next_event_secs() {
            secs = secs.min(due.max(120));
        }
        tokio::time::sleep(Duration::from_secs(secs)).await;
        Message::Enqueue {
            ident: self.ident,
            job: Job::FortressCollect,
        }
    }

    /// Seconds until the first thing that will not wait: a store full, a
    /// building or a gem search done. A minute of slack on each
    fn next_event_secs(&self) -> Option<u64> {
        let lock = self.player_status.lock_safe();
        let (AccountStatus::Idle(_, gs) | AccountStatus::Busy(gs, _)) = &*lock
        else {
            return None;
        };
        let f = gs.fortress.as_ref()?;
        let now = Local::now();
        let until = |t: Option<DateTime<Local>>| {
            t.and_then(|t| (t - now).to_std().ok())
                .map(|d| d.as_secs() + 60)
        };
        let mut best: Option<u64> = None;
        let mut take = |v: Option<u64>| {
            if let Some(v) = v {
                best = Some(best.map_or(v, |b| b.min(v)));
            }
        };
        take(until(f.building_upgrade.finish));
        take(until(f.gem_search.finish));
        for (_, res) in &f.resources {
            let per_hour = res.production.per_hour;
            if per_hour == 0 || res.production.limit == 0 {
                continue;
            }
            let room = res.production.limit.saturating_sub(res.current);
            take(Some(room * 3600 / per_hour));
        }
        best
    }
}

/// Waits for the free dungeon fight
pub struct AutoDungeonChecker {
    pub player_status: Arc<Mutex<AccountStatus>>,
    pub ident: AccountIdent,
}

impl AutoDungeonChecker {
    pub async fn check(&self) -> Message {
        let next = match &*self.player_status.lock_safe() {
            AccountStatus::Idle(_, gs) => gs.dungeons.next_free_fight,
            _ => None,
        };
        match next.and_then(|n| (n - Local::now()).to_std().ok()) {
            // A cooldown is running: sleep it out, then a little jitter.
            // Never sleep for hours in one go - the timer might get shorter
            Some(remaining) => {
                tokio::time::sleep(remaining.min(Duration::from_secs(600)))
                    .await;
                // Right behind the timer, not a minute later - the guild
                // leader watched the free fight sit unused
                crate::pace::sleep_checker(5, 20).await;
            }
            // Nothing to wait for: the free fight is available right now, or
            // the game gave no timer at all. The tick takes it if anything is
            // worth fighting. But when nothing is (no dungeon clears the
            // win-chance bar, the bag is full, the 5 min guard...) there is no
            // point asking the server again 20 s later - the dungeon list
            // does not change on that timescale. Without this hold the
            // "Dungeon list refreshed" line fired every 20-60 s all day long.
            // A flat 5 min, not pace-scaled: this limits no-op requests, it is
            // not a responsiveness knob.
            // Two minutes, not five: the "refreshed" line is silent now,
            // so the only cost of looking is one request, and a free fight
            // that became possible (a pet unlocked, the bag cleared) used
            // to wait up to five minutes for it
            None => {
                let jitter = fastrand::u64(0..=30);
                tokio::time::sleep(Duration::from_secs(120 + jitter)).await;
            }
        }
        Message::Enqueue {
            ident: self.ident,
            job: Job::Dungeon,
        }
    }
}

/// Plays the dice game every so often
pub struct DiceChecker {
    pub ident: AccountIdent,
}

impl DiceChecker {
    pub async fn check(&self) -> Message {
        crate::pace::sleep_checker(1500, 2100).await;
        Message::Enqueue {
            ident: self.ident,
            job: Job::Dice,
        }
    }
}

/// Feeds the pets every so often
pub struct PetFeedChecker {
    pub ident: AccountIdent,
}

impl PetFeedChecker {
    pub async fn check(&self) -> Message {
        crate::pace::sleep_checker(1500, 2100).await;
        Message::Enqueue {
            ident: self.ident,
            job: Job::Pets,
        }
    }
}

/// Drives the expedition state machine. Sleeps until the current wait is
/// over (when nothing may be skipped), otherwise polls every minute
pub struct ExpeditionChecker {
    pub player_status: Arc<Mutex<AccountStatus>>,
    pub ident: AccountIdent,
}

impl ExpeditionChecker {
    pub async fn check(&self) -> Message {
        // A shift at the city guard blocks the tavern for hours. Asking every
        // minute during it did nothing but take a turn in the queue and put
        // "running Tavern" in the log - so it sleeps until the shift is over
        let on_duty = match &*self.player_status.lock_safe() {
            AccountStatus::Idle(_, gs) => match gs.tavern.current_action {
                sf_api::gamestate::tavern::CurrentAction::CityGuard {
                    busy_until,
                    ..
                } => Some(busy_until),
                _ => None,
            },
            _ => None,
        };
        if let Some(until) = on_duty {
            let left = (until - Local::now()).to_std().unwrap_or_default();
            // Wake a little after the shift ends, but look again at least
            // every ten minutes in case it is cut short by hand
            sleep(left.min(Duration::from_secs(600)) + Duration::from_secs(5))
                .await;
            return Message::Enqueue {
                ident: self.ident,
                job: Job::Expedition,
            };
        }
        let wait_until = match &*self.player_status.lock_safe() {
            AccountStatus::Idle(_, gs) => {
                gs.tavern.expeditions.active().and_then(|e| {
                    match e.current_stage() {
                    sf_api::gamestate::tavern::ExpeditionStage::Waiting {
                        busy_until,
                        ..
                    } => Some(busy_until),
                    _ => None,
                }
                })
            }
            _ => None,
        };
        let mut sleep_for =
            Duration::from_secs(crate::pace::checker_secs(45, 75));
        if let Some(until) = wait_until
            && let Ok(remaining) = (until - Local::now()).to_std()
        {
            sleep_for = sleep_for.min(remaining + Duration::from_secs(5));
        }
        tokio::time::sleep(sleep_for).await;
        Message::Enqueue {
            ident: self.ident,
            job: Job::Expedition,
        }
    }
}

/// Wakes up when the guard shift is over, or every minute or so otherwise
pub struct GuardChecker {
    pub player_status: Arc<Mutex<AccountStatus>>,
    pub ident: AccountIdent,
}

impl GuardChecker {
    pub async fn check(&self) -> Message {
        use sf_api::gamestate::tavern::CurrentAction;
        let until = match &*self.player_status.lock_safe() {
            AccountStatus::Idle(_, gs) => match gs.tavern.current_action {
                CurrentAction::CityGuard { busy_until, .. }
                | CurrentAction::Quest { busy_until, .. } => Some(busy_until),
                _ => None,
            },
            _ => None,
        };
        let mut sleep_for =
            Duration::from_secs(crate::pace::checker_secs(60, 90));
        if let Some(until) = until
            && let Ok(remaining) = (until - Local::now()).to_std()
        {
            sleep_for = sleep_for.max(remaining + Duration::from_secs(10));
        }
        tokio::time::sleep(sleep_for.min(Duration::from_secs(1800))).await;
        Message::Enqueue {
            ident: self.ident,
            job: Job::Guard,
        }
    }
}

/// Looks at the shops every 20-30 minutes (they restock at midnight)
pub struct ShopChecker {
    pub ident: AccountIdent,
}

impl ShopChecker {
    pub async fn check(&self) -> Message {
        crate::pace::sleep_checker(1200, 1800).await;
        Message::Enqueue {
            ident: self.ident,
            job: Job::Shopping,
        }
    }
}

/// Spends spare gold on attributes every 10-15 minutes
pub struct AttrChecker {
    pub ident: AccountIdent,
}

impl AttrChecker {
    pub async fn check(&self) -> Message {
        crate::pace::sleep_checker(600, 900).await;
        Message::Enqueue {
            ident: self.ident,
            job: Job::Attributes,
        }
    }
}

/// Wakes up for the free spin, otherwise every half hour
pub struct WheelChecker {
    pub player_status: Arc<Mutex<AccountStatus>>,
    pub ident: AccountIdent,
}

impl WheelChecker {
    pub async fn check(&self) -> Message {
        let next = match &*self.player_status.lock_safe() {
            AccountStatus::Idle(_, gs) => gs.specials.wheel.next_free_spin,
            _ => None,
        };
        let mut sleep_for =
            Duration::from_secs(crate::pace::checker_secs(1500, 2100));
        if let Some(next) = next
            && let Ok(remaining) = (next - Local::now()).to_std()
        {
            sleep_for = sleep_for.min(remaining + Duration::from_secs(15));
        }
        tokio::time::sleep(sleep_for).await;
        Message::Enqueue {
            ident: self.ident,
            job: Job::Wheel,
        }
    }
}

/// Looks for free daily rewards - the login calendar unlocks once a day, the
/// gleeman's chests as soon as enough task points are earned
pub struct DailiesChecker {
    pub ident: AccountIdent,
}

impl DailiesChecker {
    pub async fn check(&self) -> Message {
        crate::pace::sleep_checker(600, 900).await;
        Message::Enqueue {
            ident: self.ident,
            job: Job::Dailies,
        }
    }
}

/// Checks every few minutes whether the daily guild report is due
#[allow(dead_code)]
pub struct GuildReportChecker {
    pub ident: AccountIdent,
}

#[allow(dead_code)]
impl GuildReportChecker {
    pub async fn check(&self) -> Message {
        crate::pace::sleep_checker(240, 360).await;
        Message::GuildReportTick { ident: self.ident }
    }
}

/// Buys Arena Manager upgrades every so often
pub struct IdleChecker {
    pub ident: AccountIdent,
}

impl IdleChecker {
    pub async fn check(&self) -> Message {
        crate::pace::sleep_checker(240, 360).await;
        Message::Enqueue {
            ident: self.ident,
            job: Job::ArenaManager,
        }
    }
}

/// Collects the underworld's gold and souls and keeps the buildings going up,
/// every so often. Without this the underworld job ran once when the bot was
/// switched on and then never again, so the farmed souls just sat there
pub struct UnderworldChecker {
    pub ident: AccountIdent,
}

impl UnderworldChecker {
    pub async fn check(&self) -> Message {
        crate::pace::sleep_checker(240, 360).await;
        Message::Enqueue {
            ident: self.ident,
            job: Job::Underworld,
        }
    }
}

/// Re-reads the guild every so often to sign up for battles
pub struct GuildAutoChecker {
    pub ident: AccountIdent,
}

impl GuildAutoChecker {
    pub async fn check(&self) -> Message {
        crate::pace::sleep_checker(540, 660).await;
        Message::Enqueue {
            ident: self.ident,
            job: Job::GuildSign,
        }
    }
}

impl AutoFortressChecker {
    pub async fn check(&self) -> Message {
        crate::pace::sleep_checker(40, 80).await;
        Message::Enqueue {
            ident: self.ident,
            job: Job::FortressAttack,
        }
    }
}

pub fn total_attributes(gs: &GameState) -> u32 {
    let base = gs.character.attribute_basis.as_array();
    let bonus = gs.character.attribute_additions.as_array();
    base.iter().chain(bonus).sum::<u32>()
}

#[cfg(test)]
mod score_tests {
    use super::*;
    use sf_api::gamestate::GameState;

    fn gs() -> GameState {
        GameState::default()
    }

    /// A free fight whose cooldown has run out goes before one that still
    /// has most of an hour to wait, and before a job with nothing on a
    /// clock at all
    #[test]
    fn work_that_is_due_goes_before_work_that_keeps() {
        Job::set_order(&[]);
        let mut g = gs();
        let now = Local::now();
        g.arena.next_free_fight = Some(now - chrono::Duration::minutes(9));
        g.dungeons.next_free_fight = Some(now + chrono::Duration::minutes(50));
        let arena = Job::Arena.score(&g, 0);
        let dungeon = Job::Dungeon.score(&g, 0);
        let shopping = Job::Shopping.score(&g, 0);
        assert!(arena > dungeon, "{arena} vs {dungeon}");
        assert!(arena > shopping, "{arena} vs {shopping}");
        // ...and the dungeon, an hour out, is worth less than the shop
        assert!(dungeon < shopping, "{dungeon} vs {shopping}");
    }

    /// A thirst near its ceiling is the day running out; an empty one is
    /// nothing to hurry for
    #[test]
    fn a_full_thirst_outranks_an_empty_one() {
        Job::set_order(&[]);
        let mut g = gs();
        g.tavern.thirst_for_adventure_sec = 6000;
        let full = Job::Expedition.score(&g, 0);
        g.tavern.thirst_for_adventure_sec = 0;
        let empty = Job::Expedition.score(&g, 0);
        assert!(full > empty * 3.0, "{full} vs {empty}");
    }

    /// Nothing starves: ten minutes of waiting is worth as much as being
    /// first in the order
    #[test]
    fn waiting_counts() {
        Job::set_order(&[]);
        let g = gs();
        let fresh = Job::Shopping.score(&g, 0);
        let waited = Job::Shopping.score(&g, 600);
        assert!((waited - fresh * 2.0).abs() < 1e-9, "{waited} vs {fresh}");
        // And it stops there - an hour is not six times a job's worth
        assert!((Job::Shopping.score(&g, 3600) - waited).abs() < 1e-9);
    }

    /// With the game state unreadable the app falls back on the
    /// configured order, so the old behaviour is still there underneath
    #[test]
    fn the_configured_order_still_decides_between_equals() {
        Job::set_order(&[]);
        let g = gs();
        // Two jobs with nothing on a clock: the one the player put first
        assert!(Job::Shopping.priority() < Job::Guard.priority());
        assert!(Job::Shopping.score(&g, 0) > Job::Guard.score(&g, 0));
    }
}

#[cfg(test)]
mod sacrifice_tests {
    use super::IdleInfo;

    #[test]
    fn reconnects_preserve_the_exact_threshold() {
        let setting = 100;
        let original = IdleInfo { sacrifice_gain_pct: setting, ..Default::default() };
        for _ in 0..50 {
            let reconnected = IdleInfo { sacrifice_gain_pct: setting, ..Default::default() };
            assert_eq!(original.sacrifice_target(), reconnected.sacrifice_target());
            assert_eq!(reconnected.sacrifice_target().pct, 100.0);
        }
    }

    #[test]
    fn setting_changes_apply_immediately_without_a_cached_draw() {
        let mut ii = IdleInfo { sacrifice_gain_pct: 100, ..Default::default() };
        assert_eq!(ii.sacrifice_target().pct, 100.0);
        ii.sacrifice_gain_pct = 400;
        assert_eq!(ii.sacrifice_target().pct, 400.0);
        ii.sacrifice_gain_pct = 25;
        assert_eq!(ii.sacrifice_target().pct, 25.0);
        ii.sacrifice_gain_pct = 10;
        assert_eq!(ii.sacrifice_target().pct, 25.0);
    }

}

impl ExpeditionInfo {
    pub fn beer_affordable(&self, available_mushrooms: u32, free_beer: bool) -> bool {
        if self.beer_unaffordable_at.is_some_and(|(funds, vip)| available_mushrooms <= funds && free_beer == vip) {
            return false;
        }
        available_mushrooms > 0 || free_beer
    }
}

#[cfg(test)]
mod beer_affordability_tests {
    use super::ExpeditionInfo;

    #[test]
    fn refused_vip_beer_stays_quiet_until_funds_or_entitlement_change() {
        let mut info = ExpeditionInfo::default();
        assert!(info.beer_affordable(0, true));
        info.beer_unaffordable_at = Some((0, true));
        for _ in 0..48 { assert!(!info.beer_affordable(0, true)); }
        assert!(info.beer_affordable(1, true));
        assert!(!info.beer_affordable(0, false));
    }

    #[test]
    fn paid_beer_waits_for_more_spendable_funds_and_new_vip_can_resume() {
        let info = ExpeditionInfo { beer_unaffordable_at: Some((2, false)), ..Default::default() };
        assert!(!info.beer_affordable(1, false));
        assert!(!info.beer_affordable(2, false));
        assert!(info.beer_affordable(3, false));
        assert!(info.beer_affordable(0, true));
    }
}
