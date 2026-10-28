use std::{fmt::Write, sync::Arc, time::Duration};

use chrono::{DateTime, Local, Timelike};
use config::{CharacterConfig, SFAccCharacter, SFCharIdent};
use crawler::CrawlerError;
use iced::Command;
use log::{error, trace, warn};
use sf_api::{
    gamestate::GameState,
    session::{PWHash, Response, Session},
    sso::SSOProvider,
};
use tokio::time::sleep;
use ui::OverviewAction;

use self::{
    backup::{RestoreData, get_newest_backup, restore_backup},
    login::{SSOIdent, SSOLogin, SSOLoginStatus},
    ui::underworld::LureTarget,
};
use crate::{
    crawler::CrawlerState,
    player::{
        EnemyGuild, FortressActionKind, FortressInfo, Job, ScrapbookInfo,
        UnderworldInfo,
    },
    *,
};

pub use sf_api::gamestate::dungeons::Dungeon as SFDungeon;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpeditionOption {
    Auto,
    /// true = gold, false = XP
    Gold,
    SkipGlass,
}

/// The typed fields of the pet simulator's arena opponent
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PetSimField {
    Level,
    Pack,
    At100,
    At150,
    At200,
    Gladiator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TavernOption {
    Beer,
    Wheel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShopOption {
    ProtectEpics,
    ProtectEnchanted,
    ProtectUpgraded,
    SellOld,
    /// Sell junk out of the backpack when it runs out of room
    ClearBackpack,
    Mushrooms,
    AttrAuto,
    /// Raise the worn gear at the blacksmith
    Smith,
    /// Enchant the worn gear at the witch
    Witch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShopNumber {
    MinGain,
    GoldReserve,
    /// Backpack slots to keep empty
    KeepFree,
    /// Shop rerolls a day (a mushroom each)
    Rolls,
    WMain,
    WCon,
    WLuck,
    /// Percent a piece must beat a worn legendary by (0 = never)
    LegendarySwap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FortressOption {
    Collect,
    Gems,
    Build,
    RecruitAll,
    SmartSoldiers,
    CheapSkip,
}

/// What a `SimpleResult` was about, so that the right page can react to it
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimpleNote {
    /// A dungeon fight, with the dungeon name for the log
    Dungeon(String),
    /// The dungeon screen was (re)loaded from the server
    DungeonsUpdated,
    /// A tower fight. `attempt` counts how the floor number was derived,
    /// because the server is picky about it
    Tower {
        attempt: u8,
    },
    /// One fortress housekeeping step, with a description for the log
    FortressMaint(String),
    /// One underworld housekeeping step (collect, start or finish a build)
    UnderworldMaint(String),
    /// The fortress opponent was looked up before an attack
    FortressScout {
        auto: bool,
    },
    /// We signed up for a guild battle
    GuildSigned(String),
    GuildRaid,
    /// The auto-kick removed this member
    GuildKick(String),
    /// An invitation went out to this player
    GuildInvite(String),
    /// The guild finder list was read (`GroupJoinList`)
    GuildFinder,
    /// One finder guild was looked at for its bonuses
    GuildFinderLook(String),
    /// The guild page was opened right before the join request, the way
    /// the game client does it (ENTER opens the guild first)
    GuildJoinLook(String),
    /// We asked this guild to let us in (`GroupJoin`). `by_id`: the second
    /// try, with the guild id instead of the name
    GuildJoin {
        name: String,
        by_id: bool,
    },
    /// One enchantment at the witch. `model` is the `full_model_id` of
    /// the piece, so that a refusal can be pinned to that piece and not
    /// asked again - see `ShopInfo::witch_refused`
    WitchEnchant {
        what: String,
        model: u32,
    },
    /// The daily demonic portal fight
    DemonPortal,
    /// A pet got fed. `attempt` is the fruit-index convention that was used
    PetFed {
        pet: String,
        attempt: u8,
    },
    /// A pet's screen was opened, which the game wants before it will take
    /// a feed for it
    PetOpened(String),
    /// The strongest pet of a habitat was opened (selected) right before
    /// its dungeon fight - the game fights with the pet that is clicked,
    /// the id in the fight request alone did not pick it
    PetDungeonOpened {
        pet_id: u32,
        name: String,
    },
    /// An Arena Manager building got upgraded
    IdleBought(String),
    /// Money of an Arena Manager building was collected
    IdleCollected(String),
    /// One expedition step (start, crossroads, boss, reward, skip, poll)
    Expedition(String),
    Guild,
    GuildChat,
    Mail,
    /// City guard started / finished
    Guard(String),
    /// A free daily reward was collected (calendar or gleeman chest)
    Dailies(String),
    /// Refreshing the game's own arena offer: `step` 0 is CheckArena, then
    /// one look at each of the three opponents the game named
    /// `then_fight`: the arena job asked because it had nobody to attack, so
    /// once the offer is in it goes again. False for the button and for the
    /// refresh after a fight - those only keep the panel current
    ArenaOffer {
        step: u8,
        then_fight: bool,
    },
    /// A pet habitat dungeon fight. `attempt` 0 sends the global pet id,
    /// 1 the id inside the habitat (the wire format is not documented)
    PetDungeon {
        name: String,
        habitat: sf_api::gamestate::unlockables::HabitatType,
        enemy_pos: u32,
        pet_id: u32,
        attempt: u8,
    },
    /// Junk sold out of the backpack to make room for dungeon loot
    BackpackCleared(String),
    /// The guild pet: a hydra fight or a level (5.4)
    GuildPet(String),
    /// A guild step asked for by hand: the target list, an attack (6.1)
    GuildAction(String),
    /// A retired legendary let go by the player's own F (6.0): the gem
    /// out first (`gem_out`), then the piece taken apart
    Sacrifice {
        pos: usize,
        gem_out: bool,
        what: String,
    },
    /// A piece taken apart at the blacksmith to make room - the emergency
    /// tier of `backpack::next_sale`. Its own note so a refusal (the day's
    /// dismantles used up) can switch dismantling off for a while
    BackpackDismantled(String),
    /// One pet arena fight against the daily opponent
    PetArena {
        name: String,
    },
    /// One step of the dice game. `stage` 0 opened the game, 1 re-rolled,
    /// 2 took what was on the table
    Dice {
        what: String,
        stage: u8,
    },
    /// A beer went down
    Beer,
    /// A shop purchase / equip / sale step
    Shop(String),
    Recycle { action: crate::shopping::RecycleAction },
    /// One attribute bought (silver before the purchase)
    Attr(String, u64),
    /// A spin of the wheel
    Wheel(String),
    /// `ViewGuild` result. `id` is the planned battle's guild id, None for
    /// the manual "compare with" box
    GuildLookup {
        id: Option<u32>,
    },
    /// A private message was sent
    MailSent(String),
    /// One copy of the guild report went out
    ReportSent(String),
    /// One inbox message was opened. `by_id` says whether the server was
    /// asked with the message id (first try) or the list position (fallback)
    MailOpen {
        pos: i32,
        by_id: bool,
    },
}

impl SimpleNote {
    /// What to put in the live console. The console used to print `{:?}` of
    /// this enum, cut off at seventy characters - so it read
    /// `PetFed { pet: "Water pet #81 (lvl 16, 2/3 today, 2 fruit lef` and
    /// told a player nothing at all
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            SimpleNote::Dungeon(what) => format!("Dungeon: {what}"),
            SimpleNote::DungeonsUpdated => "Dungeon list refreshed".into(),
            SimpleNote::Tower { .. } => "Tower fight".into(),
            SimpleNote::FortressMaint(what) => format!("Fortress: {what}"),
            SimpleNote::UnderworldMaint(what) => {
                format!("Underworld: {what}")
            }
            SimpleNote::FortressScout { .. } => {
                "Fortress: looking at the opponent".into()
            }
            SimpleNote::GuildSigned(what) => format!("Guild: joined {what}"),
            SimpleNote::GuildRaid => "Guild raid".into(),
            SimpleNote::GuildKick(name) => {
                format!("Kicked {name} from the guild (grade 5)")
            }
            SimpleNote::GuildInvite(name) => {
                format!("Invited {name} to the guild")
            }
            SimpleNote::GuildFinder => "Guild finder read".into(),
            SimpleNote::GuildFinderLook(name) => {
                format!("Guild finder: looked at {name}")
            }
            SimpleNote::GuildJoinLook(name) => {
                format!("Guild finder: opened {name} before asking")
            }
            SimpleNote::GuildJoin { name, .. } => {
                format!("Asked {name} to let us in")
            }
            SimpleNote::DemonPortal => "Demonic portal".into(),
            // "Feeding", not "Fed" - at this point the game has answered
            // and proven nothing. Whether it really happened is decided a
            // step later, by whether the pet data moved
            SimpleNote::PetFed { pet, .. } => format!("Feeding {pet}"),
            SimpleNote::PetOpened(pet) => format!("Opened {pet}"),
            SimpleNote::PetDungeonOpened { name, .. } => {
                format!("Pet dungeon: selected {name}")
            }
            SimpleNote::IdleBought(what) => format!("Arena Manager: {what}"),
            SimpleNote::IdleCollected(what) => {
                format!("Arena Manager: {what}")
            }
            SimpleNote::Expedition(what) => what.clone(),
            SimpleNote::Guild => "Guild refreshed".into(),
            SimpleNote::GuildChat => "Guild chat".into(),
            SimpleNote::Mail => "Mail refreshed".into(),
            SimpleNote::Guard(what) => format!("City guard: {what}"),
            SimpleNote::Dailies(what) => what.clone(),
            SimpleNote::PetDungeon { name, .. } => name.clone(),
            SimpleNote::ArenaOffer { step: 0, .. } => {
                "Arena: asked for the offer".into()
            }
            SimpleNote::ArenaOffer { step, .. } => {
                format!("Arena: looked at opponent {step}")
            }
            SimpleNote::BackpackCleared(what)
            | SimpleNote::BackpackDismantled(what) => what.clone(),
            SimpleNote::GuildPet(what) | SimpleNote::GuildAction(what) => {
                format!("Guild: {what}")
            }
            SimpleNote::Sacrifice { what, .. } => format!("Sacrifice: {what}"),
            SimpleNote::PetArena { name } => name.clone(),
            SimpleNote::Dice { what, .. } => format!("Dice: {what}"),
            SimpleNote::Beer => "Drank a beer".into(),
            SimpleNote::Shop(what) => format!("Shop: {what}"),
            SimpleNote::Recycle { action } => format!("Recycling: {action:?}"),
            SimpleNote::WitchEnchant { what, .. } => format!("Shop: {what}"),
            SimpleNote::Attr(what, _) => format!("Bought {what}"),
            SimpleNote::Wheel(what) => format!("Wheel: {what}"),
            // The battle odds lookup of the enemy guild - not the finder,
            // which only ever runs for a character without a guild
            SimpleNote::GuildLookup { .. } => {
                "Looked up the enemy guild for the battle odds".into()
            }
            SimpleNote::MailSent(to) => format!("Message sent to {to}"),
            SimpleNote::ReportSent(to) => format!("Guild report sent to {to}"),
            SimpleNote::MailOpen { .. } => "Opened a message".into(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    /// The member table in two columns or one (6.1)
    GuildTwoColumns(bool),
    /// The leadership tools for an ordinary member (6.1.2)
    GuildLeaderTools(bool),
    /// Ask the game for the guilds that can be attacked (6.1)
    GuildFindTargets {
        ident: AccountIdent,
    },
    /// Declare an attack on a suggested guild (leader / officer)
    GuildDeclareAttack {
        ident: AccountIdent,
        guild: String,
    },
    /// The Sacrifice window (6.0): "let it go" on a retired legendary
    SacrificeAsk {
        ident: AccountIdent,
        pos: usize,
    },
    /// ...changed one's mind
    SacrificeCancel {
        ident: AccountIdent,
    },
    /// ...pressed F
    SacrificeConfirm {
        ident: AccountIdent,
    },
    /// The gem is out - now the piece itself
    SacrificeNext {
        ident: AccountIdent,
        pos: usize,
    },
    MultiAction {
        action: OverviewAction,
    },
    FontLoaded(Result<(), iced::font::Error>),
    CrawlAllRes {
        servers: Option<Vec<String>>,
        concurrency: usize,
    },
    NextCLICrawling,
    AdvancedLevelRestrict(bool),
    ShowClasses(bool),
    CrawlerSetMinMax {
        server: ServerID,
        min: u32,
        max: u32,
    },
    UpdateResult(bool),
    PlayerSetMaxUndergroundLvl {
        ident: AccountIdent,
        lvl: u16,
    },
    SetOverviewSelected {
        ident: Vec<AccountIdent>,
        val: bool,
    },
    SSOLoginFailure {
        name: String,
        error: String,
    },
    PlayerRelogSuccess {
        ident: AccountIdent,
        gs: Box<GameState>,
        session: Box<Session>,
    },
    PlayerRelogDelay {
        ident: AccountIdent,
        session: Box<Session>,
    },
    CopyBattleOrder {
        ident: AccountIdent,
    },
    /// Changes how the targets of a single character are ranked
    PlayerSetOrder {
        ident: AccountIdent,
        order: TargetOrder,
    },
    /// Free text search inside the target list
    PlayerSetFilter {
        ident: AccountIdent,
        filter: String,
    },
    /// Switch between the plain target list and the calculated battle plan
    PlayerShowPlan {
        ident: AccountIdent,
        show: bool,
    },
    /// (Re-)calculates the greedy battle plan
    PlayerComputePlan {
        ident: AccountIdent,
    },
    /// Manually removes a target from the list, without fighting it
    PlayerSkipTarget {
        ident: AccountIdent,
        uid: u32,
        name: String,
    },
    PlayerClearBlacklist {
        ident: AccountIdent,
    },
    /// Default ranking for newly logged in characters
    SetTargetOrder(TargetOrder),
    SetLanguage(crate::i18n::Lang),
    AdminInput(String),
    /// Switch between the drawn lettering and the machine's own type
    ToggleDrawnFont,
    /// The explanations under the controls on or off (6.1.4)
    ToggleHelp,
    /// Somebody is typing or pasting a licence code
    LicenceInput(String),
    /// ...and has pressed the button
    LicenceApply,
    /// Copy this machine's code, so it can be sent to the author
    LicenceCopyMachine,
    AdminUnlock,
    AdminLock,
    AnnounceSetEnabled(bool),
    AnnounceSetText(String),
    AnnounceSetHours(u8),
    /// Post the announcement now (owner characters only)
    AnnounceNow,
    /// Post the Discord invite to the guild chat (owner characters only)
    AnnounceDiscord,
    AnnounceTick,
    /// The invite once a day at a random time, on / off
    DiscordDailySet(bool),
    GreetSetEnabled(bool),
    GreetSetText(String),
    FarewellSetEnabled(bool),
    FarewellSetText(String),
    /// Say something in the guild chat from the owner character `ident`,
    /// after the delay the scheduler chose - the hello to a newcomer, the
    /// goodbye to whoever left. `about` is for the console, `attempt` counts
    /// the retries while the character is busy
    SayInGuildChat {
        ident: AccountIdent,
        text: String,
        about: String,
        attempt: u8,
        /// Only say it while this member (lowercase) is still in the guild:
        /// a hello to somebody who joined and left inside the delay
        /// landed in the chat after they were gone
        only_if_member: Option<String>,
    },
    /// A timer says an automatic action is due. It waits in the queue
    Enqueue {
        ident: AccountIdent,
        job: Job,
    },
    /// How many losses in a row disable auto battle
    SetAutoBattleMaxLosses(usize),
    /// Turns the automatic fortress attacks on/off
    AutoFortress {
        ident: AccountIdent,
        state: bool,
    },
    /// Seconds between two automatic fortress attacks
    FortressSetInterval {
        ident: AccountIdent,
        secs: u64,
    },
    /// The most the raid waits after the barracks fill, in minutes
    FortressSetNoticeMax {
        ident: AccountIdent,
        minutes: u32,
    },
    /// The next step of a raid run: another attack, or the refill after
    /// the soldiers ran out. Arrives after a short human pause
    FortressRaidStep {
        ident: AccountIdent,
    },
    /// The timer for the next fortress attack has elapsed
    AutoFortressPossible {
        ident: AccountIdent,
    },
    /// Attack the current fortress opponent right now
    FortressAttack {
        ident: AccountIdent,
    },
    /// Get a different fortress opponent
    FortressReroll {
        ident: AccountIdent,
    },
    /// Queue one new soldier
    FortressTrain {
        ident: AccountIdent,
    },
    FortressActionResult {
        ident: AccountIdent,
        session: Box<Session>,
        resp: Box<Response>,
        action: FortressActionKind,
    },
    /// The server refused the action (missing resources, cooldown, ...).
    /// Unlike a connection error, this does not need a relog
    FortressActionFailed {
        ident: AccountIdent,
        session: Box<Session>,
        action: FortressActionKind,
        error: String,
    },
    /// Queue a new soldier after every attack
    FortressSetTrain {
        ident: AccountIdent,
        state: bool,
    },
    FortressSetOption {
        ident: AccountIdent,
        option: FortressOption,
        state: bool,
    },
    /// Periodic housekeeping: collect, gems, buildings (continuation)
    FortressMaintainTick {
        ident: AccountIdent,
    },
    /// The same, started by the queue / a click - resets the step counter
    FortressMaintainTimer {
        ident: AccountIdent,
    },
    GuardSetAuto {
        ident: AccountIdent,
        state: bool,
    },
    TavernSetOption {
        ident: AccountIdent,
        option: TavernOption,
        state: bool,
    },
    BeerSetMax {
        ident: AccountIdent,
        n: u8,
    },
    /// How many mushrooms the automation leaves untouched
    SetMushroomReserve {
        ident: AccountIdent,
        n: u32,
    },
    /// The bot's mushrooms for one server day
    SetMushroomBudget {
        ident: AccountIdent,
        n: u32,
    },
    /// VIP: the beer is free for this character
    SetBeerFree {
        ident: AccountIdent,
        state: bool,
    },
    WheelSetSpins {
        ident: AccountIdent,
        n: u8,
    },
    /// One spin if one is due
    /// Look for free daily rewards: login calendar, gleeman chests
    DailiesTimer {
        ident: AccountIdent,
    },
    /// Continuation of the daily collecting, one reward at a time
    DailiesTick {
        ident: AccountIdent,
    },
    /// Turns collecting those on or off
    DailiesSetAuto {
        ident: AccountIdent,
        state: bool,
    },
    /// Keep the gold-only horse mount rented, on or off
    MountSetAuto {
        ident: AccountIdent,
        state: bool,
    },
    /// Add or remove one gem colour from the keep list
    GemToggleWanted {
        ident: AccountIdent,
        gem: crate::config::GemWanted,
    },
    WheelTimer {
        ident: AccountIdent,
    },
    /// Continuation after a spin
    WheelTick {
        ident: AccountIdent,
    },
    /// Playing the dice game on or off
    DiceSetAuto {
        ident: AccountIdent,
        state: bool,
    },
    /// Skipping the wait with a quicksand glass on or off
    DiceSetSkip {
        ident: AccountIdent,
        state: bool,
    },
    DiceTimer {
        ident: AccountIdent,
    },
    /// Continuation after one roll
    DiceTick {
        ident: AccountIdent,
    },
    IdleSetSacrifice {
        ident: AccountIdent,
        state: bool,
    },
    IdleSetSacrificeStep {
        ident: AccountIdent,
        step: u32,
    },
    /// Stealth settings (see `stealth`)
    SetSafety(u8),
    /// "Do it now" - full speed for a few minutes, then back to the
    /// slider. Pressed again while it runs, it stops
    BoostToggle,
    /// How many officer seats the guild advice is built around
    SetOfficerSeats {
        ident: AccountIdent,
        n: u8,
    },
    SetQuietHours(bool),
    SetQuietFrom(u8),
    SetQuietTo(u8),
    SetRequestsPerMin(u8),
    /// The proxy of one character: `index` into the accounts of the
    /// config, `sub` the character of an SF account
    SetProxy {
        index: usize,
        sub: Option<usize>,
        url: String,
    },
    /// The founding date of the guild, as the leader types it. Only his
    /// own row and the guild's age are built from it - see
    /// `CharacterConfig::guild_founded`
    SetGuildFounded {
        ident: AccountIdent,
        text: String,
    },
    /// Open a burst session right now
    BurstNow,
    /// Pause / resume every queue without touching the settings
    TogglePause,
    /// The window follows the bot's jobs, or stays where the player put it
    ToggleFollow,
    /// The window changed size. The member table sizes its type from the
    /// width and stretches its rows to the height
    TavernTab(crate::ui::TavernTab),
    WindowResized {
        width: u32,
        height: u32,
    },
    SetGradeStrictness(u8),
    /// How many of the best (`top`) or the worst members the table marks
    GuildMark {
        top: bool,
        n: u8,
    },
    SaveSettings,
    ToggleConsole,
    ClearConsole,
    /// Empties the successes window
    ClearSuccesses,
    /// Put a job at an absolute place (0-based) in the queue order
    MoveJobTo {
        job: Job,
        pos: usize,
    },
    /// Back to the built-in order
    ResetJobOrder,
    ShopSetOption {
        ident: AccountIdent,
        option: ShopOption,
        state: bool,
    },
    ShopSetNumber {
        ident: AccountIdent,
        what: ShopNumber,
        value: u32,
    },
    /// Look at both shops and buy the best upgrade
    ShopTimer {
        ident: AccountIdent,
    },
    /// Continuation of a purchase (equip / sell)
    ShopTick {
        ident: AccountIdent,
    },
    /// Buy attributes with spare gold
    AttrTimer {
        ident: AccountIdent,
    },
    AttrTick {
        ident: AccountIdent,
    },
    /// Switch every farming automation of a character on or off at once
    FullBot {
        ident: AccountIdent,
        on: bool,
    },
    GuardSetHours {
        ident: AccountIdent,
        hours: u8,
    },
    GuardSetCutoffEnabled {
        ident: AccountIdent,
        state: bool,
    },
    GuardSetCutoff {
        ident: AccountIdent,
        text: String,
    },
    DungeonSetPetsFirst {
        ident: AccountIdent,
        state: bool,
    },
    /// One look at the tavern: finish a shift, start the next
    GuardTimer {
        ident: AccountIdent,
    },
    /// Stop the running shift (loses the wage of the remaining hours)
    GuardCancel {
        ident: AccountIdent,
    },
    /// Looks up the other side of the planned attack/defense
    GuildLookupEnemies {
        ident: AccountIdent,
    },
    GuildCompareInput {
        ident: AccountIdent,
        text: String,
    },
    GuildCompare {
        ident: AccountIdent,
    },
    /// "No, keep it on" in the switch-off confirmation
    GuildAutoCancelOff {
        ident: AccountIdent,
    },
    GuildSetAuto {
        ident: AccountIdent,
        defense: bool,
        state: bool,
    },
    /// Start raids on their own whenever none is running
    GuildSetRaidAuto {
        ident: AccountIdent,
        state: bool,
    },
    /// Auto-kick of grade 5 members on / off (owner, behind the password)
    AutoKickSet(bool),
    /// Kick one member - the auto-kick found them
    GuildKick {
        ident: AccountIdent,
        name: String,
    },
    /// Invite a guildless player from the recruiting list
    GuildInvite {
        ident: AccountIdent,
        name: String,
    },
    /// Read the game's guild finder (a character without a guild)
    GuildFinderRefresh {
        ident: AccountIdent,
    },
    /// The next lookup or the join itself, once the list is in
    GuildFinderStep {
        ident: AccountIdent,
    },
    /// The finder's ENTER button: ask this guild to let us in
    GuildFinderJoin {
        ident: AccountIdent,
        name: String,
    },
    GuildSetFinderAuto {
        ident: AccountIdent,
        state: bool,
    },
    GuildAutoTick {
        ident: AccountIdent,
    },
    GuildStartRaid {
        ident: AccountIdent,
    },
    FortressSetAttackSoldiers {
        ident: AccountIdent,
        count: u16,
    },
    FortressSetSoldierMargin {
        ident: AccountIdent,
        margin: u16,
    },
    DungeonSetAuto {
        ident: AccountIdent,
        state: bool,
    },
    DungeonSetMinChance {
        ident: AccountIdent,
        percent: u8,
    },
    /// Toggle spending mushrooms on extra dungeon fights
    DungeonSetMushroom {
        ident: AccountIdent,
        state: bool,
    },
    /// Lowest win chance worth a mushroom
    DungeonSetMushroomMin {
        ident: AccountIdent,
        percent: u8,
    },
    /// Toggle the daily demonic portal fight
    DungeonSetPortal {
        ident: AccountIdent,
        state: bool,
    },
    /// Toggle spending mushrooms on extra arena fights
    ArenaSetMushroom {
        ident: AccountIdent,
        state: bool,
    },
    /// The free dungeon fight should be available now
    AutoDungeonTick {
        ident: AccountIdent,
    },
    PetSetAutoArena {
        ident: AccountIdent,
        state: bool,
    },
    /// The simulator's arena opponent: which habitat is fought
    PetSimHabitat {
        ident: AccountIdent,
        habitat: sf_api::gamestate::unlockables::HabitatType,
    },
    /// The simulator's arena opponent: which pet of the habitat, 0-based
    PetSimTheirPet {
        ident: AccountIdent,
        index: usize,
    },
    /// The simulator's arena opponent: one of the typed fields
    PetSimField {
        ident: AccountIdent,
        field: PetSimField,
        text: String,
    },
    PetSetAutoFeed {
        ident: AccountIdent,
        state: bool,
    },
    /// Feed the next pet that can still eat
    PetFeedTick {
        ident: AccountIdent,
        attempt: u8,
    },
    /// The periodic feeding timer fired - starts a fresh round
    PetFeedTimer {
        ident: AccountIdent,
    },
    IdleSetAuto {
        ident: AccountIdent,
        state: bool,
    },
    ExpeditionSetOption {
        ident: AccountIdent,
        option: ExpeditionOption,
        state: bool,
    },
    /// Next step of the running expedition
    ExpeditionTick {
        ident: AccountIdent,
    },
    /// The periodic timer - starts a fresh round of steps
    ExpeditionTimer {
        ident: AccountIdent,
    },
    /// Buy the next Arena Manager upgrade
    IdleTick {
        ident: AccountIdent,
    },
    /// The periodic timer - starts a fresh buying round
    IdleTimer {
        ident: AccountIdent,
    },
    // ------------------------------------------------------------ dungeons
    /// Re-reads the dungeon screen from the server, then simulates
    /// `force`: ask the server even when the list was read moments ago
    DungeonRefresh {
        ident: AccountIdent,
        force: bool,
    },
    /// Simulates every dungeon with the game state we already have
    DungeonCompute {
        ident: AccountIdent,
    },
    DungeonOnlyOpen {
        ident: AccountIdent,
        only_open: bool,
    },
    /// Enters the given dungeon
    DungeonFight {
        ident: AccountIdent,
        dungeon: SFDungeon,
        name: String,
        /// Only used for the tower: which floor-number guess to send
        attempt: u8,
        /// Spend a mushroom to fight even though the free fight is used up
        mushroom: bool,
    },
    // -------------------------------------------------------- guild + mail
    /// Re-reads the guild (and with it the chat)
    GuildRefresh {
        ident: AccountIdent,
    },
    GuildChatInput {
        ident: AccountIdent,
        text: String,
    },
    /// Sends whatever the user typed into the guild chat
    GuildChatSend {
        ident: AccountIdent,
    },
    GuildMemberDetail { ident: AccountIdent, name: Option<String> },
    GuildShowMembers {
        ident: AccountIdent,
        show: bool,
    },
    /// Opens / closes the drawer with the guild summary above the table
    GuildDrawer {
        ident: AccountIdent,
    },
    GuildMemberSort {
        ident: AccountIdent,
        sort: crate::player::MemberSort,
    },
    GuildCompact {
        ident: AccountIdent,
        state: bool,
    },
    /// Puts a piece of the guild page on the clipboard - a chat line,
    /// the whole chat, or the odds for the battle that is planned
    GuildChatCopy {
        ident: AccountIdent,
        text: String,
    },
    /// Puts the rating text on the clipboard
    GuildReportCopy {
        ident: AccountIdent,
        text: String,
    },
    /// Build the report and start mailing it to every member
    GuildReportSend {
        ident: AccountIdent,
    },
    /// Mail the next member in the queue
    GuildReportNext {
        ident: AccountIdent,
    },
    /// Periodic check: is it time for today's report?
    GuildReportTick {
        ident: AccountIdent,
    },
    /// Pulls in new mail/messages
    MailRefresh {
        ident: AccountIdent,
    },
    MailReplyToInput {
        ident: AccountIdent,
        text: String,
    },
    MailReplyInput {
        ident: AccountIdent,
        text: String,
    },
    /// Sends the typed reply to the typed recipient
    MailSend {
        ident: AccountIdent,
    },
    /// Opens one message of the inbox
    MailOpen {
        ident: AccountIdent,
        pos: i32,
        /// Ask by message id (what the official client does). When the
        /// server refuses, the handler retries with the list position
        by_id: bool,
    },
    /// A plain command, that only needs the game state updated afterwards
    SimpleResult {
        ident: AccountIdent,
        session: Box<Session>,
        resp: Box<Response>,
        note: SimpleNote,
    },
    SimpleFailed {
        ident: AccountIdent,
        session: Box<Session>,
        note: SimpleNote,
        error: String,
    },
    ConfigSetAutoFortress {
        name: String,
        server: ServerID,
        nv: bool,
    },
    BackupRes {
        server: ServerID,
        error: Option<String>,
    },
    SaveHoF(ServerID),
    PlayerSetMaxLvl {
        ident: AccountIdent,
        max: u16,
    },
    PlayerSetMaxAttributes {
        ident: AccountIdent,
        max: u32,
    },
    /// Successes / live feed: this character only <-> everybody
    ToggleFeedAll,
    /// Fetch the three opponents the game offers in the arena. `then_fight`
    /// only when the arena job asked - see `SimpleNote::ArenaOffer`
    ArenaOfferRefresh {
        ident: AccountIdent,
        then_fight: bool,
    },
    /// Fight one of the offered opponents by hand
    ArenaFightOffer {
        ident: AccountIdent,
        pid: u32,
    },
    PlayerAttack {
        ident: AccountIdent,
        target: AttackTarget,
    },
    PlayerLure {
        ident: AccountIdent,
        target: LureTarget,
    },
    OpenLink(String),
    /// Recovery page for an unreadable `helper.toml`: read it again (the
    /// user fixed it by hand), open the folder, or set the broken file
    /// aside and start with no saved accounts
    ConfigRetry,
    ConfigOpenFolder,
    ConfigStartFresh,
    SSOSuccess {
        auth_name: String,
        chars: Vec<Session>,
        provider: SSOProvider,
    },
    SSORetry,
    SSOAuthError {
        _error: String,
    },
    SetMaxThreads(usize),
    SetStartThreads(usize),
    SetBlacklistThr(usize),
    SetAutoFetch(bool),
    ViewSubPage {
        player: AccountIdent,
        page: AccountPage,
    },
    SSOImport {
        pos: usize,
    },
    SSOImportAuto {
        ident: SFCharIdent,
    },
    SSOLoginSuccess {
        name: String,
        pass: PWHash,
        chars: Vec<Session>,
        remember: bool,
        auto_login: bool,
    },
    ViewSettings,
    /// Which page of the settings to show
    ShowSettingsTab(crate::SettingsTab),
    ChangeTheme(AvailableTheme),
    ViewOverview,
    CrawlerRevived {
        server_id: ServerID,
    },
    CrawlerStartup {
        server: ServerID,
        state: Arc<CrawlerState>,
    },
    AutoBattle {
        ident: AccountIdent,
        state: bool,
    },
    AutoLure {
        ident: AccountIdent,
        state: bool,
    },
    PlayerCommandFailed {
        ident: AccountIdent,
        session: Box<Session>,
        attempt: u64,
    },
    PlayerAttackResult {
        ident: AccountIdent,
        session: Box<Session>,
        against: AttackTarget,
        resp: Box<Response>,
    },
    PlayerLureResult {
        ident: AccountIdent,
        session: Box<Session>,
        against: LureTarget,
        resp: Box<Response>,
    },
    AutoBattlePossible {
        ident: AccountIdent,
    },
    OrderChange {
        server: ServerID,
        new: CrawlingOrder,
    },
    Login {
        account: AccountConfig,
        auto_login: bool,
    },
    RememberMe(bool),
    ClearHof(ServerID),
    CrawlerSetThreads {
        server: ServerID,
        new_count: usize,
    },
    PageCrawled,
    RemoveAccount {
        ident: AccountIdent,
    },
    CharacterCrawled {
        server: ServerID,
        que_id: QueID,
        character: CharacterInfo,
    },
    CrawlerDied {
        server: ServerID,
        error: String,
    },
    ShowPlayer {
        ident: AccountIdent,
    },
    CrawlerIdle(ServerID),
    CrawlerNoPlayerResult,
    CrawlerUnable {
        server: ServerID,
        action: CrawlAction,
        error: CrawlerError,
    },
    ViewLogin,
    LoginNameInputChange(String),
    LoginPWInputChange(String),
    LoginServerChange(String),
    LoginSFSubmit,
    LoginRegularSubmit,
    LoginViewChanged(LoginType),
    LoggininSuccess {
        ident: AccountIdent,
        gs: Box<GameState>,
        session: Box<Session>,
        remember: bool,
    },
    LoggininFailure {
        ident: AccountIdent,
        error: String,
    },
    ResetCrawling {
        server: ServerID,
        status: Box<RestoreData>,
    },
    ConfigSetAutoLogin {
        name: String,
        server: ServerID,
        nv: bool,
    },
    ConfigSetAutoBattle {
        name: String,
        server: ServerID,
        nv: bool,
    },
    ConfigSetAutoLure {
        name: String,
        server: ServerID,
        nv: bool,
    },
    UIActive,
    AutoLureIdle,
    AutoLurePossible {
        ident: AccountIdent,
    },
    /// Collect gold and souls in the underworld on its own
    UnderworldAutoCollect {
        ident: AccountIdent,
        state: bool,
    },
    /// Keep the underworld buildings going up on their own
    UnderworldAutoBuild {
        ident: AccountIdent,
        state: bool,
    },
    /// Souls first, or the character's bonuses - see `mechanics`
    UnderworldSetPhase {
        ident: AccountIdent,
        phase: crate::mechanics::UnderworldPhase,
    },
    /// The underworld job came up in the queue - start a fresh round
    UnderworldMaintainTimer {
        ident: AccountIdent,
    },
    /// One more step of the same round, after the previous one came back
    UnderworldMaintainTick {
        ident: AccountIdent,
    },
    CopyBestLures {
        ident: AccountIdent,
    },
    SetAction(Option<ActionSelection>),
    /// Nothing to do - used where a command has to return something
    Noop,
    /// Time to ask the server whether there is a newer version
    UpdateCheck,
    /// The downloaded version takes over right now, quiet or not
    UpdateNow,
    /// OK on the level up box - shows the next one, if more are waiting
    LevelPopupOk,
    /// An epic find was acknowledged
    EpicPopupOk,
    /// Ask the server how many people use the app (owner only)
    FetchUserCount,
    /// The answer to that
    UserCount(Result<(Option<u64>, Option<u64>), String>),
    /// Sounds on or off
    SetSounds(bool),
    /// Write the guild chat to a file on the desktop, or stop
    SetChatArchive(bool),
    /// Show or hide the older lines from the archive above the live chat
    GuildArchiveToggle {
        ident: AccountIdent,
    },
    /// Leave the game's own lines (level ups and the like) out of the
    /// older lines, or put them back
    GuildArchivePeopleOnly {
        ident: AccountIdent,
        state: bool,
    },
    /// The server has nothing newer than what is running
    UpdateUpToDate,
    /// A newer version exists - now downloading it. Carried so the top-right
    /// can show "Downloading x.y" before it flips to "Update ready"
    UpdateFound(Box<crate::update::Available>),
    /// A newer version has been downloaded: where it sits, its version and
    /// the one-line note the manifest carried
    UpdateReady(std::path::PathBuf, String, String),
    /// The check or the download did not work out. Never fatal - the app
    /// keeps running on the version it has
    UpdateFailed(String),
}

/// The most "do it now" may run. It ends when the round is done - the
/// queues empty and nobody busy for a little while - and this is the cap
/// behind that, so a job that never finishes cannot hold the app at
/// button pace all night. It used to be the whole rule: a flat quarter of
/// an hour, which cut a round short when there was more to do and ran on
/// with nothing to do when there was less
pub const BOOST_MINUTES: i64 = 40;

/// How long the queues have to stay empty, with nobody busy, before a run
/// counts as finished. A few ticks, not one: a job's answer puts the next
/// job in the queue, and between the two the queue is empty for a moment
pub const BOOST_DONE_AFTER_SECS: i64 = 25;

/// How many times the game's arena offer may be thrown back in one
/// attempt. The re-roll is free, but three fresh strangers who are all
/// stronger than you is the server telling you where you stand, not bad
/// luck to be re-rolled away
const MAX_FREE_REROLLS: u8 = 2;

/// The window "do it now" spreads its characters over, as a session
/// length for `stealth::stagger_secs` - which takes a quarter of it. Eight
/// minutes, so the delays land inside the first two: enough that no two
/// characters ever act in the same second, small enough that the quarter
/// of an hour still gets through the whole round
const BOOST_STAGGER_SECS: i64 = 8 * 60;

/// "sessionid invalid": the game keeps one session per character, so
/// somebody logging in elsewhere - a browser, another device, another copy
/// of this app - kills ours. Every request from then on fails the same
/// way, so the session has to be thrown away and a new login started
/// rather than handed back to the next job
fn session_is_dead(error: &str) -> bool {
    error.to_lowercase().contains("session")
}

impl Helper {
    /// The pet simulator's typed opponent: the three messages that change
    /// it. Shared with the preview, which has no live handler
    pub(crate) fn pet_sim_input(&mut self, message: Message) {
        match message {
            Message::PetSimHabitat { ident, habitat } => {
                if let Some(account) = self.servers.get_account_mut(&ident) {
                    account.pet_info.sim.habitat = habitat;
                    // A pet number past what the new habitat has is kept
                    // - all five have twenty
                }
            }
            Message::PetSimTheirPet { ident, index } => {
                if let Some(account) = self.servers.get_account_mut(&ident) {
                    account.pet_info.sim.their_index = index;
                }
            }
            Message::PetSimField { ident, field, text } => {
                if let Some(account) = self.servers.get_account_mut(&ident) {
                    // Stored as typed - a field that refuses a keystroke
                    // cannot be typed into. The number is read off it
                    // where it is used
                    let sim = &mut account.pet_info.sim;
                    *match field {
                        PetSimField::Level => &mut sim.their_level,
                        PetSimField::Pack => &mut sim.their_pack,
                        PetSimField::At100 => &mut sim.their_at100,
                        PetSimField::At150 => &mut sim.their_at150,
                        PetSimField::At200 => &mut sim.their_at200,
                        PetSimField::Gladiator => &mut sim.their_gladiator,
                    } = text;
                }
            }
            _ => {}
        }
    }

    /// Work the pet simulator out again if what it reads has changed:
    /// the pets' levels, the habitats' stages, the gladiator trainer, the
    /// typed opponent. Cheap when nothing has - one hash - and that is
    /// what runs five times a second
    pub(crate) fn refresh_pet_sim(&mut self, ident: AccountIdent) {
        use std::hash::{Hash, Hasher};
        let Some(account) = self.servers.get_account_mut(&ident) else {
            return;
        };
        let arc = account.status.clone();
        let lock = arc.lock_safe();
        let gs = match &*lock {
            AccountStatus::Idle(_, gs) | AccountStatus::Busy(gs, _) => gs,
            _ => return,
        };
        let Some(pets) = gs.pets.as_ref() else {
            return;
        };
        let gladiator: u32 = gs
            .underworld
            .as_ref()
            .map(|uw| {
                uw.buildings[sf_api::gamestate::underworld::UnderworldBuildingType::GladiatorTrainer]
                    .level
                    .into()
            })
            .unwrap_or(0);
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for (habitat, info) in &pets.habitats {
            (habitat as usize).hash(&mut h);
            match info.exploration {
                sf_api::gamestate::unlockables::HabitatExploration::Finished => {
                    u32::MAX.hash(&mut h);
                }
                sf_api::gamestate::unlockables::HabitatExploration::Exploring {
                    fights_won,
                    next_fight_lvl,
                } => {
                    fights_won.hash(&mut h);
                    next_fight_lvl.hash(&mut h);
                }
            }
            for p in &info.pets {
                p.level.hash(&mut h);
            }
        }
        gladiator.hash(&mut h);
        let sim = &account.pet_info.sim;
        (sim.habitat as usize).hash(&mut h);
        sim.their_index.hash(&mut h);
        sim.their_level.hash(&mut h);
        sim.their_pack.hash(&mut h);
        sim.their_at100.hash(&mut h);
        sim.their_at150.hash(&mut h);
        sim.their_at200.hash(&mut h);
        sim.their_gladiator.hash(&mut h);
        let fingerprint = h.finish();
        if account
            .pet_info
            .sim_report
            .as_ref()
            .is_some_and(|(fp, _)| *fp == fingerprint)
        {
            return;
        }
        let report =
            crate::pet_sim::report(pets, gladiator, &account.pet_info.sim);
        account.pet_info.sim_report = Some((fingerprint, report));
    }

    pub fn handle_msg(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::UIActive => {
                // The pet page's simulator: worked out again when what
                // it is worked out from has changed, and only while that
                // page is the one on screen
                if let View::Account {
                    ident,
                    page: AccountPage::Pets,
                } = self.current_view
                {
                    self.refresh_pet_sim(ident);
                }
                // "Do it now" ends by itself: when the round is done, or
                // at the cap. Nothing about it is written to the config,
                // so a crash in the middle of one cannot leave the app at
                // button pace for ever
                if self.boosting() {
                    let now = Local::now();
                    let idle = !self.work_in_hand();
                    let since = *self.boost_idle_since.get_or_insert(now);
                    if !idle {
                        self.boost_idle_since = Some(now);
                    }
                    let done = idle
                        && (now - since).num_seconds()
                            >= BOOST_DONE_AFTER_SECS;
                    if done {
                        self.end_boost();
                        self.console(
                            None,
                            "Done going through it - the round is finished, \
                             back to the slider"
                                .to_string(),
                            None,
                        );
                    }
                } else if self.boost_until.is_some() {
                    self.end_boost();
                    self.console(
                        None,
                        "Done going through it - time is up, back to the \
                         slider"
                            .to_string(),
                        None,
                    );
                }
                // A downloaded update waits for a quiet moment. Restarting in
                // the middle of a request would lose whatever the character
                // was doing, and unattended farming is the whole point
                // The new version takes over the moment no request is in
                // flight - and after a while whatever the characters are
                // doing: a character stuck in a relogin used to hold a
                // broken build in place for the whole guild.
                //
                // How long that while is depends on what a drop costs. A
                // password character that is not idle logs in again from
                // the config and nobody notices. A Steam or Google one
                // cannot: that login is only renewable through the
                // browser, so dropping it puts the player back on the
                // login page by hand. Those are worth waiting minutes for,
                // not seconds - the update still goes out, just not at the
                // price of the key
                let sso_busy = self.servers.0.values().any(|server| {
                    server.accounts.values().any(|a| {
                        matches!(a.auth, crate::login::PlayerAuth::SSO)
                            && !matches!(
                                &*a.status.lock_safe(),
                                AccountStatus::Idle(..)
                            )
                    })
                });
                let patience = if sso_busy { 300 } else { 30 };
                let overdue = self.update_since.is_some_and(|t| {
                    Local::now() - t >= chrono::Duration::seconds(patience)
                });
                if self.update_ready.is_some()
                    && (self.quiet_enough() || overdue)
                {
                    let path = self.update_ready.take().unwrap_or_default();
                    self.update_since = None;
                    // The sessions go with it, so nobody logs in again (see
                    // handover.rs). This only comes back on failure - the
                    // app then carries on with the version it has
                    if let Err(e) = self.hand_over_to(&path) {
                        self.update_status = Some(format!(
                            "Could not start the new version: {e}"
                        ));
                        log::error!("update handover: {e}");
                    }
                }
                let (worth_saying, popups, epics) = self.watch_levels();
                for p in popups {
                    self.level_popups.push_back(p);
                }
                for e in epics {
                    self.epic_popups.push_back(e);
                }
                let any = !worth_saying.is_empty();
                for (ident, what) in worth_saying {
                    // A level up or an epic find is the best news there is
                    self.success(Some(ident), what.clone());
                    // ...and the goblin has a word about it
                    let mood = if what.starts_with("LEGENDARY!") {
                        Some(crate::goblin::Mood::LegendaryFound)
                    } else if what.starts_with("POTION!") {
                        Some(crate::goblin::Mood::PotionDrunk)
                    } else if what.starts_with("EPIC!") {
                        Some(crate::goblin::Mood::EpicFound)
                    } else if what.contains("evel") {
                        Some(crate::goblin::Mood::LevelUp)
                    } else {
                        None
                    };
                    self.console(Some(ident), what, Some(true));
                    if let Some(mood) = mood {
                        self.goblin(Some(ident), mood);
                    }
                }
                if any && self.config.sounds {
                    crate::sound::play(crate::sound::Sound::Celebrate);
                }
                self.sweep();

                // The quiet hours are quiet, not a logout: the characters
                // stay logged in and simply send nothing (`burst_active`
                // holds the sweep and the queues). Logging them out here
                // dropped the accounts out of the app altogether - and
                // nothing logs them back in, so the morning found an empty
                // Login page instead of a night's farming. An SSO session
                // torn down that way also knocks the player out of their
                // own game window
                let queues = self.run_queues();
                // Keep the console pinned to its newest line
                if self.console_open && self.console.len() != self.console_seen
                {
                    self.console_seen = self.console.len();
                    return Command::batch([
                        queues,
                        crate::ui::console_snap_to_end(),
                    ]);
                }
                return queues;
            }
            Message::Enqueue { ident, job } => {
                if let Some(account) = self.servers.get_account_mut(&ident)
                    && !account.queue.iter().any(|(j, _)| *j == job)
                {
                    account.queue.push((job, Local::now()));
                }
            }
            Message::Noop => {}
            Message::UpdateNow => {
                let Some(path) = self.update_ready.take() else {
                    return Command::none();
                };
                self.update_since = None;
                if let Err(e) = self.hand_over_to(&path) {
                    self.update_status =
                        Some(format!("Could not start the new version: {e}"));
                    log::error!("update handover: {e}");
                }
            }
            Message::UpdateCheck => {
                if self.update_ready.is_some() {
                    return Command::none();
                }
                self.update_status = Some("Asking the server...".to_string());
                // The same trip counts this install. Fire and forget - the
                // update never waits for it and never fails because of it
                let id = self.config.install_id.clone();
                let hello = Command::perform(crate::update::ping(id), |()| {
                    Message::Noop
                });
                let check = Command::perform(
                    async {
                        // Only the check here. The download is its own step so
                        // the top-right can show "Downloading" in between
                        match crate::update::check().await {
                            Ok(Some(a)) => Some(Ok(a)),
                            Ok(None) => None,
                            Err(e) => Some(Err(e.to_string())),
                        }
                    },
                    |res| match res {
                        // Nothing newer. The automatic check every ten
                        // minutes used to swallow this, which is fine until
                        // somebody presses the button and sees nothing happen
                        None => Message::UpdateUpToDate,
                        Some(Ok(available)) => {
                            Message::UpdateFound(Box::new(available))
                        }
                        Some(Err(e)) => Message::UpdateFailed(e),
                    },
                );
                return Command::batch([hello, check]);
            }
            Message::UpdateFound(available) => {
                // A newer version is out - show it, then fetch it. The button
                // reads this to say "Downloading x.y" until the file is here
                if self.update_ready.is_some() {
                    return Command::none();
                }
                self.update_status =
                    Some(format!("Downloading {}...", available.version));
                return Command::perform(
                    async move {
                        let version = available.version.to_string();
                        let notes = available.manifest.notes.clone();
                        match crate::update::download(&available).await {
                            Ok(path) => Ok((path, version, notes)),
                            Err(e) => Err(e.to_string()),
                        }
                    },
                    |res| match res {
                        Ok((path, version, notes)) => {
                            log::info!("downloaded version {version}");
                            Message::UpdateReady(path, version, notes)
                        }
                        Err(e) => Message::UpdateFailed(e),
                    },
                );
            }
            Message::FetchUserCount => {
                if !self.config.admin_unlocked {
                    return Command::none();
                }
                self.user_count = None;
                return Command::perform(
                    crate::update::user_count(),
                    Message::UserCount,
                );
            }
            Message::UserCount(res) => {
                self.user_count = Some(res);
            }
            Message::EpicPopupOk => {
                self.epic_popups.pop_front();
            }
            Message::LevelPopupOk => {
                self.level_popups.pop_front();
            }
            Message::GuildArchiveToggle { ident } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let show = !account.social.show_archive;
                account.social.show_archive = show;
                if show {
                    // Read back from the file, the last thousand lines -
                    // what the game has already let go of is exactly
                    // what this is for
                    let guild_name = {
                        let arc = account.status.clone();
                        let lock = arc.lock_safe();
                        match &*lock {
                            AccountStatus::Idle(_, gs)
                            | AccountStatus::Busy(gs, _) => {
                                gs.guild.as_ref().map(|g| g.name.clone())
                            }
                            _ => None,
                        }
                    };
                    account.social.archive_lines = guild_name
                        .map(|g| {
                            crate::archive::read_tail(
                                &crate::archive::chat_file(&g),
                                1000,
                            )
                        })
                        .unwrap_or_default();
                } else {
                    account.social.archive_lines.clear();
                }
            }
            Message::GuildArchivePeopleOnly { ident, state } => {
                if let Some(account) = self.servers.get_account_mut(&ident) {
                    account.social.archive_people_only = state;
                }
            }
            Message::SetChatArchive(on) => {
                self.config.chat_archive = on;
                _ = self.config.write();
                // Switched on: the next refresh writes what the game holds
                // now, and the note says where. Switched off: the memory
                // of what was written goes too, so switching on again
                // later starts by writing what is there then
                if !on {
                    self.chat_archived.clear();
                    self.chat_archive_note = None;
                }
            }
            Message::SetSounds(on) => {
                self.config.sounds = on;
                _ = self.config.write();
                // So the switch says something the moment it is pressed
                if on {
                    crate::sound::play(crate::sound::Sound::Message);
                }
            }
            Message::UpdateUpToDate => {
                // The plain answer to the plain question on the button. The
                // time is there so a second press is visibly a new answer
                self.update_status = Some(format!(
                    "There's no new update ({})",
                    Local::now().format("%H:%M")
                ));
            }
            Message::UpdateReady(path, version, notes) => {
                self.update_status = Some(if notes.trim().is_empty() {
                    format!("Version {version} is ready")
                } else {
                    format!("Version {version} is ready - {}", notes.trim())
                });
                self.update_ready = Some(path);
                self.update_since = Some(Local::now());
                self.console(
                    None,
                    "A new version was downloaded - it takes over as soon as \
                     no character is in the middle of something"
                        .to_string(),
                    Some(true),
                );
            }
            Message::UpdateFailed(err) => {
                // Never fatal: the app carries on with the version it has
                self.update_status =
                    Some(format!("Update check failed: {err}"));
                log::warn!("update: {err}");
            }
            Message::PageCrawled => {
                // Gets handled in crawling
            }
            Message::CrawlerDied { server, error } => {
                log::error!("Crawler died on {server} - {error}");

                let Some(server) = self.servers.get_mut(&server) else {
                    return Command::none();
                };
                server.crawling = CrawlingStatus::CrawlingFailed(error)
            }
            Message::CharacterCrawled {
                server,
                que_id,
                character,
            } => {
                let Some(server) = self.servers.get_mut(&server) else {
                    return Command::none();
                };

                trace!("{} crawled {}", server.ident.ident, character.name);

                let CrawlingStatus::Crawling {
                    player_info,
                    equipment,
                    que_id: crawl_que_id,
                    last_update,
                    que,
                    recent_failures,
                    naked,
                    ..
                } = &mut server.crawling
                else {
                    return Command::none();
                };

                let crawler_finished = {
                    let mut lock = que.lock_safe();
                    if let Some(pb) = &server.headless_progress {
                        let remaining = lock.count_remaining();
                        let crawled = player_info.len();
                        let total = remaining + crawled;
                        pb.set_length(total as u64);
                        pb.set_position(crawled as u64);
                    };
                    lock.in_flight_accounts.remove(&character.name);
                    lock.todo_pages.is_empty() && lock.todo_accounts.is_empty()
                };

                if *crawl_que_id != que_id {
                    // This was crawled for an outdated que version (we cleared
                    // the que)
                    return Command::none();
                }

                // We were able to make this request, so something must work
                recent_failures.clear();
                server.crawler_revives = 0;

                *last_update = Local::now();

                handle_new_char_info(character, equipment, player_info, naked);

                if crawler_finished {
                    let mut commands = vec![];
                    let todo: Vec<_> =
                        server.accounts.values().map(|a| a.ident).collect();
                    for acc in todo {
                        commands.push(self.update_best(acc, false));
                    }
                    return Command::batch(commands);
                }

                if let View::Account { ident, .. } = self.current_view
                    && let Some(current) =
                        server.accounts.get_mut(&ident.account)
                {
                    let ident = current.ident;
                    return self.update_best(ident, true);
                }
            }
            Message::CrawlerIdle(server_id) => {
                let Some(server) = self.servers.get_mut(&server_id) else {
                    return Command::none();
                };
                let CrawlingStatus::Crawling {
                    player_info, que, ..
                } = &mut server.crawling
                else {
                    return Command::none();
                };
                let lock = que.lock_safe();
                if server.headless_progress.is_none()
                    || !lock.todo_pages.is_empty()
                    || !lock.todo_accounts.is_empty()
                    || player_info.is_empty()
                {
                    return Command::none();
                }
                let backup = lock.create_backup(player_info);
                let ident = server.ident.ident.to_string();
                let id = server.ident.id;

                return Command::perform(
                    async move { backup.write(&ident).await },
                    move |res| Message::BackupRes {
                        server: id,
                        error: res.err().map(|a| a.to_string()),
                    },
                );
            }
            Message::CrawlerNoPlayerResult => {
                // Maybe we want to count this as an error?
                warn!("No player result");
            }
            Message::CrawlerUnable {
                server: server_id,
                action,
                error,
            } => {
                let Some(server) = self.servers.get_mut(&server_id) else {
                    return Command::none();
                };
                let CrawlingStatus::Crawling {
                    que_id,
                    que,
                    recent_failures,
                    crawling_session,
                    ..
                } = &mut server.crawling
                else {
                    return Command::none();
                };

                let mut lock = que.lock_safe();
                match &action {
                    CrawlAction::Wait | CrawlAction::InitTodo => {}
                    CrawlAction::Page(a, b) => {
                        if *b != *que_id {
                            return Command::none();
                        }
                        lock.in_flight_pages.retain(|x| x != a);
                        if error == CrawlerError::RateLimit {
                            lock.todo_pages.push(*a);
                            return Command::none();
                        } else {
                            lock.invalid_pages.push(*a);
                        }
                    }
                    CrawlAction::Character(a, b) => {
                        if *b != *que_id {
                            return Command::none();
                        }
                        lock.in_flight_accounts.remove(a);
                        if error == CrawlerError::RateLimit {
                            lock.todo_accounts.push(a.clone());
                            return Command::none();
                        } else {
                            lock.invalid_accounts.push(a.clone());
                        }
                    }
                }

                match error {
                    CrawlerError::NotFound => {
                        return Command::none();
                    }
                    // Once the crawler has had to log in again twice in a
                    // row, the same ten lines would follow every attempt.
                    // The first round says what is wrong; the rest is noise
                    CrawlerError::Generic(err) if server.crawler_revives < 2 => {
                        warn!(
                            "Crawler was unable to complete: '{action}' on {} \
                             -> {err}",
                            server.ident.id
                        );
                    }
                    CrawlerError::Generic(err) => debug!(
                        "Crawler was unable to complete: '{action}' on {} -> \
                         {err}",
                        server.ident.id
                    ),
                    CrawlerError::RateLimit => {}
                }

                recent_failures.push(action);

                if recent_failures.len() != 10 {
                    return Command::none();
                }
                debug!("Restarting crawler on {}", server.ident.ident);

                // The last 10 command failed consecutively. This means there
                // is some sort of issue with either the internet connection, or
                // the session. To resolve this, we try to login the crawler
                // again.

                let Some(state) = crawling_session.clone() else {
                    return Command::none();
                };

                let id = server.ident.ident.clone();
                server.crawler_revives = server.crawler_revives.saturating_add(1);
                let revives = server.crawler_revives;
                let pause = crate::server::crawler_backoff(revives);
                if pause.is_zero() {
                    info!("Crawler on {id}: session lost, logging in again");
                } else {
                    warn!(
                        "Crawler on {id}: session lost {revives}x in a row - \
                         waiting {} s before the next login. Is another copy \
                         of SF Commander running with the same helper.toml?",
                        pause.as_secs()
                    );
                }

                return Command::perform(
                    async move {
                        // Holding the session lock through the pause: the
                        // crawler threads wait on it, so nothing goes out
                        // until the login below is done
                        let mut session_lock = state.session.write().await;
                        if !pause.is_zero() {
                            sleep(pause).await;
                        }
                        // A login that fails outright backs off as well -
                        // 2 s, 4 s, ... up to 10 min - instead of hammering
                        // the server every couple of seconds
                        let mut retry = Duration::from_secs(2);
                        loop {
                            debug!("Relog crawler on {}", id);
                            let Ok(resp) = session_lock.login().await else {
                                error!("Could not login crawler on {}", id);
                                sleep(retry).await;
                                retry = (retry * 2).min(Duration::from_secs(600));
                                continue;
                            };
                            let Ok(new_gs) = GameState::new(resp) else {
                                error!(
                                    "Could not parse GS for crawler on {}",
                                    id
                                );
                                // we can not hold mutex guards accross awaits
                                sleep(retry).await;
                                retry = (retry * 2).min(Duration::from_secs(600));
                                continue;
                            };
                            sleep(Duration::from_secs(5)).await;

                            let mut gs = state.gs.lock_safe();
                            *gs = new_gs;
                            return;
                        }
                    },
                    move |()| Message::CrawlerRevived { server_id },
                );
            }
            Message::ViewLogin => self.current_view = View::Login,
            Message::LoginNameInputChange(a) => self.login_state.name = a,
            Message::LoginSFSubmit => {
                return self.login_sf_acc(
                    self.login_state.name.clone(),
                    PWHash::new(&self.login_state.password),
                    self.login_state.remember_me,
                    false,
                );
            }
            Message::LoginPWInputChange(a) => self.login_state.password = a,
            Message::LoginServerChange(a) => self.login_state.server = a,
            Message::LoginRegularSubmit => {
                let pw_hash = PWHash::new(&self.login_state.password.clone());

                return self.login_regular(
                    self.login_state.name.to_string(),
                    self.login_state.server.to_string(),
                    pw_hash,
                    self.login_state.remember_me,
                    Default::default(),
                );
            }
            Message::LoginViewChanged(a) => {
                self.login_state.login_typ = a;
            }
            Message::LoggininSuccess {
                gs,
                session,
                remember,
                ident,
            } => {
                info!("Successfully logged in {ident}",);

                let Some(server) = self.servers.0.get_mut(&ident.server_id)
                else {
                    return Command::none();
                };
                let Some(player) = server.accounts.get_mut(&ident.account)
                else {
                    return Command::none();
                };

                if remember {
                    match &player.auth {
                        PlayerAuth::Normal(hash) => {
                            self.config.accounts.retain(|a| match &a {
                                AccountConfig::Regular {
                                    name,
                                    server: server_url,
                                    ..
                                } => {
                                    !(name == &player.name
                                        && server_url == &server.ident.url)
                                }
                                _ => true,
                            });
                            self.config.accounts.push(AccountConfig::new(
                                AccountCreds::Regular {
                                    name: player.name.clone(),
                                    pw_hash: hash.clone(),
                                    server: server.ident.url.clone(),
                                },
                            ));
                            _ = self.config.write();
                        }
                        PlayerAuth::SSO => {}
                    }
                }

                let total_players = gs.hall_of_fames.players_total;
                let total_pages = (total_players as usize).div_ceil(PER_PAGE);

                let char_conf =
                    self.config.get_char_conf(&player.name, ident.server_id);

                player.scrapbook_info = ScrapbookInfo::new(
                    &gs, char_conf, self.config.target_order,
                );
                player.underworld_info = UnderworldInfo::new(&gs, char_conf);
                player.fortress_info = FortressInfo::new(&gs, char_conf);
                player.social.compact_members = true;
                player.social.report_auto = false;
                player.social.report_hour =
                    char_conf.map(|a| a.guild_report_hour).unwrap_or(20);
                player.social.auto_attack =
                    char_conf.map(|a| a.guild_attack).unwrap_or(true);
                player.social.auto_defense =
                    char_conf.map(|a| a.guild_defense).unwrap_or(true);
                player.social.raid_auto =
                    char_conf.map(|a| a.guild_raid_auto).unwrap_or(false);
                player.social.finder_auto =
                    char_conf.map(|a| a.guild_finder_auto).unwrap_or(false);
                player.social.portal_auto =
                    char_conf.map(|a| a.guild_portal).unwrap_or(true);
                player.dungeon_info.auto =
                    char_conf.map(|a| a.dungeon_auto).unwrap_or(false);
                player.dungeon_info.min_chance =
                    char_conf.map(|a| a.dungeon_min_chance).unwrap_or(0);
                player.dungeon_info.mushroom =
                    char_conf.map(|a| a.dungeon_mushroom).unwrap_or(false);
                player.dungeon_info.mushroom_min =
                    char_conf.map(|a| a.dungeon_mushroom_min).unwrap_or(50);
                player.dungeon_info.mushroom_max =
                    char_conf.map(|a| a.dungeon_mushroom_max).unwrap_or(5);
                player.dungeon_info.portal_auto =
                    char_conf.map(|a| a.dungeon_portal).unwrap_or(true);
                player.pet_info.auto_feed =
                    char_conf.map(|a| a.pets_feed).unwrap_or(false);
                player.pet_info.auto_arena =
                    char_conf.map(|a| a.pets_arena).unwrap_or(true);
                player.idle_info.auto_upgrade =
                    char_conf.map(|a| a.idle_upgrade).unwrap_or(false);
                // There is nothing to collect in the Arena Manager - the
                // buildings pay out on their own - and the raw "collect"
                // request only ever wrote "Collected Seat" a dozen times a
                // minute. Off, whatever a config says
                player.idle_info.auto_collect = false;
                player.shop_info.rolls_per_day =
                    char_conf.map(|a| a.shop_rolls_per_day).unwrap_or(3);
                {
                    let ei = &mut player.expedition_info;
                    ei.auto =
                        char_conf.map(|a| a.expedition_auto).unwrap_or(false);
                    ei.goal = if char_conf
                        .map(|a| a.expedition_gold)
                        .unwrap_or(false)
                    {
                        crate::expedition::ExpeditionGoal::Gold
                    } else {
                        crate::expedition::ExpeditionGoal::Xp
                    };
                    ei.skip_glass = char_conf
                        .map(|a| a.expedition_skip_glass)
                        .unwrap_or(true);
                    // Never quests: expeditions pay better for the same
                    // thirst. The config field stays for old files, the
                    // switch is gone from the page
                    ei.quest_auto = false;
                    // Expedition waits are skipped with quicksand glasses
                    // only - the mushroom variant stays off
                    ei.skip_mushroom = false;
                }
                player.dailies_info.auto =
                    char_conf.map(|a| a.dailies_auto).unwrap_or(true);
                player.dailies_info.mount_auto =
                    char_conf.map(|a| a.mount_auto).unwrap_or(true);
                if let Some(fi) = &mut player.fortress_info {
                    fi.gem_wanted = char_conf
                        .map(|a| a.gem_wanted.clone())
                        .unwrap_or_default();
                    fi.gem_socket =
                        char_conf.map(|a| a.gem_socket).unwrap_or(false);
                }
                player.guard_info.auto =
                    char_conf.map(|a| a.guard_auto).unwrap_or(false);
                player.guard_info.hours =
                    char_conf.map(|a| a.guard_hours).unwrap_or(10).clamp(1, 10);
                player.guard_info.cutoff_enabled =
                    char_conf.map(|a| a.guard_cutoff_enabled).unwrap_or(true);
                player.guard_info.cutoff = char_conf
                    .map(|a| a.guard_cutoff.clone())
                    .unwrap_or_else(|| "23:30".to_string());
                player.dungeon_info.pets_first =
                    char_conf.map(|a| a.dungeon_pets_first).unwrap_or(true);
                player.social.guild_founded = char_conf
                    .map(|a| a.guild_founded.clone())
                    .unwrap_or_default();
                player.idle_info.auto_sacrifice =
                    char_conf.map(|a| a.idle_sacrifice).unwrap_or(false);
                player.idle_info.sacrifice_gain_pct = char_conf
                    .map(|a| a.idle_sacrifice_gain_pct)
                    .unwrap_or(20)
                    .max(1);
                player.idle_info.sacrificed_at_resets = match char_conf
                    .map(|a| a.idle_sacrificed_units)
                    .unwrap_or(0)
                {
                    0 => None,
                    u => u32::try_from(u).ok(),
                };
                player.idle_info.last_sacrifice_runes =
                    char_conf.map(|a| a.idle_last_sacrifice).unwrap_or(0);
                {
                    let si = &mut player.shop_info;
                    let c = |f: fn(&crate::config::CharacterConfig) -> bool,
                             d: bool| {
                        char_conf.map(f).unwrap_or(d)
                    };
                    // Gear buying is part of the ON/OFF set, no switch of
                    // its own: ON writes it into the config, OFF takes it
                    // out, and a start picks up whatever was running before
                    // - never a purchase on its own right after the app
                    // opens on a character that had the automation off
                    si.auto = c(|a| a.shop_auto, false);
                    si.min_gain =
                        char_conf.map(|a| a.shop_min_gain).unwrap_or(5);
                    si.gold_reserve = char_conf
                        .map(|a| a.gold_reserve)
                        .unwrap_or(30)
                        .min(100);
                    si.protect_epics = c(|a| a.shop_protect_epics, true);
                    si.legendary_swap_pct = char_conf
                        .map(|a| a.legendary_swap_pct)
                        .unwrap_or(50);
                    si.protect_enchanted =
                        c(|a| a.shop_protect_enchanted, true);
                    si.protect_upgraded = c(|a| a.shop_protect_upgraded, true);
                    si.smith_auto = c(|a| a.smith_auto, true);
                    si.witch_auto = c(|a| a.witch_auto, true);
                    si.recommended_gain =
                        crate::shopping::recommended_min_gain(None);
                    // Selling the replaced piece is off unless the player
                    // asked: a sale is the one step that cannot be undone
                    si.sell_old = c(|a| a.shop_sell_old, false);
                    si.clear_backpack = c(|a| a.shop_clear_backpack, true);
                    // The companions are always dressed from the bag
                    let _ = c(|a| a.equip_companions, true);
                    si.equip_companions = true;
                    si.keep_free = char_conf
                        .map(|a| a.shop_keep_free)
                        .unwrap_or(2)
                        .clamp(1, 10);
                    si.mushrooms = c(|a| a.shop_mushrooms, false);
                    si.attr_auto = c(|a| a.attr_auto, false);
                    si.attr_w_main =
                        char_conf.map(|a| a.attr_w_main).unwrap_or(10);
                    si.attr_w_con =
                        char_conf.map(|a| a.attr_w_con).unwrap_or(10);
                    si.attr_w_luck =
                        char_conf.map(|a| a.attr_w_luck).unwrap_or(5);
                }
                {
                    let ti = &mut player.tavern_info;
                    ti.beer_auto =
                        char_conf.map(|a| a.beer_auto).unwrap_or(true);
                    ti.beer_max = char_conf.map(|a| a.beer_max).unwrap_or(11);
                    ti.mushroom_reserve =
                        char_conf.map(|a| a.mushroom_reserve).unwrap_or(10);
                    ti.mushroom_budget =
                        char_conf.map(|a| a.mushroom_day_budget).unwrap_or(0);
                    ti.beer_free =
                        char_conf.map(|a| a.beer_free).unwrap_or(false);
                    ti.wheel_auto =
                        char_conf.map(|a| a.wheel_auto).unwrap_or(false);
                    ti.dice_auto =
                        char_conf.map(|a| a.dice_auto).unwrap_or(false);
                    ti.dice_skip =
                        char_conf.map(|a| a.dice_skip).unwrap_or(true);
                    // 0 in an old file means "never set" (the derive
                    // default of the day), not "no spins" - the checkbox
                    // is for that. Twenty is the day's limit
                    ti.wheel_spins = match char_conf.map(|a| a.wheel_spins) {
                        Some(0) | None => crate::config::default_wheel_spins(),
                        Some(n) => n.clamp(1, 20),
                    };
                }

                // An update handed the switches over as they were - they
                // win over the config, which for a Steam login has nothing
                if let Some(flags) = player.pending_flags.take() {
                    flags.apply(player);
                }

                *player.status.lock_safe() = AccountStatus::Idle(session, gs);

                // The arena's three opponents, so the page has them the
                // first time it is opened - a few seconds after the login
                // settles, not on top of it
                let offer = Command::perform(
                    async {
                        sleep(Duration::from_secs(8)).await;
                    },
                    move |()| Message::ArenaOfferRefresh {
                        ident,
                        then_fight: false,
                    },
                );
                let server_ident = server.ident.ident.clone();
                let server_id = server.ident.id;
                let afn = self.config.auto_fetch_newest;
                match &server.crawling {
                    CrawlingStatus::Waiting => {
                        server.crawling = CrawlingStatus::Restoring;
                        let restore = Command::perform(
                            async move {
                                let backup =
                                    get_newest_backup(server_ident, afn).await;
                                Box::new(
                                    restore_backup(backup, total_pages).await,
                                )
                            },
                            move |backup| Message::ResetCrawling {
                                server: server_id,
                                status: backup,
                            },
                        );
                        return Command::batch([offer, restore]);
                    }
                    CrawlingStatus::Crawling { .. } => {
                        let ident = player.ident;
                        let best = self.update_best(ident, false);
                        return Command::batch([offer, best]);
                    }
                    _ => return offer,
                }
            }
            Message::LoggininFailure { error, ident } => {
                error!("Error loggin in {ident}: {error}");
                let Some((_, player)) = self.servers.get_ident(&ident) else {
                    return Command::none();
                };
                *player.status.lock_safe() = AccountStatus::FatalError(error)
            }
            Message::ShowPlayer { ident } => {
                let Some(server) = self.servers.0.get_mut(&ident.server_id)
                else {
                    return Command::none();
                };
                let Some(account) = server.accounts.get_mut(&ident.account)
                else {
                    return Command::none();
                };

                self.current_view = View::Account {
                    ident,
                    page: AccountPage::Scrapbook,
                };

                let CrawlingStatus::Crawling { last_update, .. } =
                    &server.crawling
                else {
                    return Command::none();
                };

                if account.last_updated < *last_update {
                    let ident = account.ident;
                    return self.update_best(ident, false);
                }
            }
            Message::ResetCrawling {
                server: server_id,
                status,
            } => {
                let Some(server) = self.servers.get_mut(&server_id) else {
                    return Command::none();
                };

                let mut commands = vec![];
                match &mut server.crawling {
                    CrawlingStatus::Waiting | CrawlingStatus::Restoring => {
                        server.crawling = status.into_status();
                        commands.push(server.set_threads(
                            self.config.start_threads, &self.config.base_name,
                        ));
                    }
                    CrawlingStatus::Crawling {
                        que_id,
                        que,
                        player_info,
                        equipment,
                        last_update,
                        recent_failures,
                        naked,
                        threads: _,
                        crawling_session: _,
                    } => {
                        let mut que = que.lock_safe();
                        que.que_id = status.que_id;
                        que.todo_accounts = status.todo_accounts;
                        que.todo_pages = status.todo_pages;
                        que.invalid_accounts = status.invalid_accounts;
                        que.invalid_pages = status.invalid_pages;
                        que.order = status.order;
                        que.in_flight_pages = vec![];
                        que.in_flight_accounts = Default::default();
                        *que_id = status.que_id;
                        *naked = status.naked;
                        *player_info = status.player_info;
                        *equipment = status.equipment;
                        *last_update = Local::now();
                        recent_failures.clear();
                        drop(que);
                    }
                    CrawlingStatus::CrawlingFailed(_) => {
                        return Command::none();
                    }
                }

                let CrawlingStatus::Crawling { .. } = &server.crawling else {
                    return Command::none();
                };

                let todo: Vec<_> =
                    server.accounts.values().map(|a| a.ident).collect();
                for acc in todo {
                    commands.push(self.update_best(acc, false));
                }
                return Command::batch(commands);
            }
            Message::RemoveAccount { ident } => {
                let Some(server) = self.servers.get_mut(&ident.server_id)
                else {
                    return Command::none();
                };
                if let Some(old) = server.accounts.remove(&ident.account)
                    && matches!(old.auth, PlayerAuth::SSO)
                    && let Ok(mut sl) = old.status.lock()
                    && let Some(session) = sl.take_session("Removing")
                {
                    self.login_state.import_que.push(*session);
                }
                if server.accounts.is_empty()
                    && let CrawlingStatus::Crawling { threads, .. } =
                        &mut server.crawling
                {
                    *threads = 0;
                }

                match &mut self.current_view {
                    View::Account { ident: current, .. }
                        if ident == *current =>
                    {
                        self.current_view = View::Login;
                    }
                    View::Overview { selected, action } => {
                        _ = selected.remove(&ident);
                        *action = None;
                    }
                    _ => {}
                }
            }
            Message::CrawlerSetThreads {
                server: server_id,
                new_count,
            } => {
                let new_count = new_count.clamp(0, self.config.max_threads);
                let Some(server) = self.servers.get_mut(&server_id) else {
                    return Command::none();
                };

                return server.set_threads(new_count, &self.config.base_name);
            }
            Message::ClearHof(server_id) => {
                let Some(server) = self.servers.get_mut(&server_id) else {
                    return Command::none();
                };

                let Some(tp) = server.accounts.iter().find_map(|(_, b)| {
                    match &*b.status.lock_safe() {
                        AccountStatus::LoggingInAgain
                        | AccountStatus::LoggingIn
                        | AccountStatus::FatalError(_) => None,
                        AccountStatus::Idle(_, gs)
                        | AccountStatus::Busy(gs, _) => {
                            Some(gs.hall_of_fames.players_total)
                        }
                    }
                }) else {
                    return Command::none();
                };

                let tp = (tp as usize).div_ceil(PER_PAGE);

                let id = server.ident.id;

                return Command::perform(
                    async move { Box::new(restore_backup(None, tp).await) },
                    move |res| Message::ResetCrawling {
                        server: id,
                        status: res,
                    },
                );
            }
            Message::RememberMe(val) => self.login_state.remember_me = val,
            Message::Login {
                account,
                auto_login,
            } => match account {
                AccountConfig::Regular {
                    name,
                    pw_hash,
                    server,
                    ..
                } => {
                    return self.login_regular(
                        name, server, pw_hash, false, auto_login,
                    );
                }
                AccountConfig::SF { name, pw_hash, .. } => {
                    return self.login_sf_acc(name, pw_hash, false, auto_login);
                }
            },
            Message::OrderChange { server, new } => {
                let Some(server) = self.servers.get_mut(&server) else {
                    return Command::none();
                };
                if let CrawlingStatus::Crawling { que, .. } = &server.crawling {
                    let mut que = que.lock_safe();
                    que.order = new;
                    new.apply_order(&mut que.todo_pages);
                }
            }
            Message::AutoBattlePossible { ident } => {
                let refetch = self.update_best(ident, true);
                let key = self.char_key(&ident);

                let Some(server) = self.servers.0.get_mut(&ident.server_id)
                else {
                    return Command::none();
                };
                let Some(account) = server.accounts.get_mut(&ident.account)
                else {
                    return Command::none();
                };

                // The arena picks its target from the crawled Hall of Fame
                // list. Without a running crawler there is nothing to pick
                // from - but this used to return in silence, so the arena
                // just looked broken with no reason given anywhere
                let CrawlingStatus::Crawling { .. } = &server.crawling else {
                    let why = match &server.crawling {
                        CrawlingStatus::CrawlingFailed(e) => format!(
                            "The Hall of Fame crawler failed ({e}) - the \
                             arena has no targets to choose from"
                        ),
                        _ => "Waiting for the Hall of Fame - the arena needs \
                              the crawled list to pick a target"
                            .to_string(),
                    };
                    log::info!("{ident}: arena idle - {why}");
                    set_auto_status(account, why);
                    return refetch;
                };

                // The Arc is cloned, so that the guard does not borrow the
                // account, which we still want to write the status into
                let status_arc = account.status.clone();
                let mut status = status_arc.lock_safe();
                let AccountStatus::Idle(_, gs) = &*status else {
                    drop(status);
                    log::info!("{ident}: arena idle - Character is busy");
                    set_auto_status(account, "Character is busy");
                    return refetch;
                };
                let next = gs.arena.next_free_fight.unwrap_or_default();
                let free = next <= Local::now() + Duration::from_millis(200);
                // The mushroom allowance renews with the server's day, and
                // what was spent of it is on disk (see `spend_ledger`)
                let today = gs.server_time().current().date();
                let spent =
                    key.as_deref().map(|k| self.spend.today(k, today).arena);
                if let Some(si) = account.scrapbook_info.as_mut() {
                    if si.mushroom_day != Some(today) {
                        si.mushroom_day = Some(today);
                        si.mushroom_today = 0;
                    }
                    if let Some(spent) = spent {
                        si.mushroom_today = si.mushroom_today.max(spent);
                    }
                }
                // The free fight comes first; a mushroom is only spent once
                // it is on cooldown, the user turned it on, and the daily
                // ceiling is not yet hit
                let use_mush = if free {
                    false
                } else if account.scrapbook_info.as_ref().is_some_and(|s| {
                    s.mushroom && s.mushroom_today < s.mushroom_max
                }) && account.free_mushrooms(gs) > 0
                {
                    true
                } else {
                    drop(status);
                    // Show the actual countdown, not just "waiting" - the
                    // arena free fight is on a ten minute timer and the user
                    // wants to see how long is left
                    let secs = (next - Local::now()).num_seconds().max(0);
                    log::info!(
                        "{ident}: arena idle - Waiting for the free fight \
                         ({}:{:02} left)",
                        secs / 60,
                        secs % 60
                    );
                    set_auto_status(
                        account,
                        format!(
                            "Waiting for the free fight - {}:{:02} left",
                            secs / 60,
                            secs % 60
                        ),
                    );
                    return refetch;
                };

                // The free fight is once every ten minutes. When the game's
                // own timer does not move - a refused fight, a game state
                // that was not updated - the check above passes every time
                // and the app attacks over and over. This floor is what
                // stops that from becoming a flood of requests
                const MIN_GAP: chrono::TimeDelta =
                    chrono::TimeDelta::minutes(5);
                if let Some(last) = account
                    .scrapbook_info
                    .as_ref()
                    .and_then(|si| si.last_auto_attack)
                    && Local::now() - last < MIN_GAP
                {
                    let secs = (MIN_GAP - (Local::now() - last)).num_seconds();
                    drop(status);
                    log::info!(
                        "{ident}: arena - the game's fight timer has not \
                         moved since the last attack, waiting {secs}s"
                    );
                    set_auto_status(
                        account,
                        format!(
                            "The game's fight timer has not moved - waiting \
                             {secs}s before trying again"
                        ),
                    );
                    return refetch;
                }

                let Some(mut session) = status.take_session("A Fighting")
                else {
                    drop(status);
                    log::info!("{ident}: arena idle - Character is busy");
                    set_auto_status(account, "Character is busy");
                    return refetch;
                };

                let Some(si) = &account.scrapbook_info else {
                    status.put_session(session);
                    return refetch;
                };

                let total_len = si.best.len();

                // With nothing crawled there is still the arena itself: the
                // game always puts up three opponents. A free fight is worth
                // more spent on the weakest of those than thrown away waiting
                // for the crawler - only never a mushroom, there is no
                // scrapbook item in it
                let mut offer_target: Option<crate::AttackTarget> = None;
                if total_len == 0 {
                    if use_mush {
                        status.put_session(session);
                        drop(status);
                        set_auto_status(
                            account,
                            "No scrapbook target - not spending a mushroom on \
                             the game's offer",
                        );
                        return refetch;
                    }
                    if !si.offer_fresh() {
                        status.put_session(session);
                        drop(status);
                        log::info!(
                            "{ident}: arena - no crawled target, asking the \
                             game for its offer"
                        );
                        set_auto_status(
                            account,
                            "No crawled target - checking the game's own \
                             arena offer",
                        );
                        let ask = Command::perform(async {}, move |()| {
                            Message::ArenaOfferRefresh {
                                ident,
                                then_fight: true,
                            }
                        });
                        return Command::batch([refetch, ask]);
                    }
                    // The chance is the simulation's, bent by what really
                    // happened in fights it was this confident about and by
                    // this opponent's own record (see `fight_log`)
                    let cal_key = key.clone().unwrap_or_default();
                    match si.best_offer(&self.fights, &cal_key) {
                        Some(o) => {
                            // Say it out loud when the record and the
                            // simulation disagree - that line is the whole
                            // point of keeping the record
                            if let Some(line) = self
                                .fights
                                .disagreement(&cal_key, &o.name, o.win)
                            {
                                Self::push_think(
                                    &mut self.console, &account.name, line,
                                );
                            }
                            offer_target =
                                Some(crate::AttackTarget::from_offer(o));
                        }
                        None => {
                            status.put_session(session);
                            drop(status);
                            // **All three under fifty: throw them back.**
                            // `CheckArena` puts up three new opponents and
                            // costs nothing at all, so sitting on a trio
                            // nobody can beat is a free fight left on the
                            // table. Twice at most, and never in a circle:
                            // if the game hands back the same three, that
                            // is its answer and we stop asking
                            let ids: Vec<u32> = {
                                let mut v: Vec<u32> = si
                                    .arena_offer
                                    .iter()
                                    .map(|o| o.pid)
                                    .collect();
                                v.sort_unstable();
                                v
                            };
                            let same_again = ids == si.offer_seen_ids;
                            let tries_left =
                                si.offer_rerolls < MAX_FREE_REROLLS;
                            let Some(si) = &mut account.scrapbook_info else {
                                return refetch;
                            };
                            if tries_left && !same_again && !ids.is_empty() {
                                si.offer_rerolls += 1;
                                si.offer_seen_ids = ids;
                                let n = si.offer_rerolls;
                                log::info!(
                                    "{ident}: arena - all three under 50 %, \
                                     asking the game for three new ones \
                                     (free re-roll {n})"
                                );
                                set_auto_status(
                                    account,
                                    "All three under 50 % - asking the game \
                                     for three new ones (it is free)",
                                );
                                let name = account.name.clone();
                                Self::push_think(
                                    &mut self.console,
                                    &name,
                                    match crate::i18n::lang() {
                                        crate::i18n::Lang::Cs => format!(
                                            "všichni tři soupeři, které hra \
                                             nabídla, jsou pod 50 % - beru si \
                                             zdarma tři nové ({n} z \
                                             {MAX_FREE_REROLLS})"
                                        ),
                                        crate::i18n::Lang::En => format!(
                                            "all three the game put up are \
                                             under 50 % - throwing them back \
                                             for three new ones, which costs \
                                             nothing ({n} of \
                                             {MAX_FREE_REROLLS})"
                                        ),
                                    },
                                );
                                let ask =
                                    Command::perform(async {}, move |()| {
                                        Message::ArenaOfferRefresh {
                                            ident,
                                            then_fight: true,
                                        }
                                    });
                                return Command::batch([refetch, ask]);
                            }
                            // The free re-rolls stay spent until the next
                            // fight (see below). Starting the count
                            // again here threw the trio back every thirty
                            // seconds for good - "1 of 2" in the live feed
                            // over and over, 28 October
                            si.offer_rerolls = MAX_FREE_REROLLS;
                            log::info!(
                                "{ident}: arena idle - nobody beatable in the \
                                 game's offer either"
                            );
                            set_auto_status(
                                account,
                                if same_again {
                                    "Nobody beatable, and the game put up the \
                                     same three again"
                                } else {
                                    "Nobody beatable: not in the crawled \
                                     list, not in the game's offer"
                                },
                            );
                            return refetch;
                        }
                    }
                }

                // A fresh target is better, but the free fight expires on its
                // own clock. Waiting for the list to be 90 % refreshed used
                // to throw those fights away, so a stale target is taken
                // rather than none - at the risk that the item is already
                // gone or the opponent has grown since
                let (target, stale) = match offer_target {
                    Some(t) => (t, false),
                    None => match si.best.iter().find(|a| !a.is_old()) {
                        Some(t) => (t.clone(), false),
                        None => match si.best.first() {
                            Some(t) => (t.clone(), true),
                            None => {
                                status.put_session(session);
                                drop(status);
                                set_auto_status(account, "Refreshing targets");
                                return refetch;
                            }
                        },
                    },
                };
                if stale && use_mush {
                    // A mushroom on a target whose item may already be gone
                    // is a mushroom wasted - hold it for a fresh one
                    status.put_session(session);
                    drop(status);
                    log::info!(
                        "{ident}: arena - no fresh target, not spending a \
                         mushroom"
                    );
                    set_auto_status(account, "Refreshing targets");
                    return refetch;
                }
                if stale {
                    log::info!(
                        "{ident}: arena - no fresh target out of {total_len}, \
                         taking the best known one anyway"
                    );
                }
                drop(status);
                if let Some(si) = &mut account.scrapbook_info {
                    // A fight, whoever it is against, starts a new run of
                    // free re-rolls
                    si.offer_rerolls = 0;
                    si.offer_seen_ids.clear();
                    si.last_auto_attack = Some(Local::now());
                    // Who is on the wire, and what the app believed of
                    // them - the result handler files the pair (see
                    // `fight_log`). A crawled target carries no simulated
                    // chance, so only the name is remembered
                    let predicted = si
                        .arena_offer
                        .iter()
                        .find(|o| o.name == target.info.name)
                        .map(|o| o.win);
                    si.pending_fight =
                        Some((target.info.name.clone(), predicted));
                    if use_mush {
                        si.mushroom_today = si.mushroom_today.saturating_add(1);
                        if let Some(k) = key.as_deref() {
                            let on_disk = self
                                .spend
                                .bump(
                                    k,
                                    today,
                                    crate::spend_ledger::Spend::ArenaMushroom,
                                )
                                .arena;
                            si.mushroom_today = si.mushroom_today.max(on_disk);
                        }
                    }
                }
                set_auto_status(
                    account,
                    format!(
                        "Attacking {} (+{})",
                        target.info.name, target.missing
                    ),
                );
                log::info!(
                    "{ident}: arena - attacking {} (+{} items)",
                    target.info.name, target.missing
                );

                if use_mush {
                    set_auto_status(
                        account,
                        format!(
                            "Mushroom attack on {} (+{})",
                            target.info.name, target.missing
                        ),
                    );
                }
                let tn = target.info.name.clone();
                let fight = Command::perform(
                    async move {
                        let cmd = sf_api::command::Command::Fight {
                            name: tn,
                            use_mushroom: use_mush,
                        };
                        let resp = session.send_command(&cmd).await;
                        (resp, session)
                    },
                    move |r| match r.0 {
                        Ok(resp) => Message::PlayerAttackResult {
                            ident,
                            session: r.1,
                            against: target,
                            resp: Box::new(resp),
                        },
                        Err(_) => Message::PlayerCommandFailed {
                            ident,
                            session: r.1,
                            attempt: 0,
                        },
                    },
                );

                return Command::batch([refetch, fight]);
            }
            Message::PlayerCommandFailed {
                ident,
                mut session,
                attempt,
            } => {
                let Some(server) = self.servers.0.get_mut(&ident.server_id)
                else {
                    return Command::none();
                };
                let Some(player) = server.accounts.get_mut(&ident.account)
                else {
                    return Command::none();
                };

                // STOP means no login, whatever failed: the player is in
                // the game themselves (Steam, browser) and every login
                // from here throws them out - it was a fight, fifteen
                // seconds a round. The session is parked for RESUME
                if self.paused {
                    player.parked_session = Some(session);
                    self.console(
                        Some(ident),
                        "STOP - the session is gone (logged in elsewhere); \
                         the app stays out of the game until RESUME"
                            .to_string(),
                        Some(false),
                    );
                    return Command::none();
                }

                let mut lock = player.status.lock_safe();
                *lock = AccountStatus::LoggingInAgain;
                drop(lock);
                if let Some(si) = &mut player.scrapbook_info {
                    si.auto_status = Some(format!(
                        "Connection lost - retry #{}",
                        attempt + 1
                    ));
                    // Whatever the offer fetch was doing died with the
                    // session; it is asked for again after the relogin
                    si.arena_offer_busy = false;
                }
                warn!("Logging in {ident} again (attempt {})", attempt + 1);
                if attempt == 0 {
                    self.console(
                        Some(ident),
                        "Connection to the game lost - logging in again. If \
                         you just logged in elsewhere, that is the cause: the \
                         game keeps one session per character"
                            .to_string(),
                        Some(false),
                    );
                }
                // Every failed attempt waits longer than the last one, so that
                // a server, that is down (or rate limiting us), does not get
                // hammered by every character at once
                let backoff = retry_delay(attempt);
                return Command::perform(
                    async move {
                        if attempt > 0 {
                            sleep(backoff).await;
                        }
                        let Ok(resp) = session.login().await else {
                            return Err(session);
                        };
                        let Ok(gamestate) = GameState::new(resp) else {
                            return Err(session);
                        };
                        Ok((Box::new(gamestate), session))
                    },
                    move |res| match res {
                        Ok((gs, session)) => {
                            Message::PlayerRelogSuccess { ident, gs, session }
                        }
                        Err(session) => Message::PlayerCommandFailed {
                            ident,
                            session,
                            attempt: attempt.saturating_add(1),
                        },
                    },
                );
            }
            Message::PlayerAttackResult {
                ident,
                session,
                against,
                resp,
            } => {
                let Some(server) = self.servers.0.get_mut(&ident.server_id)
                else {
                    return Command::none();
                };
                let Some(account) = server.accounts.get_mut(&ident.account)
                else {
                    return Command::none();
                };

                // The on-disk key for this character, taken before the
                // account is borrowed out (see `fight_log`)
                let fight_key = crate::probe_history::ProbeHistory::key(
                    &server.ident.url, &account.name,
                );
                let v = account.status.clone();
                let mut lock = v.lock_safe();

                let AccountStatus::Busy(s, _) = &mut *lock else {
                    // Nothing to put the session back into, so it would be
                    // lost either way
                    return Command::none();
                };

                if let Err(e) = s.update(*resp) {
                    // it would *probably* be ok to just ignore this in most
                    // cases, but whatever
                    *lock = AccountStatus::FatalError(e.to_string());
                    return Command::none();
                };

                let won = s.last_fight.as_ref().map(|a| a.has_player_won);
                // Whatever happens from here on, the character has to become
                // usable again. Not returning the session used to leave the
                // account stuck in "Fighting" until the next relog
                if let Some(si) = &mut account.scrapbook_info {
                    si.refresh(s);
                }
                lock.put_session(session);
                drop(lock);

                let Some(won) = won else {
                    warn!("Fight without a result for {ident}");
                    return Command::none();
                };

                let nt = against.info.name.clone();
                let ut = against.info.uid;
                let auto_off_after = self.config.auto_battle_max_losses;

                let Some(si) = &mut account.scrapbook_info else {
                    return Command::none();
                };

                if won {
                    for new in &against.info.equipment {
                        si.scrapbook.items.insert(*new);
                    }
                    si.losses_in_a_row = 0;
                } else {
                    si.losses_in_a_row += 1;
                    si.blacklist.entry(ut).or_insert((nt, 0)).1 += 1;
                }
                si.auto_status = None;
                // Any plan is based on the items we had before this fight
                si.plan.clear();
                // File what the simulation said against what happened. This
                // is the whole training loop: no model to fit, one tally per
                // confidence bucket and one per opponent, and the next
                // estimate is bent by both (see `fight_log`)
                if let Some((foe, predicted)) = si.pending_fight.take() {
                    match predicted {
                        Some(p) => self.fights.note(&fight_key, &foe, p, won),
                        None => self.fights.note_result(&fight_key, &foe, won),
                    }
                    self.fights.save();
                }
                let Some(si) = &mut account.scrapbook_info else {
                    return Command::none();
                };
                let line = format!(
                    "Arena: {} {}",
                    if won { "won against" } else { "lost against" },
                    against.info.name
                );
                si.attack_log.push((Local::now(), against, won));
                // Borrowed `server` is still needed below - push directly
                let who = titlecase::titlecase(&account.name);
                if won {
                    Self::push_success(
                        &mut self.successes,
                        &account.name,
                        line.clone(),
                    );
                }
                let dup =
                    crate::ConsoleLine::just_succeeded(&self.successes, &who);
                self.console.push_back(crate::ConsoleLine {
                    at: Local::now(),
                    who,
                    text: line,
                    ok: Some(won),
                    dup,
                });
                // The goblin's word on the fight (held to one line a while)
                Self::push_goblin(
                    &mut self.console,
                    &account.name,
                    if won {
                        crate::goblin::Mood::ArenaWon
                    } else {
                        crate::goblin::Mood::ArenaLost
                    },
                );

                // If we lose over and over again, our attribute limit is
                // simply set too high. Continuing would just burn free fights,
                // so we stop and let the user decide
                if auto_off_after > 0
                    && si.auto_battle
                    && si.losses_in_a_row >= auto_off_after
                {
                    si.auto_battle = false;
                    si.auto_status = Some(format!(
                        "Auto battle stopped after {} losses in a row. Lower                          the max attributes",
                        si.losses_in_a_row
                    ));
                    warn!(
                        "Auto battle disabled for {ident} after repeated losses"
                    );
                }

                let mut res = Command::none();
                if won && let CrawlingStatus::Crawling { .. } = &server.crawling
                {
                    let ident = account.ident;
                    res = self.update_best(ident, false);
                }
                // The game deals three new opponents after every fight, so
                // the panel showing the old three was stale the moment the
                // fight ended. A short pause first - the fight's own
                // requests just went out
                let offer = Command::perform(
                    async {
                        crate::pace::sleep_between(1500, 3000).await;
                    },
                    move |()| Message::ArenaOfferRefresh {
                        ident,
                        then_fight: false,
                    },
                );

                return Command::batch([res, offer]);
            }
            Message::AutoBattle { ident, state } => {
                let Some(server) = self.servers.0.get_mut(&ident.server_id)
                else {
                    return Command::none();
                };
                let Some(player) = server.accounts.get_mut(&ident.account)
                else {
                    return Command::none();
                };

                let Some(si) = &mut player.scrapbook_info else {
                    return Command::none();
                };

                si.auto_battle = state;
                // A fresh start should not immediately trip the loss limit of
                // the run before it
                si.losses_in_a_row = 0;
                si.auto_status = None;
            }
            Message::CrawlerStartup { server, state } => {
                let Some(server) = self.servers.get_mut(&server) else {
                    return Command::none();
                };

                let CrawlingStatus::Crawling {
                    crawling_session, ..
                } = &mut server.crawling
                else {
                    return Command::none();
                };
                *crawling_session = Some(state);
            }
            Message::CrawlerRevived { server_id } => {
                info!("Crawler revived");
                let Some(server) = self.servers.get_mut(&server_id) else {
                    return Command::none();
                };
                let CrawlingStatus::Crawling {
                    que,
                    recent_failures,
                    ..
                } = &mut server.crawling
                else {
                    return Command::none();
                };

                let mut que = que.lock_safe();

                let mut ok_pages = vec![];
                let mut ok_character = vec![];
                for action in recent_failures.drain(..) {
                    match action {
                        CrawlAction::Wait | CrawlAction::InitTodo => {}
                        CrawlAction::Page(page, que_id) => {
                            if que_id != que.que_id {
                                continue;
                            }
                            ok_pages.push(page);
                        }
                        CrawlAction::Character(name, que_id) => {
                            if que_id != que.que_id {
                                continue;
                            }
                            ok_character.push(name);
                        }
                    }
                }

                que.invalid_pages.retain(|a| !ok_pages.contains(a));
                que.invalid_accounts.retain(|a| !ok_character.contains(a));
                que.todo_accounts.append(&mut ok_character);
                que.todo_pages.append(&mut ok_pages);
            }
            Message::ViewOverview => {
                self.current_view = View::Overview {
                    selected: Default::default(),
                    action: Default::default(),
                };
            }
            Message::ChangeTheme(theme) => {
                self.config.theme = theme;
                _ = self.config.write();
            }
            Message::ViewSettings => {
                self.current_view = View::Settings;
            }
            Message::ShowSettingsTab(tab) => self.settings_tab = tab,
            Message::SSOLoginSuccess {
                name,
                pass,
                mut chars,
                remember,
                auto_login,
            } => {
                let ident = SSOIdent::SF(name.clone());

                let Some(res) = self
                    .login_state
                    .active_sso
                    .iter_mut()
                    .find(|a| a.ident == ident)
                else {
                    // Already logged in
                    return Command::none();
                };
                if remember {
                    self.config.accounts.retain(|a| match &a {
                        AccountConfig::Regular { .. } => true,
                        AccountConfig::SF { name: uuu, .. } => {
                            name.to_lowercase() != uuu.to_lowercase()
                        }
                    });

                    self.config.accounts.push(AccountConfig::SF {
                        name: name.clone(),
                        pw_hash: pass,
                        characters: chars
                            .iter()
                            .map(|a| SFAccCharacter {
                                config: CharacterConfig::default(),
                                ident: SFCharIdent {
                                    name: a.username().to_string(),
                                    server: a.server_url().as_str().to_string(),
                                },
                            })
                            .collect(),
                    });
                    _ = self.config.write();
                }

                if let Some(existing) = self.config.get_sso_accounts_mut(&name)
                {
                    let mut new: HashSet<(ServerIdent, String)> =
                        HashSet::new();
                    for char in &chars {
                        let name = char.username().trim().to_lowercase();
                        new.insert((
                            ServerIdent::new(char.server_url().as_str()),
                            name,
                        ));
                    }

                    let mut modified = false;

                    existing.retain(|a| {
                        let res = new.remove(&(
                            ServerIdent::new(&a.ident.server),
                            a.ident.name.trim().to_lowercase(),
                        ));
                        if !res {
                            modified = true;
                            info!("Removed a SSO char")
                        }
                        res
                    });

                    for (server, name) in new {
                        modified = true;
                        info!("Registered a a new SSO chars");
                        existing.push(SFAccCharacter {
                            config: CharacterConfig::default(),
                            ident: SFCharIdent {
                                name,
                                server: server.url.to_string(),
                            },
                        })
                    }

                    if modified {
                        _ = self.config.write();
                    }
                }

                self.login_state.import_que.append(&mut chars);

                res.status = SSOLoginStatus::Success;
                if auto_login {
                    for acc in &self.config.accounts {
                        let AccountConfig::SF {
                            name: s_name,
                            characters,
                            ..
                        } = acc
                        else {
                            continue;
                        };
                        if s_name != &name {
                            continue;
                        }
                        let mut commands = vec![];
                        for SFAccCharacter { ident, config } in characters {
                            if !config.login {
                                continue;
                            }
                            let ident = ident.clone();
                            commands
                                .push(Command::perform(async {}, move |_| {
                                    Message::SSOImportAuto { ident }
                                }))
                        }
                        return Command::batch(commands);
                    }
                }

                if self.current_view == View::Login
                    && self.login_state.login_typ == LoginType::SSOAccounts
                {
                    self.login_state.login_typ = LoginType::SSOChars;
                };
            }
            Message::SSOImport { pos } => {
                // TODO: Bounds check this?
                let account = self.login_state.import_que.remove(pos);
                return self.login(account, false, PlayerAuth::SSO, false);
            }
            Message::ViewSubPage { player, page } => {
                self.current_view = View::Account {
                    ident: player,
                    page,
                };
                // The arena's three opponents used to wait for the Refresh
                // button. Opening the page is the moment they are wanted
                if page == AccountPage::Scrapbook {
                    let stale = self
                        .servers
                        .get_ident(&player)
                        .and_then(|(_, a)| a.scrapbook_info.as_ref())
                        .is_some_and(|si| {
                            (!si.arena_offer_busy || si.offer_stuck())
                                && !si.offer_fresh()
                        });
                    if stale {
                        return Command::perform(async {}, move |()| {
                            Message::ArenaOfferRefresh {
                                ident: player,
                                then_fight: false,
                            }
                        });
                    }
                }
                // Opening the dungeon tab without any numbers in it would be
                // useless, so the simulation runs right away
                if page == AccountPage::Dungeons {
                    let needs_calc = self
                        .servers
                        .get_ident(&player)
                        .map(|(_, a)| a.dungeon_info.chances.is_empty())
                        .unwrap_or(false);
                    if needs_calc {
                        return Command::perform(async {}, move |()| {
                            Message::DungeonRefresh {
                                ident: player,
                                force: false,
                            }
                        });
                    }
                }
            }
            Message::SetAutoFetch(b) => {
                self.config.auto_fetch_newest = b;
                _ = self.config.write();
            }
            Message::SetMaxThreads(nv) => {
                self.config.max_threads = nv.clamp(0, 50);
                self.config.start_threads = self
                    .config
                    .start_threads
                    .clamp(0, 50.min(self.config.max_threads));
                _ = self.config.write();
            }
            Message::SetStartThreads(nv) => {
                self.config.start_threads =
                    nv.clamp(0, 50.min(self.config.max_threads));
                _ = self.config.write();
            }
            Message::SSOSuccess {
                auth_name,
                mut chars,
                provider,
            } => {
                let ident = match provider {
                    SSOProvider::Google => SSOIdent::Google(auth_name.clone()),
                    SSOProvider::Steam => SSOIdent::Steam(auth_name.clone()),
                };
                if self.login_state.active_sso.iter().any(|a| a.ident == ident)
                {
                    // Already logged in
                    return Command::none();
                };

                let new_sso = SSOLogin {
                    ident,
                    status: SSOLoginStatus::Success,
                };

                self.login_state.active_sso.push(new_sso);
                self.login_state.import_que.append(&mut chars);

                if self.current_view == View::Login
                    && self.login_state.login_typ == LoginType::Google
                    || self.login_state.login_typ == LoginType::Steam
                {
                    self.login_state.login_typ = LoginType::SSOChars;
                };
            }
            Message::SSORetry => {
                // The subscription will handle this
            }
            Message::SSOAuthError { .. } => {
                // TODO: Display this I guess
            }
            Message::OpenLink(url) => {
                _ = open::that(url);
            }
            Message::ConfigOpenFolder => {
                if let Some(dir) = crate::config::Config::path().parent() {
                    _ = open::that(dir);
                }
            }
            Message::ConfigRetry => {
                let loaded = match crate::config::Config::restore() {
                    Ok(Some((c, _))) => c,
                    Ok(None) => crate::config::Config::default(),
                    Err(e) => {
                        self.config_problem = Some(e);
                        return Command::none();
                    }
                };
                return self.config_recovered(loaded);
            }
            Message::ConfigStartFresh => {
                // The broken file stays, renamed - never deleted by the app
                match crate::storage::set_aside(&crate::config::Config::path())
                {
                    Ok(aside) => log::warn!(
                        "helper.toml set aside as {} on the user's choice",
                        aside.display()
                    ),
                    Err(e) => {
                        log::warn!("could not set helper.toml aside: {e}")
                    }
                }
                return self.config_recovered(crate::config::Config::default());
            }
            Message::PlayerAttack { ident, target } => {
                let Some(server) = self.servers.get_mut(&ident.server_id)
                else {
                    return Command::none();
                };
                let Some(account) = server.accounts.get_mut(&ident.account)
                else {
                    return Command::none();
                };
                let CrawlingStatus::Crawling { .. } = &server.crawling else {
                    return Command::none();
                };

                let mut status = account.status.lock_safe();
                let AccountStatus::Idle(_, gs) = &*status else {
                    return Command::none();
                };
                let next = gs.arena.next_free_fight.unwrap_or_default();
                if next > Local::now() + Duration::from_millis(200)
                    && gs.character.mushrooms == 0
                {
                    return Command::none();
                }

                let Some(mut session) = status.take_session("Fighting") else {
                    return Command::none();
                };
                drop(status);
                let ident = account.ident;
                let tn = target.info.name.clone();
                return Command::perform(
                    async move {
                        let cmd = sf_api::command::Command::Fight {
                            name: tn,
                            use_mushroom: false,
                        };
                        let resp = session.send_command(&cmd).await;
                        (resp, session)
                    },
                    move |r| match r.0 {
                        Ok(resp) => Message::PlayerAttackResult {
                            ident,
                            session: r.1,
                            against: target,
                            resp: Box::new(resp),
                        },
                        Err(_) => Message::PlayerCommandFailed {
                            ident,
                            session: r.1,
                            attempt: 0,
                        },
                    },
                );
            }
            Message::ArenaOfferRefresh { ident, then_fight } => {
                // Nothing is going out, so nothing may put up a flag that
                // says something is on its way. Raising it first and then
                // failing to send is what left the arena panel reading
                // "fetching..." with no request behind it
                if let Some(why) = self.halt_reason() {
                    let msg = format!(
                        "Arena: the three opponents were not fetched - \
                         nothing is being sent to the game ({why})"
                    );
                    if let Some(account) = self.servers.get_account_mut(&ident)
                        && let Some(si) = &mut account.scrapbook_info
                    {
                        si.arena_offer_busy = false;
                        si.arena_offer_started = None;
                    }
                    self.console(Some(ident), msg, Some(false));
                    return Command::none();
                }
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let Some(si) = &mut account.scrapbook_info else {
                    return Command::none();
                };
                if si.arena_offer_busy && !si.offer_stuck() {
                    return Command::none();
                }
                si.arena_offer_busy = true;
                si.arena_offer_started = Some(Local::now());
                return self.send_simple(
                    ident,
                    "Arena",
                    sf_api::command::Command::CheckArena,
                    SimpleNote::ArenaOffer {
                        step: 0,
                        then_fight,
                    },
                );
            }
            Message::ArenaFightOffer { ident, pid } => {
                // A fight against one of the game's own three, picked by
                // hand. No crawler needed - the game named them itself
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let Some(target) = account
                    .scrapbook_info
                    .as_ref()
                    .and_then(|si| si.arena_offer.iter().find(|o| o.pid == pid))
                    .map(crate::AttackTarget::from_offer)
                else {
                    return Command::none();
                };
                let mut status = account.status.lock_safe();
                let Some(mut session) = status.take_session("Fighting") else {
                    return Command::none();
                };
                drop(status);
                let predicted = account
                    .scrapbook_info
                    .as_ref()
                    .and_then(|si| si.arena_offer.iter().find(|o| o.pid == pid))
                    .map(|o| o.win);
                if let Some(si) = &mut account.scrapbook_info {
                    si.last_auto_attack = Some(Local::now());
                    si.auto_status = Some(format!(
                        "Attacking {} (the game's offer)",
                        target.info.name
                    ));
                    si.pending_fight =
                        Some((target.info.name.clone(), predicted));
                }
                let tn = target.info.name.clone();
                return Command::perform(
                    async move {
                        let cmd = sf_api::command::Command::Fight {
                            name: tn,
                            use_mushroom: false,
                        };
                        let resp = session.send_command(&cmd).await;
                        (resp, session)
                    },
                    move |r| match r.0 {
                        Ok(resp) => Message::PlayerAttackResult {
                            ident,
                            session: r.1,
                            against: target,
                            resp: Box::new(resp),
                        },
                        Err(_) => Message::PlayerCommandFailed {
                            ident,
                            session: r.1,
                            attempt: 0,
                        },
                    },
                );
            }
            Message::PlayerSetMaxLvl { ident, max } => {
                let Some(server) = self.servers.get_mut(&ident.server_id)
                else {
                    return Command::none();
                };
                let Some(account) = server.accounts.get_mut(&ident.account)
                else {
                    return Command::none();
                };
                let Some(si) = &mut account.scrapbook_info else {
                    return Command::none();
                };
                si.max_level = max;
                return self.update_best(ident, false);
            }
            Message::PlayerSetMaxAttributes { ident, max } => {
                let Some(server) = self.servers.get_mut(&ident.server_id)
                else {
                    return Command::none();
                };
                let Some(account) = server.accounts.get_mut(&ident.account)
                else {
                    return Command::none();
                };
                let Some(si) = &mut account.scrapbook_info else {
                    return Command::none();
                };
                si.max_attributes = max;
                return self.update_best(ident, false);
            }
            Message::SaveHoF(server_id) => {
                let Some(server) = self.servers.get(&server_id) else {
                    return Command::none();
                };

                let CrawlingStatus::Crawling {
                    que, player_info, ..
                } = &server.crawling
                else {
                    return Command::none();
                };

                let lock = que.lock_safe();
                let backup = lock.create_backup(player_info);
                drop(lock);
                let id = server.ident.id;
                let ident = server.ident.ident.to_string();

                return Command::perform(
                    async move { backup.write(&ident).await },
                    move |res| Message::BackupRes {
                        server: id,
                        error: res.err().map(|a| a.to_string()),
                    },
                );
            }
            Message::BackupRes {
                server: server_id,
                error,
            } => {
                // TODO: Display error?
                let Some(server) = self.servers.get_mut(&server_id) else {
                    return Command::none();
                };
                let Some(pb) = server.headless_progress.clone() else {
                    return Command::none();
                };
                if let Some(err) = error {
                    pb.println(err)
                }
                self.servers.0.remove(&server_id);
                pb.finish_and_clear();
                return Command::perform(async {}, |_| {
                    Message::NextCLICrawling
                });
            }
            Message::CopyBattleOrder { ident } => {
                let Some((server, account)) = self.servers.get_ident(&ident)
                else {
                    return Command::none();
                };

                let CrawlingStatus::Crawling {
                    player_info,
                    equipment,
                    que,
                    ..
                } = &server.crawling
                else {
                    return Command::none();
                };

                let Some(si) = &account.scrapbook_info else {
                    return Command::none();
                };

                let lock = que.lock_safe();
                let invalid =
                    lock.invalid_accounts.iter().map(|a| a.as_str()).collect();

                // If a plan was calculated already, we copy exactly what the
                // user is looking at instead of silently using another one
                let plan = if si.plan.is_empty() {
                    greedy_battle_plan(
                        player_info, equipment, si,
                        self.config.blacklist_threshold, &invalid,
                        MAX_PLAN_STEPS,
                    )
                } else {
                    si.plan.clone()
                };
                drop(lock);

                let target_list: Vec<_> =
                    plan.into_iter().map(|a| a.info.name).collect();
                return iced::clipboard::write(target_list.join("/"));
            }
            Message::PlayerComputePlan { ident } => {
                let Some((server, account)) = self.servers.get_ident(&ident)
                else {
                    return Command::none();
                };
                let CrawlingStatus::Crawling {
                    player_info,
                    equipment,
                    que,
                    ..
                } = &server.crawling
                else {
                    return Command::none();
                };
                let Some(si) = &account.scrapbook_info else {
                    return Command::none();
                };

                let lock = que.lock_safe();
                let invalid =
                    lock.invalid_accounts.iter().map(|a| a.as_str()).collect();
                let plan = greedy_battle_plan(
                    player_info, equipment, si,
                    self.config.blacklist_threshold, &invalid, MAX_PLAN_STEPS,
                );
                drop(lock);

                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                if let Some(si) = &mut account.scrapbook_info {
                    si.plan = plan;
                    si.show_plan = true;
                }
            }
            Message::PlayerShowPlan { ident, show } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let Some(si) = &mut account.scrapbook_info else {
                    return Command::none();
                };
                si.show_plan = show;
                if show && si.plan.is_empty() {
                    return Command::perform(async {}, move |()| {
                        Message::PlayerComputePlan { ident }
                    });
                }
            }
            Message::PlayerSetOrder { ident, order } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                if let Some(si) = &mut account.scrapbook_info {
                    si.order = order;
                    si.plan.clear();
                }
                return self.update_best(ident, false);
            }
            Message::PlayerSetFilter { ident, filter } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                if let Some(si) = &mut account.scrapbook_info {
                    si.filter = filter;
                }
            }
            Message::PlayerSkipTarget { ident, uid, name } => {
                let bl_threshold = self.config.blacklist_threshold.max(1);
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                if let Some(si) = &mut account.scrapbook_info {
                    si.blacklist.insert(uid, (name, bl_threshold));
                    si.plan.clear();
                }
                return self.update_best(ident, false);
            }
            Message::PlayerClearBlacklist { ident } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                if let Some(si) = &mut account.scrapbook_info {
                    si.blacklist.clear();
                    si.plan.clear();
                }
                return self.update_best(ident, false);
            }
            Message::AdminInput(s) => self.admin_input = s,
            Message::ToggleHelp => {
                self.config.show_help = !self.config.show_help;
                crate::ui::glyphs::set_help(self.config.show_help);
                _ = self.config.write();
            }
            Message::ToggleDrawnFont => {
                self.config.drawn_font = !self.config.drawn_font;
                crate::ui::glyphs::set_artwork(self.config.drawn_font);
                _ = self.config.write();
            }
            Message::LicenceInput(s) => self.licence_input = s,
            Message::LicenceApply => {
                let pasted = std::mem::take(&mut self.licence_input);
                let pasted = pasted.trim().to_string();
                if pasted.is_empty() {
                    return Command::none();
                }
                // Only a code that actually checks out replaces the stored
                // one. Pasting the wrong thing must not cost somebody the
                // licence they already had
                match crate::licence::verify(
                    &pasted,
                    &crate::licence::PUBLIC_KEY,
                ) {
                    Ok(l) => {
                        self.config.licence = pasted;
                        _ = self.config.write();
                        self.refresh_licence();
                        self.console(
                            None,
                            format!(
                                "licence {} accepted, for {}",
                                l.id, l.holder
                            ),
                            Some(true),
                        );
                    }
                    Err(e) => {
                        self.licence = crate::licence::Status::Bad(e.clone());
                        self.console(None, e.to_string(), Some(false));
                    }
                }
            }
            Message::LicenceCopyMachine => {
                return iced::clipboard::write(crate::licence::fingerprint());
            }
            Message::AdminUnlock => {
                if crate::admin::password_ok(&self.admin_input) {
                    self.config.admin_unlocked = true;
                    if self.config.announce_text.is_empty() {
                        self.config.announce_text =
                            crate::admin::DEFAULT_ANNOUNCEMENT.to_string();
                    }
                    _ = self.config.write();
                }
                self.admin_input.clear();
            }
            Message::AdminLock => {
                self.config.admin_unlocked = false;
                self.config.announce_enabled = false;
                self.config.greet_enabled = false;
                self.config.farewell_enabled = false;
                self.config.auto_kick = false;
                self.config.discord_daily = false;
                _ = self.config.write();
            }
            Message::GreetSetEnabled(v) => {
                self.config.greet_enabled = v;
                _ = self.config.write();
                // Whatever is in the chat right now happened before the
                // switch - the next refresh takes it as the starting point
                self.chat_baseline.clear();
            }
            Message::GreetSetText(v) => {
                self.config.greet_text = v;
                _ = self.config.write();
            }
            Message::FarewellSetEnabled(v) => {
                self.config.farewell_enabled = v;
                _ = self.config.write();
                self.chat_baseline.clear();
            }
            Message::FarewellSetText(v) => {
                self.config.farewell_text = v;
                _ = self.config.write();
            }
            Message::SayInGuildChat {
                ident,
                text,
                about,
                attempt,
                only_if_member,
            } => {
                if !self.config.admin_unlocked || text.trim().is_empty() {
                    return Command::none();
                }
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                // Still the owner's character, and free to talk. If it is in
                // the middle of something the line waits a little - it must
                // not be lost, the person is already marked as done
                if !crate::admin::is_owner(&account.name) {
                    return Command::none();
                }
                let (idle, still_there) = {
                    let lock = account.status.lock_safe();
                    let idle = matches!(&*lock, AccountStatus::Idle(..));
                    let still_there = match (&only_if_member, &*lock) {
                        (None, _) => true,
                        (
                            Some(who),
                            AccountStatus::Idle(_, gs)
                            | AccountStatus::Busy(gs, _),
                        ) => gs.guild.as_ref().is_some_and(|g| {
                            g.members
                                .iter()
                                .any(|m| m.name.to_lowercase() == *who)
                        }),
                        _ => true,
                    };
                    (idle, still_there)
                };
                if !still_there {
                    self.console(
                        Some(ident),
                        format!("{about} - and left again before the hello, nothing said"),
                        None,
                    );
                    return Command::none();
                }
                if !idle {
                    if attempt >= 10 {
                        log::warn!(
                            "{ident}: gave up on \"{text}\" ({about}) - the \
                             character never became free"
                        );
                        return Command::none();
                    }
                    return Command::perform(
                        async {
                            sleep(Duration::from_secs(20)).await;
                        },
                        move |()| Message::SayInGuildChat {
                            ident,
                            text,
                            about,
                            attempt: attempt + 1,
                            only_if_member,
                        },
                    );
                }
                self.console(
                    Some(ident),
                    format!("Guild chat, {about}: {text}"),
                    Some(true),
                );
                return self.send_simple(
                    ident,
                    "Chat",
                    sf_api::command::Command::GuildSendChat {
                        message: text.trim().to_string(),
                    },
                    SimpleNote::GuildChat,
                );
            }
            Message::AnnounceSetEnabled(v) => {
                self.config.announce_enabled = v;
                _ = self.config.write();
            }
            Message::AnnounceSetText(v) => {
                self.config.announce_text = v;
                _ = self.config.write();
            }
            Message::AnnounceSetHours(v) => {
                self.config.announce_hours = v.clamp(1, 48);
                _ = self.config.write();
            }
            Message::DiscordDailySet(v) => {
                self.config.discord_daily = v;
                _ = self.config.write();
            }
            Message::AnnounceTick => {
                if !self.config.admin_unlocked {
                    return Command::none();
                }
                // The daily invite: a time between 10:00 and midnight is
                // drawn once per day and remembered, so a restart does not
                // draw again; once it is past and today's post is not out
                // yet, every owner character posts it in its own guild
                if self.config.discord_daily {
                    let now = Local::now().naive_local();
                    let today = now.date();
                    let drawn = self
                        .config
                        .discord_daily_at
                        .filter(|t| t.date() == today);
                    let at = match drawn {
                        Some(t) => t,
                        None => {
                            let minute = fastrand::u32(0..(14 * 60));
                            let t = today
                                .and_hms_opt(10 + minute / 60, minute % 60, 0)
                                .unwrap_or(now);
                            self.config.discord_daily_at = Some(t);
                            _ = self.config.write();
                            log::info!("discord invite drawn for {t}");
                            t
                        }
                    };
                    let posted_today =
                        self.config.discord_daily_last == Some(today);
                    if now >= at && !posted_today {
                        // Only marked as posted once it really went out:
                        // the poster busy at the drawn minute used to cost
                        // the whole day's invite
                        if let Some(cmd) = self.post_discord_invite() {
                            self.config.discord_daily_last = Some(today);
                            _ = self.config.write();
                            // The periodic announcement, if due, gets the
                            // next tick - two lines in one breath would
                            // look odd
                            return cmd;
                        }
                    }
                }
                if !self.config.announce_enabled {
                    return Command::none();
                }
                let due = self.config.announce_last.is_none_or(|last| {
                    Local::now() - last
                        >= chrono::Duration::hours(i64::from(
                            self.config.announce_hours,
                        ))
                });
                if !due {
                    return Command::none();
                }
                return Command::perform(async {}, |()| Message::AnnounceNow);
            }
            Message::AnnounceNow => {
                let msg = self.config.announce_text.trim().to_string();
                self.config.announce_last = Some(Local::now());
                _ = self.config.write();
                return self.post_to_guild_chat(msg);
            }
            Message::AnnounceDiscord => {
                // Deliberately not touching `announce_last`: this is a one
                // off, not the periodic announcement, so sending it does not
                // push the next scheduled post an hour further away
                return self.post_to_guild_chat(
                    crate::admin::DISCORD_INVITE.to_string(),
                );
            }
            Message::ShopSetOption {
                ident,
                option,
                state,
            } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let name = account.name.clone();
                let si = &mut account.shop_info;
                match option {
                    ShopOption::ProtectEpics => si.protect_epics = state,
                    ShopOption::ProtectEnchanted => {
                        si.protect_enchanted = state
                    }
                    ShopOption::ProtectUpgraded => si.protect_upgraded = state,
                    ShopOption::SellOld => si.sell_old = state,
                    ShopOption::ClearBackpack => si.clear_backpack = state,
                    ShopOption::Mushrooms => si.mushrooms = state,
                    ShopOption::AttrAuto => si.attr_auto = state,
                    ShopOption::Smith => si.smith_auto = state,
                    ShopOption::Witch => si.witch_auto = state,
                }
                if let Some(conf) =
                    self.config.get_char_conf_mut(&name, ident.server_id)
                {
                    match option {
                        ShopOption::ProtectEpics => {
                            conf.shop_protect_epics = state
                        }
                        ShopOption::ProtectEnchanted => {
                            conf.shop_protect_enchanted = state
                        }
                        ShopOption::ProtectUpgraded => {
                            conf.shop_protect_upgraded = state
                        }
                        ShopOption::SellOld => conf.shop_sell_old = state,
                        ShopOption::ClearBackpack => {
                            conf.shop_clear_backpack = state
                        }
                        ShopOption::Mushrooms => conf.shop_mushrooms = state,
                        ShopOption::AttrAuto => conf.attr_auto = state,
                        ShopOption::Smith => conf.smith_auto = state,
                        ShopOption::Witch => conf.witch_auto = state,
                    }
                    _ = self.config.write();
                }
                if state
                    && matches!(
                        option,
                        ShopOption::ClearBackpack
                            | ShopOption::AttrAuto
                            | ShopOption::Smith
                            | ShopOption::Witch
                    )
                {
                    let job = if option != ShopOption::AttrAuto {
                        Job::Shopping
                    } else {
                        Job::Attributes
                    };
                    return Command::perform(async {}, move |()| {
                        Message::Enqueue { ident, job }
                    });
                }
            }
            Message::ShopSetNumber { ident, what, value } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let name = account.name.clone();
                let si = &mut account.shop_info;
                match what {
                    ShopNumber::MinGain => si.min_gain = value.min(100) as u8,
                    ShopNumber::GoldReserve => {
                        si.gold_reserve = value.min(100) as u8
                    }
                    ShopNumber::KeepFree => {
                        si.keep_free =
                            u8::try_from(value.clamp(1, 10)).unwrap_or(1);
                    }
                    ShopNumber::Rolls => {
                        si.rolls_per_day =
                            u8::try_from(value.min(20)).unwrap_or(3);
                    }
                    ShopNumber::WMain => si.attr_w_main = value.min(100) as u8,
                    ShopNumber::WCon => si.attr_w_con = value.min(100) as u8,
                    ShopNumber::WLuck => si.attr_w_luck = value.min(100) as u8,
                    ShopNumber::LegendarySwap => {
                        si.legendary_swap_pct = value.min(200) as u8
                    }
                }
                if let Some(conf) =
                    self.config.get_char_conf_mut(&name, ident.server_id)
                {
                    match what {
                        ShopNumber::MinGain => {
                            conf.shop_min_gain = value.min(100) as u8
                        }
                        ShopNumber::GoldReserve => {
                            conf.gold_reserve = value.min(100) as u8
                        }
                        ShopNumber::KeepFree => {
                            conf.shop_keep_free =
                                u8::try_from(value.clamp(1, 10)).unwrap_or(1);
                        }
                        ShopNumber::Rolls => {
                            conf.shop_rolls_per_day =
                                u8::try_from(value.min(20)).unwrap_or(3);
                        }
                        ShopNumber::WMain => {
                            conf.attr_w_main = value.min(100) as u8
                        }
                        ShopNumber::WCon => {
                            conf.attr_w_con = value.min(100) as u8
                        }
                        ShopNumber::WLuck => {
                            conf.attr_w_luck = value.min(100) as u8
                        }
                        ShopNumber::LegendarySwap => {
                            conf.legendary_swap_pct = value.min(200) as u8
                        }
                    }
                    _ = self.config.write();
                }
            }
            Message::ShopTimer { ident } => {
                if let Some(account) = self.servers.get_account_mut(&ident) {
                    account.shop_info.steps_this_round = 0;
                }
                return self.shop_step(ident);
            }
            Message::ShopTick { ident } => {
                // A continuation only, never a manual click: stop when the
                // automation was switched off in the meantime
                if self
                    .servers
                    .get_account_mut(&ident)
                    .is_some_and(|a| !account_job_enabled(a, Job::Shopping))
                {
                    return Command::none();
                }
                return self.shop_step(ident);
            }
            Message::AttrTimer { ident } => {
                if let Some(account) = self.servers.get_account_mut(&ident) {
                    account.shop_info.attr_bought_this_round = 0;
                }
                return self.attr_step(ident);
            }
            Message::AttrTick { ident } => {
                // A continuation only, never a manual click: stop when the
                // automation was switched off in the meantime
                if self
                    .servers
                    .get_account_mut(&ident)
                    .is_some_and(|a| !(a.shop_info.attr_auto))
                {
                    return Command::none();
                }
                return self.attr_step(ident);
            }
            Message::TogglePause => {
                self.paused = !self.paused;
                if self.paused {
                    // Everything that was waiting for its turn goes away.
                    // Otherwise pressing RESUME after an evening of playing
                    // would fire off a pile of stale jobs at once.
                    // The status lines go with it - they describe a moment
                    // that has passed, and leaving them on screen next to a
                    // ticking countdown looks like the bot is still working
                    for server in self.servers.0.values_mut() {
                        for account in server.accounts.values_mut() {
                            account.queue.clear();
                            account.expedition_info.steps_this_round = 0;
                            account.expedition_info.checks_in_a_row = 0;
                            account.expedition_info.status = None;
                            account.dailies_info.status = None;
                            account.guard_info.status = None;
                            account.dungeon_info.status = None;
                            account.idle_info.status = None;
                            account.shop_info.status = None;
                            account.shop_info.attr_status = None;
                            account.pet_info.status = None;
                            account.tavern_info.wheel_status = None;
                            account.social.status = None;
                            if let Some(fi) = &mut account.fortress_info {
                                fi.maintain_status = None;
                                fi.status = None;
                            }
                            if let Some(si) = &mut account.scrapbook_info {
                                si.auto_status = None;
                            }
                        }
                    }
                    self.console(
                        None,
                        "STOP - the app is not talking to the game any more. \
                         You can play in the browser; press RESUME when you \
                         are done"
                            .to_string(),
                        Some(true),
                    );
                } else {
                    // Two things have to happen, and neither does on its
                    // own. The session the app was holding is dead - the
                    // player's own login in the browser invalidated it - so
                    // every character has to log in again. And every checker
                    // sleeps before it fires, so without queueing the work
                    // here the app would sit still for minutes and look
                    // broken.
                    let now = Local::now();
                    let order = Job::order();
                    let mut logins: Vec<Command<Message>> = Vec::new();
                    for server in self.servers.0.values_mut() {
                        for account in server.accounts.values_mut() {
                            for job in &order {
                                if account_job_enabled(account, *job)
                                    && !account
                                        .queue
                                        .iter()
                                        .any(|(j, _)| j == job)
                                {
                                    account.queue.push((*job, now));
                                }
                            }
                            let ident = account.ident;
                            let arc = account.status.clone();
                            let mut lock = arc.lock_safe();
                            // Taking the session out and handing it to the
                            // re-login path is what the app already does when
                            // a command fails, so this is the same road, just
                            // without waiting for something to fail first.
                            // A session that died during STOP was parked
                            // instead of logged in - it goes the same way now
                            let session = lock
                                .take_session("Resume")
                                .or_else(|| account.parked_session.take());
                            if let Some(session) = session {
                                drop(lock);
                                logins.push(Command::perform(
                                    async {},
                                    move |()| Message::PlayerCommandFailed {
                                        ident,
                                        session,
                                        attempt: 0,
                                    },
                                ));
                            }
                        }
                    }
                    // **RESUME opens a session.** Queueing the work is
                    // not enough on its own: the queues only run inside a
                    // session, so with the next one drawn hours out the
                    // app sat there with a full queue and did nothing -
                    // which is exactly what "why does it take so long
                    // after RESUME" was. Pressing it is the player saying
                    // they have finished playing and the app can take
                    // over, so it takes over now. The quiet hours still
                    // hold; they are checked in `burst_active`
                    if !self.config.off() {
                        self.open_burst();
                        // ...and the characters come in over a couple of
                        // minutes, not over a quarter of a two-hour
                        // session. The stagger `open_burst` draws is sized
                        // for a session nobody is waiting on; this one has
                        // somebody sitting in front of it
                        let now = Local::now();
                        for server in self.servers.0.values_mut() {
                            for account in server.accounts.values_mut() {
                                account.not_before = Some(
                                    now + chrono::Duration::seconds(
                                        crate::stealth::stagger_secs(
                                            BOOST_STAGGER_SECS,
                                        ),
                                    ),
                                );
                            }
                        }
                    }
                    self.console(
                        None,
                        "RESUME - logging back in and picking the automation \
                         up again"
                            .to_string(),
                        Some(true),
                    );
                    if !logins.is_empty() {
                        return Command::batch(logins);
                    }
                }
            }
            Message::ToggleConsole => {
                self.console_open = !self.console_open;
            }
            Message::ClearConsole => {
                self.console.clear();
            }
            Message::ToggleFeedAll => {
                self.feed_all = !self.feed_all;
            }
            Message::ClearSuccesses => {
                self.successes.clear();
            }
            Message::SaveSettings => {
                _ = self.config.write();
                self.settings_saved_at = Some(Local::now());
            }
            Message::SetGradeStrictness(v) => {
                self.config.grade_strictness = v.min(100);
                _ = self.config.write();
            }
            Message::ToggleFollow => {
                self.config.follow_bot = !self.config.follow_bot;
                _ = self.config.write();
            }
            Message::TavernTab(tab) => self.tavern_tab = tab,
            Message::WindowResized { width, height } => {
                self.window_w = width as f32;
                self.window_h = height as f32;
            }
            Message::GuildMark { top, n } => {
                if top {
                    self.config.guild_top_mark = n;
                } else {
                    self.config.guild_bottom_mark = n;
                }
                _ = self.config.write();
            }
            Message::MoveJobTo { job, pos } => {
                let mut order = Job::order();
                if let Some(cur) = order.iter().position(|j| *j == job) {
                    let j = order.remove(cur);
                    order.insert(pos.min(order.len()), j);
                }
                self.config.job_order =
                    order.iter().map(|j| j.key().to_string()).collect();
                Job::set_order(&self.config.job_order);
                _ = self.config.write();
            }
            Message::ResetJobOrder => {
                self.config.job_order.clear();
                Job::set_order(&[]);
                _ = self.config.write();
            }
            Message::SetGuildFounded { ident, text } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let name = account.name.clone();
                account.social.guild_founded = text.clone();
                if let Some(conf) =
                    self.config.get_char_conf_mut(&name, ident.server_id)
                {
                    conf.guild_founded = text;
                    _ = self.config.write();
                }
            }
            Message::BurstNow => {
                self.open_burst();
            }
            Message::BoostToggle => {
                let now = Local::now();
                if self.boosting() {
                    self.end_boost();
                } else {
                    self.boost_until =
                        Some(now + chrono::Duration::minutes(BOOST_MINUTES));
                    self.boost_idle_since = None;
                    // Not "full speed": the pauses between clicks stay
                    // exactly as they are. What this removes is the
                    // *waiting* - the session gate, and the ten-minute
                    // hold on the sweep. Everything that is due goes into
                    // the queues at once and is then worked through at the
                    // pace of somebody sitting there doing it by hand,
                    // which is a quarter of an hour for the whole round
                    self.last_sweep = None;
                    // **The sessions stand down while this runs.** The
                    // first version opened one instead, which is two
                    // schedulers on the same queues: the session went on
                    // drawing its own length and its own stretch away
                    // underneath, and whatever was left of it kept
                    // running after the quarter of an hour was up. There
                    // is nothing being paced right now - the player asked
                    // for the lot - so the slider reads OFF for the
                    // duration and the next session is drawn fresh when
                    // this ends (`end_boost`)
                    self.burst_until = None;
                    self.burst_next = None;
                    self.session_started = None;
                    // Every *drawn* wait the app keeps for itself is
                    // dropped too, or "do it now" is only half a button:
                    // `next_due` is the fortress attack's own moment,
                    // drawn around the average the player set - with "3
                    // minutes" that can sit twelve minutes in the future,
                    // and a fortress full of soldiers with the app doing
                    // nothing is exactly what it looked like. The *game's*
                    // cooldowns are untouched: those are not ours to skip.
                    //
                    // **The stagger between characters is not one of those
                    // waits.** It is the whole reason two of them never
                    // click in the same second, which is the plainest sign
                    // of one person running several accounts there is.
                    // Clearing it sent both characters in together, so it
                    // is re-drawn over the first couple of minutes instead
                    // - they all still come in at once as far as the
                    // quarter of an hour is concerned, just not in lockstep
                    for server in self.servers.0.values_mut() {
                        for account in server.accounts.values_mut() {
                            account.not_before = Some(
                                now + chrono::Duration::seconds(
                                    crate::stealth::stagger_secs(
                                        BOOST_STAGGER_SECS,
                                    ),
                                ),
                            );
                            if let Some(fi) = &mut account.fortress_info {
                                fi.next_due = Some(now);
                            }
                        }
                    }
                    self.console(
                        None,
                        format!(
                            "Going through everything that is due - about \
                             {BOOST_MINUTES} min, at the usual pace. The \
                             sessions stand down until it is over"
                        ),
                        None,
                    );
                }
            }
            Message::SetOfficerSeats { ident, n } => {
                self.config.officer_seats =
                    n.clamp(1, crate::guild_report::OFFICER_SEATS_MAX);
                _ = self.config.write();
                if let Some(account) = self.servers.get_account_mut(&ident) {
                    account.social.status = Some(format!(
                        "Officer seats: {}",
                        self.config.officer_seats
                    ));
                }
            }
            Message::SetSafety(v) => {
                // The slider and "do it now" are two different things and
                // neither touches the other. The slider is how the app
                // plays on its own; the button is one round, now. Moving
                // the slider used to stop the run, because an earlier
                // version parked the slider at OFF while it ran and a drag
                // was read as taking the wheel back - the slider stopped
                // being parked, the stopping stayed, and the two fought:
                // adjust the slider for afterwards and the round you asked
                // for dies under your hand. The only thing that stops a
                // run is its own button
                self.config.safety = v.min(crate::stealth::MAX_LEVEL);
                // The pace, the sessions level and the length of a stretch
                // away all follow the slider - it is the one control now
                self.config.apply_safety();
                _ = self.config.write();
                if self.boosting() {
                    // The new position is what the schedule resumes from
                    // when the run is over; nothing to draw now
                    return Command::none();
                }
                if self.config.safety == 0 {
                    // The left stop is off: there is no session to wait for
                    self.burst_until = None;
                    self.burst_next = None;
                }
                // Moving off full power opens a session on its own -
                // `burst_active` does it the moment it finds no session
                // due. Opening one here meant a slider *dragged* across
                // the scale opened one per pixel: four "Session opened"
                // lines inside twenty seconds, each throwing away the
                // draw before it
            }
            Message::SetQuietHours(on) => {
                self.config.quiet_hours = on;
                _ = self.config.write();
            }
            Message::SetQuietFrom(h) => {
                self.config.quiet_from = h.min(23);
                _ = self.config.write();
            }
            Message::SetQuietTo(h) => {
                self.config.quiet_to = h.min(24);
                _ = self.config.write();
            }
            Message::SetRequestsPerMin(n) => {
                self.config.requests_per_min = n.clamp(3, 60);
                _ = self.config.write();
            }
            Message::SetProxy { index, sub, url } => {
                match self.config.accounts.get_mut(index) {
                    Some(crate::config::AccountConfig::Regular {
                        config,
                        ..
                    }) => config.proxy = url,
                    Some(crate::config::AccountConfig::SF {
                        characters,
                        ..
                    }) => {
                        if let Some(c) = sub.and_then(|s| characters.get_mut(s))
                        {
                            c.config.proxy = url;
                        }
                    }
                    None => {}
                }
                _ = self.config.write();
            }
            Message::SetLanguage(lang) => {
                self.config.lang = lang;
                crate::i18n::set_lang(lang);
                _ = self.config.write();
            }
            Message::SetTargetOrder(order) => {
                self.config.target_order = order;
                _ = self.config.write();
            }
            Message::SetAutoBattleMaxLosses(max) => {
                self.config.auto_battle_max_losses = max;
                _ = self.config.write();
            }
            Message::DungeonOnlyOpen { ident, only_open } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                account.dungeon_info.only_open = only_open;
            }
            Message::DungeonRefresh { ident, force } => {
                // The dungeon progress the login hands over can be stale (or
                // missing), so the dungeon screen is opened first - exactly
                // what the official client does before it lets you fight
                let (can_send, fresh) = {
                    let Some(account) = self.servers.get_account_mut(&ident)
                    else {
                        return Command::none();
                    };
                    // One minute: long enough to swallow the relogin burst
                    // (twelve refreshes in three seconds), short enough
                    // that the page and the fight see a current list
                    let fresh =
                        account.dungeon_info.refreshed_at.is_some_and(|t| {
                            Local::now() - t < chrono::Duration::seconds(60)
                        });
                    let arc = account.status.clone();
                    let lock = arc.lock_safe();
                    (matches!(&*lock, AccountStatus::Idle(..)), fresh)
                };
                // A recent answer is good enough unless something changed
                // it (a fight) or somebody asked by hand - and even then
                // never twice within five seconds: whatever fired a dozen
                // of these in three seconds gets one
                let just_now = self
                    .servers
                    .get_ident(&ident)
                    .and_then(|(_, a)| a.dungeon_info.refreshed_at)
                    .is_some_and(|t| {
                        Local::now() - t < chrono::Duration::seconds(5)
                    });
                if (fresh && !force) || just_now {
                    log::debug!(
                        "{ident}: dungeon refresh skipped (force={force}, \
                         fresh={fresh}, just_now={just_now})"
                    );
                    return Command::none();
                }
                log::info!(
                    "{ident}: dungeon refresh (force={force}, idle={can_send})"
                );
                if can_send {
                    if let Some(account) = self.servers.get_account_mut(&ident)
                    {
                        account.dungeon_info.refreshed_at = Some(Local::now());
                    }
                    return self.send_simple(
                        ident,
                        "Dungeons",
                        sf_api::command::Command::UpdateDungeons,
                        SimpleNote::DungeonsUpdated,
                    );
                }
                return Command::perform(async {}, move |()| {
                    Message::DungeonCompute { ident }
                });
            }
            Message::DungeonCompute { ident } => {
                let server_url = self
                    .servers
                    .get(&ident.server_id)
                    .map(|s| s.ident.url.clone())
                    .unwrap_or_default();
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let arc = account.status.clone();
                let lock = arc.lock_safe();
                let gs = match &*lock {
                    AccountStatus::Idle(_, gs) | AccountStatus::Busy(gs, _) => {
                        gs
                    }
                    _ => {
                        drop(lock);
                        account.dungeon_info.status =
                            Some("Character is not logged in".to_string());
                        return Command::none();
                    }
                };

                let start = std::time::Instant::now();
                let chances = crate::dungeon::compute_chances(
                    gs,
                    dungeon::SIM_ITERATIONS,
                );
                let took = start.elapsed().as_millis();
                drop(lock);

                account.dungeon_info.chances = chances;
                account.dungeon_info.computed = Some(Local::now());
                account.dungeon_info.sim_millis = took;
                account.dungeon_info.status = None;

                // Fill an empirical estimate into the fights the simulation
                // could not rate (the twister), from what past probes learned.
                // Without samples the odds stay unknown and the daily probe
                // will go and get the first one
                let key = crate::probe_history::ProbeHistory::key(
                    &server_url, &account.name,
                );
                for c in &mut account.dungeon_info.chances {
                    if !(c.no_data && c.open) {
                        continue;
                    }
                    let dname = format!("{:?}", c.dungeon);
                    // This exact floor first - it is the only exact
                    // answer there is. Then what past fights against
                    // enemies this far above or below the character say:
                    // a dungeon you walk through never fights the same
                    // floor twice, so the floor tally is empty precisely
                    // when it is wanted, and the column read "-" for ever
                    // while everything was going well
                    let exact = self
                        .probe_history
                        .get(&key, &dname, c.stage)
                        .filter(|t| t.attempts() > 0);
                    let from_gap = exact
                        .is_none()
                        .then(|| self.probe_history.by_gap(&key, c.level_gap))
                        .flatten();
                    if let Some(t) = exact.or(from_gap) {
                        c.win = t.estimate();
                        let base = c
                            .enemy
                            .strip_suffix(" (no data)")
                            .unwrap_or(&c.enemy)
                            .to_string();
                        c.enemy = if exact.is_some() {
                            format!(
                                "{base} (~{:.0}%, {}/{})",
                                c.win * 100.0,
                                t.wins,
                                t.attempts()
                            )
                        } else {
                            format!(
                                "{base} (~{:.0}%, from {} similar)",
                                c.win * 100.0,
                                t.attempts()
                            )
                        };
                    }
                }

                if account.dungeon_info.auto {
                    return self.dungeon_auto_fight(ident);
                }
            }
            Message::DungeonFight {
                ident,
                dungeon,
                name,
                attempt,
                mushroom,
            } => {
                let tower_lvl = {
                    let Some(account) = self.servers.get_account_mut(&ident)
                    else {
                        return Command::none();
                    };
                    let arc = account.status.clone();
                    let lock = arc.lock_safe();
                    match &*lock {
                        AccountStatus::Idle(_, gs)
                        | AccountStatus::Busy(gs, _) => {
                            crate::dungeon::tower_level(gs)
                        }
                        _ => None,
                    }
                };

                // The tower has its own command, everything else shares one
                let is_tower = dungeon
                    == SFDungeon::Light(
                        sf_api::gamestate::dungeons::LightDungeon::Tower,
                    );
                let (cmd, note) = match (is_tower, tower_lvl) {
                    (true, Some(finished)) => {
                        // 40 floors done means floor 41 is next. Should the
                        // server disagree, the failure handler retries with
                        // the other plausible numbers
                        let current_level = match attempt {
                            0 => finished.saturating_add(1),
                            1 => finished,
                            _ => finished.saturating_sub(1),
                        };
                        (
                            sf_api::command::Command::FightTower {
                                current_level,
                                use_mush: mushroom,
                            },
                            SimpleNote::Tower { attempt },
                        )
                    }
                    _ => (
                        sf_api::command::Command::FightDungeon {
                            dungeon,
                            use_mushroom: mushroom,
                        },
                        SimpleNote::Dungeon(name),
                    ),
                };

                return self.send_simple(ident, "Dungeon", cmd, note);
            }
            Message::GuildRefresh { ident } => {
                let guild_name = {
                    let Some(account) = self.servers.get_account_mut(&ident)
                    else {
                        return Command::none();
                    };
                    let arc = account.status.clone();
                    let lock = arc.lock_safe();
                    match &*lock {
                        AccountStatus::Idle(_, gs)
                        | AccountStatus::Busy(gs, _) => {
                            gs.guild.as_ref().map(|a| a.name.clone())
                        }
                        _ => None,
                    }
                };
                let Some(guild_name) = guild_name else {
                    let Some(account) = self.servers.get_account_mut(&ident)
                    else {
                        return Command::none();
                    };
                    account.social.status =
                        Some("This character is not in a guild".to_string());
                    // The guild refresh is the one tick a guildless
                    // character gets, so the finder hangs off it: a fresh
                    // list every half hour, and the join at the end of it
                    let due = account.social.finder_auto
                        && account.social.finder_at.is_none_or(|t| {
                            Local::now() - t > chrono::Duration::minutes(30)
                        });
                    if due {
                        return Command::perform(async {}, move |()| {
                            Message::GuildFinderRefresh { ident }
                        });
                    }
                    return Command::none();
                };

                // Reading the guild needs the character free for a moment.
                // With a full queue that moment is easy to miss, and missing
                // it used to mean the chat simply never arrived - so wait
                // for a gap instead of giving up on this tick
                let busy = match self.servers.get_account_mut(&ident) {
                    Some(account) => {
                        let arc = account.status.clone();
                        let idle = matches!(
                            &*arc.lock_safe(),
                            AccountStatus::Idle(..)
                        );
                        !idle
                    }
                    None => return Command::none(),
                };
                if busy {
                    let Some(account) = self.servers.get_account_mut(&ident)
                    else {
                        return Command::none();
                    };
                    account.social.chat_attempts =
                        account.social.chat_attempts.saturating_add(1);
                    if account.social.chat_attempts <= 20 {
                        return Command::perform(
                            async {
                                sleep(Duration::from_millis(1200)).await;
                            },
                            move |()| Message::GuildRefresh { ident },
                        );
                    }
                    account.social.chat_attempts = 0;
                    account.social.status = Some(
                        "The character has not been free long enough to read \
                         the guild - trying again shortly"
                            .to_string(),
                    );
                    return Command::none();
                }
                if let Some(account) = self.servers.get_account_mut(&ident) {
                    account.social.chat_attempts = 0;
                }

                return self.send_simple(
                    ident,
                    "Guild",
                    sf_api::command::Command::ViewGuild {
                        guild_ident: guild_name,
                    },
                    SimpleNote::Guild,
                );
            }
            Message::GuildMemberDetail { ident, name } => {
                if let Some(account) = self.servers.get_account_mut(&ident) {
                    account.social.selected_member = name;
                }
            }
            Message::GuildShowMembers { ident, show } => {
                if let Some(account) = self.servers.get_account_mut(&ident) {
                    account.social.show_chat = !show;
                }
            }
            Message::GuildDrawer { ident } => {
                if let Some(account) = self.servers.get_account_mut(&ident) {
                    account.social.drawer_open = !account.social.drawer_open;
                }
            }
            Message::GuildMemberSort { ident, sort } => {
                if let Some(account) = self.servers.get_account_mut(&ident) {
                    account.social.member_sort = sort;
                }
            }
            Message::GuildChatCopy { ident, text } => {
                if let Some(account) = self.servers.get_account_mut(&ident) {
                    account.social.status = Some(format!(
                        "Copied to the clipboard ({})",
                        Local::now().format("%H:%M:%S")
                    ));
                }
                return iced::clipboard::write(text);
            }
            Message::GuildReportCopy { ident, text } => {
                if let Some(account) = self.servers.get_account_mut(&ident) {
                    account.social.report_status = Some(format!(
                        "Copied to the clipboard ({})",
                        Local::now().format("%H:%M:%S")
                    ));
                }
                return iced::clipboard::write(text);
            }
            Message::GuildReportTick { ident } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                if !account.social.report_auto
                    || !account.social.report_queue.is_empty()
                {
                    return Command::none();
                }
                let name = account.name.clone();
                let hour = account.social.report_hour;
                let now = Local::now();
                let today = now.date_naive();
                let already = self
                    .config
                    .get_char_conf(&name, ident.server_id)
                    .and_then(|c| c.guild_report_last)
                    .is_some_and(|d| d >= today);
                if already
                    || u8::try_from(
                        now.format("%H")
                            .to_string()
                            .parse::<u32>()
                            .unwrap_or(0),
                    )
                    .unwrap_or(0)
                        < hour
                {
                    return Command::none();
                }
                return Command::perform(async {}, move |()| {
                    Message::GuildReportSend { ident }
                });
            }
            Message::GuildReportSend { ident } => {
                let server_url = self
                    .servers
                    .get(&ident.server_id)
                    .map(|s| s.ident.url.clone())
                    .unwrap_or_default();
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let own = account.name.to_lowercase();
                let (text, members) = {
                    let arc = account.status.clone();
                    let lock = arc.lock_safe();
                    let gs = match &*lock {
                        AccountStatus::Idle(_, gs)
                        | AccountStatus::Busy(gs, _) => gs,
                        _ => return Command::none(),
                    };
                    let Some(guild) = gs.guild.as_ref() else {
                        drop(lock);
                        account.social.report_status =
                            Some("Not in a guild".to_string());
                        return Command::none();
                    };
                    let key = crate::guild_history::GuildHistory::key(
                        &server_url, guild,
                    );
                    let text = crate::guild_report::build_report(
                        guild, &self.guild_history, &key,
                        self.config.grade_strictness,
                        self.config.officer_seats,
                    );
                    let members: Vec<String> = guild
                        .members
                        .iter()
                        .map(|m| m.name.clone())
                        .filter(|n| n.to_lowercase() != own)
                        .collect();
                    (text, members)
                };
                let name = account.name.clone();
                account.social.report_text = text;
                account.social.report_queue = members;
                account.social.report_sent = 0;
                account.social.report_failed = 0;
                account.social.report_status = Some(format!(
                    "Sending the report to {} members...",
                    account.social.report_queue.len()
                ));
                if let Some(conf) =
                    self.config.get_char_conf_mut(&name, ident.server_id)
                {
                    conf.guild_report_last = Some(Local::now().date_naive());
                    _ = self.config.write();
                }
                return Command::perform(async {}, move |()| {
                    Message::GuildReportNext { ident }
                });
            }
            Message::GuildReportNext { ident } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                if account.social.report_failed >= 5 {
                    account.social.report_queue.clear();
                    account.social.report_status = Some(
                        "Stopped: the server refused 5 mails in a row"
                            .to_string(),
                    );
                    return Command::none();
                }
                let Some(to) = account.social.report_queue.pop() else {
                    account.social.report_status = Some(format!(
                        "Report delivered to {} members ({} failed)",
                        account.social.report_sent,
                        account.social.report_failed
                    ));
                    return Command::none();
                };
                let msg = account.social.report_text.clone();
                return self.send_simple(
                    ident,
                    "Report",
                    sf_api::command::Command::SendMessage {
                        to: to.clone(),
                        msg,
                    },
                    SimpleNote::ReportSent(to),
                );
            }
            Message::GuildCompact { ident, state } => {
                if let Some(account) = self.servers.get_account_mut(&ident) {
                    account.social.compact_members = state;
                }
            }
            Message::GuildChatInput { ident, text } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                account.social.chat_input = text;
            }
            Message::GuildChatSend { ident } => {
                let msg = {
                    let Some(account) = self.servers.get_account_mut(&ident)
                    else {
                        return Command::none();
                    };
                    let msg = account.social.chat_input.trim().to_string();
                    if msg.is_empty() {
                        return Command::none();
                    }
                    account.social.chat_input.clear();
                    msg
                };
                return self.send_simple(
                    ident,
                    "Chat",
                    sf_api::command::Command::GuildSendChat { message: msg },
                    SimpleNote::GuildChat,
                );
            }
            Message::MailRefresh { ident } => {
                return self.send_simple(
                    ident,
                    "Mail",
                    sf_api::command::Command::Update,
                    SimpleNote::Mail,
                );
            }
            Message::MailReplyToInput { ident, text } => {
                if let Some(account) = self.servers.get_account_mut(&ident) {
                    account.social.reply_to = text;
                }
            }
            Message::MailReplyInput { ident, text } => {
                if let Some(account) = self.servers.get_account_mut(&ident) {
                    account.social.reply_text = text;
                }
            }
            Message::MailSend { ident } => {
                let (to, msg) = {
                    let Some(account) = self.servers.get_account_mut(&ident)
                    else {
                        return Command::none();
                    };
                    let to = account.social.reply_to.trim().to_string();
                    let msg = account.social.reply_text.trim().to_string();
                    if to.is_empty() || msg.is_empty() {
                        account.social.status =
                            Some("Recipient and text are both needed".into());
                        return Command::none();
                    }
                    (to, msg)
                };
                return self.send_simple(
                    ident,
                    "Mail",
                    sf_api::command::Command::SendMessage {
                        to: to.clone(),
                        msg,
                    },
                    SimpleNote::MailSent(to),
                );
            }
            Message::MailOpen { ident, pos, by_id } => {
                let msg_id = {
                    let Some(account) = self.servers.get_account_mut(&ident)
                    else {
                        return Command::none();
                    };
                    account.social.open_msg = Some(pos);
                    account.social.open_text = None;
                    let arc = account.status.clone();
                    let lock = arc.lock_safe();
                    match &*lock {
                        AccountStatus::Idle(_, gs)
                        | AccountStatus::Busy(gs, _) => {
                            gs.mail.inbox.get(pos as usize).map(|a| a.msg_id)
                        }
                        _ => None,
                    }
                };
                // The library sends `pos + 1`, so the id has to be handed in
                // off by one to end up on the wire unchanged
                let wire_pos = match (by_id, msg_id) {
                    (true, Some(id)) => id - 1,
                    _ => pos,
                };
                let by_id = by_id && msg_id.is_some();
                return self.send_simple(
                    ident,
                    "Mail",
                    sf_api::command::Command::MessageOpen { pos: wire_pos },
                    SimpleNote::MailOpen { pos, by_id },
                );
            }
            Message::SimpleResult {
                ident,
                session,
                resp,
                note,
            } => {
                // The guild is re-read every ~15 s to catch chat and activity.
                // That is on purpose, but one console line per read buries
                // everything else, so the routine guild refresh is kept silent
                // (a real change - a level up, a battle - still speaks for
                // itself elsewhere)
                // The arena offer is re-read after every fight now, four
                // requests each time - the panel shows when it was taken,
                // the console does not need to say so
                // Opening the dungeon list is bookkeeping too - the fight it
                // leads to is what gets a line
                // The expedition poll ("Checked the expedition") is a
                // screen refresh, not an action - it filled the Live
                // console forty lines deep
                let is_poll = matches!(&note, SimpleNote::Expedition(what) if what.starts_with("Checked"));
                // **The good news goes first.** `console()` marks a line
                // `dup` when the successes window already has the same
                // event, and it asks that question by looking at the
                // newest success - so writing the console line first meant
                // the answer was always about the *previous* event and
                // nothing was ever marked. Every arena-manager purchase
                // therefore appeared twice, and with the manager now
                // spending as fast as it earns, the Live feed was nothing
                // but purchases: the goblin's lines were still being
                // written, they were just buried within seconds
                if Self::is_success_note(&note) {
                    self.success(Some(ident), note.label());
                }
                if !is_poll
                    && !matches!(
                        note,
                        SimpleNote::Guild
                            | SimpleNote::ArenaOffer { .. }
                            | SimpleNote::DungeonsUpdated
                    )
                {
                    self.console(Some(ident), note.label(), Some(true));
                }
                let server_url = self
                    .servers
                    .get(&ident.server_id)
                    .map(|s| s.ident.url.clone())
                    .unwrap_or_default();
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let arc = account.status.clone();
                let mut lock = arc.lock_safe();
                let AccountStatus::Busy(gs, _) = &mut *lock else {
                    return Command::none();
                };
                // Which request carries the inbox vouchers is not written
                // down anywhere - the state kept coming up empty while the
                // game showed four Twitch drops. So the mail refresh says
                // what the server sent, until that is settled
                let pending_keys: Vec<String> =
                    if matches!(note, SimpleNote::Mail) {
                        resp.values()
                            .keys()
                            .filter(|k| k.contains("pending"))
                            .map(|k| (*k).to_string())
                            .collect()
                    } else {
                        Vec::new()
                    };
                // The finder list is not a game state key the library
                // knows ("Update ignored"), so it is read off the raw answer
                // here, before the answer is consumed
                let finder_list = if matches!(note, SimpleNote::GuildFinder) {
                    // The wire key is `joinablegrouplist.r`; the parser
                    // splits at the dot and files the `r` as the sub key,
                    // so the full name found nothing and the list came up
                    // empty three reads in a row
                    resp.values()
                        .get("joinablegrouplist")
                        .map(|v| crate::guild_finder::parse_list(v.as_str()))
                } else {
                    None
                };
                if let Err(e) = gs.update(*resp) {
                    *lock = AccountStatus::FatalError(e.to_string());
                    return Command::none();
                }
                if matches!(note, SimpleNote::Mail) {
                    log::info!(
                        "{ident}: mail refresh - pending keys in the answer: \
                         {pending_keys:?}, claimables in state now: {} ({})",
                        gs.mail.claimables.len(),
                        gs.mail
                            .claimables
                            .iter()
                            .map(|c| format!("{} {:?}", c.name, c.status))
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
                let won = gs.last_fight.as_ref().map(|a| a.has_player_won);
                // What a won fight paid, for the successes window. Read here,
                // because the next poll wipes `last_fight`
                let fight_gain = gs
                    .last_fight
                    .as_ref()
                    .map(|f| (f.xp_change, f.silver_change));
                let gain_text = |gain: Option<(u64, i64)>| -> String {
                    match gain {
                        Some((xp, silver)) if xp != 0 || silver != 0 => {
                            let mut parts = Vec::new();
                            if xp != 0 {
                                parts.push(format!("+{xp} XP"));
                            }
                            if silver != 0 {
                                parts.push(format!("{:+} gold", silver / 100));
                            }
                            format!(" ({})", parts.join(", "))
                        }
                        _ => String::new(),
                    }
                };

                // A mail is either plain text, or a fight report. The fight
                // lives in `last_fight`, which every poll wipes, so whatever
                // we want to show has to be copied out right now
                let mail_text = if let SimpleNote::MailOpen { .. } = &note {
                    let text = gs.mail.open_msg.take();
                    let fight = gs.last_fight.as_ref().map(|f| {
                        let mut out = String::new();
                        let name = |a: &Option<sf_api::gamestate::arena::Fighter>| {
                            a.as_ref()
                                .and_then(|a| a.name.clone())
                                .map(|a| titlecase::titlecase(&a))
                                .unwrap_or_else(|| "?".to_string())
                        };
                        for (i, single) in f.fights.iter().enumerate() {
                            if i > 0 {
                                out.push('\n');
                            }
                            out.push_str(&format!(
                                "{} vs {}",
                                name(&single.fighter_a),
                                name(&single.fighter_b)
                            ));
                        }
                        out.push_str(&format!(
                            "\n\nResult: {}\nSilver: {:+}\nXP: +{}\nHonor: {:+}",
                            if f.has_player_won { "won" } else { "lost" },
                            f.silver_change,
                            f.xp_change,
                            f.honor_change,
                        ));
                        if f.mushroom_change > 0 {
                            out.push_str(&format!(
                                "\nMushrooms: +{}",
                                f.mushroom_change
                            ));
                        }
                        format!("Fight report\n\n{out}")
                    });
                    Some(text.or(fight).unwrap_or_else(|| {
                        "This message has no readable text (the game sent \
                         nothing to display for it)"
                            .to_string()
                    }))
                } else {
                    None
                };

                lock.put_session(session);
                drop(lock);

                let note = match note {
                    SimpleNote::Tower { .. } => {
                        SimpleNote::Dungeon("Tower".to_string())
                    }
                    SimpleNote::PetDungeon { name, .. } => {
                        SimpleNote::Dungeon(name)
                    }
                    other => other,
                };
                match note {
                    SimpleNote::Dungeon(name) => {
                        if won == Some(true) {
                            Self::push_success(
                                &mut self.successes,
                                &account.name,
                                format!(
                                    "Dungeon {name}: won{}",
                                    gain_text(fight_gain)
                                ),
                            );
                        }
                        let (msg, ok) = match won {
                            Some(true) => (format!("{name}: won"), true),
                            Some(false) => (format!("{name}: lost"), false),
                            None => (
                                format!(
                                    "{name}: the server did not report a \
                                     fight (timer not up yet?)"
                                ),
                                false,
                            ),
                        };
                        account.dungeon_info.status = Some(msg.clone());
                        account.dungeon_info.log.push((Local::now(), msg, ok));
                        // The goblin's word on the fight
                        match won {
                            Some(true) => Self::push_goblin(
                                &mut self.console,
                                &account.name,
                                crate::goblin::Mood::DungeonWon,
                            ),
                            Some(false) => Self::push_goblin(
                                &mut self.console,
                                &account.name,
                                crate::goblin::Mood::DungeonLost,
                            ),
                            None => {}
                        }
                        // If this was a fight on an enemy we can not simulate
                        // (the twister), remember how it went for the floor we
                        // fought, so its estimate sharpens. Only a real result
                        // counts - a "timer not up" non-fight teaches nothing
                        let probed = account.dungeon_info.pending_probe.take();
                        let char_name = account.name.clone();
                        if let (Some((dungeon, floor, gap)), Some(w)) =
                            (probed, won)
                        {
                            let key = crate::probe_history::ProbeHistory::key(
                                &server_url, &char_name,
                            );
                            let dname = format!("{dungeon:?}");
                            self.probe_history
                                .record(&key, &dname, floor, gap, w);
                            self.probe_history.save();
                        }
                        // The next enemy is a different one, so the old
                        // chances are worthless
                        return Command::perform(async {}, move |()| {
                            Message::DungeonRefresh { ident, force: true }
                        });
                    }
                    SimpleNote::DungeonsUpdated => {
                        return Command::perform(async {}, move |()| {
                            Message::DungeonCompute { ident }
                        });
                    }
                    // Mapped to `Dungeon("Tower")` above
                    SimpleNote::Tower { .. }
                    | SimpleNote::PetDungeon { .. } => {}
                    SimpleNote::Guild => {
                        account.social.status = Some(format!(
                            "Chat refreshed {}",
                            Local::now().format("%H:%M:%S")
                        ));
                        // One diary entry per refresh: levels + sign-ups
                        let owner = self.config.admin_unlocked
                            && crate::admin::is_owner(&account.name);
                        let greet_on = owner && self.config.greet_enabled;
                        let farewell_on = owner && self.config.farewell_enabled;
                        let own_name = account.name.to_lowercase();
                        // (what for the console, the line itself, the member
                        // who still has to be there when it goes out)
                        let mut to_say: Vec<(String, String, Option<String>)> =
                            Vec::new();
                        // Who the auto-kick removes on this refresh, if anyone
                        let mut kick_target: Option<String> = None;
                        let raid_settled = {
                            let arc = account.status.clone();
                            let lock = arc.lock_safe();
                            let mut settled = None;
                            if let AccountStatus::Idle(_, gs)
                            | AccountStatus::Busy(gs, _) = &*lock
                                && let Some(guild) = gs.guild.as_ref()
                            {
                                let key =
                                    crate::guild_history::GuildHistory::key(
                                        &server_url, guild,
                                    );
                                let now = Local::now();
                                let today = now.date_naive();
                                // Who is in the list right now, lowercase ->
                                // as the game spells it
                                let present: std::collections::HashMap<
                                    String,
                                    String,
                                > = guild
                                    .members
                                    .iter()
                                    .map(|m| {
                                        (m.name.to_lowercase(), m.name.clone())
                                    })
                                    .collect();
                                // The first look after the switch (or the
                                // start) only takes stock: every join and
                                // leave line already in the chat, and
                                // everybody already on the list, is old
                                // news. Only what appears from now on gets
                                // a line in the chat
                                if (greet_on || farewell_on)
                                    && !self.chat_baseline.contains(&key)
                                {
                                    for m in &guild.chat {
                                        let line =
                                            format!("{} {}", m.user, m.message);
                                        if let Some(n) =
                                            crate::guild_history::joined_name(
                                                &line,
                                            )
                                        {
                                            self.guild_history
                                                .mark_greeted(&key, &n, now);
                                        }
                                        if let Some(n) =
                                            crate::guild_history::left_name(
                                                &line,
                                            )
                                        {
                                            self.guild_history
                                                .mark_farewelled(&key, &n, now);
                                        }
                                    }
                                    for n in present.keys() {
                                        self.guild_history
                                            .mark_greeted(&key, n, now);
                                    }
                                    self.chat_baseline.insert(key.clone());
                                    log::info!(
                                        "{ident}: chat baseline taken - {} \
                                         members, nobody is greeted for \
                                         what is already there",
                                        present.len()
                                    );
                                }
                                // Somebody who left: the chat's "X left the
                                // guild" line, for a name the diary saw
                                // yesterday or today and that is gone from
                                // the list. Read BEFORE today's diary entry
                                // is written, so "last seen" still means the
                                // last refresh. Older names are old lines -
                                // the chat keeps days - marked, never said
                                if farewell_on {
                                    let gone: Vec<String> = guild
                                        .chat
                                        .iter()
                                        .filter_map(|m| {
                                            crate::guild_history::left_name(
                                                &format!(
                                                    "{} {}",
                                                    m.user, m.message
                                                ),
                                            )
                                        })
                                        .map(|n| n.to_lowercase())
                                        .filter(|n| {
                                            !present.contains_key(n)
                                                && *n != own_name
                                                && !self
                                                    .guild_history
                                                    .is_farewelled(&key, n)
                                        })
                                        .collect();
                                    for lower in gone {
                                        let recent = self
                                            .guild_history
                                            .last_seen(&key, &lower)
                                            .is_some_and(|d| {
                                                (today - d).num_days() <= 1
                                            });
                                        self.guild_history
                                            .mark_farewelled(&key, &lower, now);
                                        if recent {
                                            to_say.push((
                                                format!("{lower} left"),
                                                self.config
                                                    .farewell_text
                                                    .clone(),
                                                None,
                                            ));
                                        }
                                    }
                                }
                                self.guild_history.record(&key, guild, now);
                                settled = self
                                    .guild_history
                                    .record_raid(&key, guild, now);
                                // The chat archive: whatever is new since
                                // the last look goes into the file
                                if self.config.chat_archive {
                                    let seen = self
                                        .chat_archived
                                        .entry(key.clone())
                                        .or_default();
                                    let lines = crate::archive::new_lines(
                                        seen,
                                        &guild.chat,
                                        now,
                                    );
                                    let path =
                                        crate::archive::chat_file(&guild.name);
                                    match crate::archive::append(&path, &lines)
                                    {
                                        Ok(()) => {
                                            self.chat_archive_note =
                                                Some(path.display().to_string());
                                            if account.social.show_archive {
                                                account
                                                    .social
                                                    .archive_lines
                                                    .extend(lines.iter().cloned());
                                            }
                                        }
                                        Err(e) => {
                                            self.chat_archive_note = Some(
                                                format!(
                                                    "{}: {e}",
                                                    path.display()
                                                ),
                                            );
                                            log::warn!(
                                                "chat archive {}: {e}",
                                                path.display()
                                            );
                                        }
                                    }
                                }
                                // Somebody new gets a hello - marked right
                                // here, before the delay, so a second
                                // refresh (or a second owner character in
                                // the guild) can not pick them up again.
                                // Fastest signal: the game's own "X joined
                                // the guild" line in the chat; the member
                                // list diary is the fallback for a line the
                                // chat parser did not keep. Somebody the
                                // diary knew from an earlier day is an old
                                // line - marked, never greeted
                                if greet_on {
                                    let mut candidates: Vec<String> = guild
                                        .chat
                                        .iter()
                                        .filter_map(|m| {
                                            crate::guild_history::joined_name(
                                                &format!(
                                                    "{} {}",
                                                    m.user, m.message
                                                ),
                                            )
                                        })
                                        .map(|n| n.to_lowercase())
                                        .filter(|n| {
                                            !self
                                                .guild_history
                                                .is_greeted(&key, n)
                                        })
                                        .collect();
                                    candidates.retain(|n| {
                                        let stale = self
                                            .guild_history
                                            .first_seen(&key, n)
                                            .is_some_and(|d| d < today);
                                        if stale {
                                            self.guild_history
                                                .mark_greeted(&key, n, now);
                                        }
                                        !stale
                                    });
                                    candidates.extend(
                                        self.guild_history.newcomers(&key, now),
                                    );
                                    candidates.sort();
                                    candidates.dedup();
                                    for lower in candidates {
                                        if lower == own_name {
                                            continue;
                                        }
                                        if let Some(name) = present.get(&lower)
                                        {
                                            self.guild_history.mark_greeted(
                                                &key, &lower, now,
                                            );
                                            to_say.push((
                                                format!("{name} joined"),
                                                self.config
                                                    .greet_text
                                                    .replace("{name}", name),
                                                Some(lower.clone()),
                                            ));
                                        }
                                    }
                                }
                                self.guild_history.save();
                                // The auto-kick: one member per refresh, the
                                // one the grading marks 5 and would advise to
                                // kick - never an officer, never a newcomer
                                // (the verdict already spares both), never
                                // ourselves. Only from a character that may
                                // kick at all
                                let may_kick = owner
                                    && self.config.auto_kick
                                    && guild.members.iter().any(|m| {
                                        m.name.to_lowercase() == own_name
                                            && matches!(
                                                m.guild_rank,
                                                sf_api::gamestate::guild::GuildRank::Leader
                                                    | sf_api::gamestate::guild::GuildRank::Officer
                                            )
                                    });
                                if may_kick {
                                    let stats =
                                        crate::guild_report::member_stats(
                                            &guild.members,
                                            guild.attacking.is_some(),
                                            guild.defending.is_some(),
                                            &self.guild_history,
                                            &key,
                                            self.config.grade_strictness,
                                            self.config.officer_seats,
                                        );
                                    kick_target = stats
                                        .iter()
                                        .find(|s| {
                                            s.verdict == crate::guild_report::Verdict::Kick
                                                && s.member.name.to_lowercase() != own_name
                                        })
                                        .map(|s| s.member.name.clone());
                                }
                            }
                            settled
                        };
                        let mut greets = Vec::new();
                        if let Some(name) = kick_target {
                            self.console.push_back(crate::ConsoleLine {
                                at: Local::now(),
                                who: titlecase::titlecase(&account.name),
                                text: format!(
                                    "Auto-kick: {name} is graded 5 - removing \
                                     from the guild"
                                ),
                                ok: Some(false),
                                dup: false,
                            });
                            log::info!("{ident}: auto-kick {name}");
                            greets.push(Command::perform(
                                async {
                                    crate::pace::sleep_between(2000, 4000)
                                        .await;
                                },
                                move |()| Message::GuildKick { ident, name },
                            ));
                        }
                        for (about, text, only_if_member) in to_say {
                            // A person types it a minute or two after
                            // noticing, not the same second
                            let delay = fastrand::u64(60..=120);
                            // Straight into the feed - `self.console()`
                            // would need `self` while the account is out
                            self.console.push_back(crate::ConsoleLine {
                                at: Local::now(),
                                who: titlecase::titlecase(&account.name),
                                text: format!(
                                    "{about} the guild - saying \"{text}\" in \
                                     {delay} s"
                                ),
                                ok: None,
                                dup: false,
                            });
                            log::info!(
                                "{ident}: {about} - \"{text}\" in {delay} s"
                            );
                            greets.push(Command::perform(
                                async move {
                                    sleep(Duration::from_secs(delay)).await;
                                },
                                move |()| Message::SayInGuildChat {
                                    ident,
                                    text,
                                    about,
                                    attempt: 0,
                                    only_if_member,
                                },
                            ));
                        }
                        // The one refresh that settles a raid says how it
                        // went; a win is good news for the successes window
                        if let Some((number, true)) = raid_settled {
                            Self::push_success(
                                &mut self.successes,
                                &account.name,
                                format!("Guild raid #{number} won"),
                            );
                        }
                        // Fresh member data - the right moment to sign up
                        let sign = self.guild_auto_sign(ident);
                        let lookup = Command::perform(
                            async {
                                sleep(Duration::from_secs(4)).await;
                            },
                            move |()| Message::GuildLookupEnemies { ident },
                        );
                        greets.push(sign);
                        greets.push(lookup);
                        return Command::batch(greets);
                    }
                    SimpleNote::Sacrifice { pos, gem_out, what } => {
                        account.shop_info.status =
                            Some(format!("Sacrifice: {what}"));
                        account.shop_info.log.push((
                            Local::now(),
                            format!("Sacrifice: {what}"),
                            true,
                        ));
                        if gem_out {
                            return Command::perform(
                                async {
                                    crate::pace::sleep_between(800, 1500)
                                        .await;
                                },
                                move |()| Message::SacrificeNext { ident, pos },
                            );
                        }
                        return Command::none();
                    }
                    SimpleNote::GuildAction(what) => {
                        account.social.status = Some(what);
                        return Command::none();
                    }
                    SimpleNote::GuildPet(what) => {
                        account.social.status = Some(what.clone());
                        return Command::perform(
                            async {
                                crate::pace::sleep_between(1500, 3000).await;
                            },
                            move |()| Message::GuildRefresh { ident },
                        );
                    }
                    SimpleNote::GuildSigned(what) => {
                        account.social.status = Some(format!("Joined {what}"));
                        // Re-read the guild, so that the second battle (if
                        // any) gets handled and the UI shows the new state
                        return Command::perform(
                            async {
                                sleep(Duration::from_secs(3)).await;
                            },
                            move |()| Message::GuildRefresh { ident },
                        );
                    }
                    SimpleNote::ArenaOffer { step, then_fight } => {
                        // CheckArena named three player ids. Look at each we
                        // do not already know, one request at a time; once
                        // all three are known, the offer is built from them
                        let (missing, offer) = {
                            let arc = account.status.clone();
                            let lock = arc.lock_safe();
                            match &*lock {
                                AccountStatus::Idle(_, gs)
                                | AccountStatus::Busy(gs, _) => {
                                    let ids = gs.arena.enemy_ids;
                                    let missing =
                                        ids.iter().copied().find(|&pid| {
                                            pid != 0
                                                && gs
                                                    .lookup
                                                    .lookup_pid(pid)
                                                    .is_none()
                                        });
                                    // Our own fighter once; the simulation
                                    // runs a thousand fights per opponent
                                    let us = sf_api::simulate::Fighter::from(
                                        &sf_api::simulate::PlayerFighterSquad::new(gs)
                                            .character,
                                    );
                                    let offer: Vec<crate::player::ArenaOpponent> =
                                        ids.iter()
                                            .filter_map(|&pid| gs.lookup.lookup_pid(pid))
                                            .map(|o| {
                                                crate::player::ArenaOpponent::from_other(o, &us)
                                            })
                                            .collect();
                                    (missing, offer)
                                }
                                _ => (None, Vec::new()),
                            }
                        };
                        let Some(si) = &mut account.scrapbook_info else {
                            return Command::none();
                        };
                        match missing {
                            Some(pid) if step < 4 => {
                                let next = SimpleNote::ArenaOffer {
                                    step: step + 1,
                                    then_fight,
                                };
                                return self.send_simple(
                                    ident,
                                    "Arena",
                                    sf_api::command::Command::ViewPlayer {
                                        ident: pid.to_string(),
                                    },
                                    next,
                                );
                            }
                            _ => {
                                si.arena_offer = offer;
                                si.arena_offer_at = Some(Local::now());
                                si.arena_offer_busy = false;
                                // Only when the arena job asked for it
                                // because it had nobody - now it has, so go
                                // again. The refresh after a fight and the
                                // button must not start a fight of their own
                                if si.auto_battle && then_fight {
                                    return Command::perform(
                                        async {
                                            crate::pace::sleep_between(
                                                1500, 3000,
                                            )
                                            .await;
                                        },
                                        move |()| Message::AutoBattlePossible {
                                            ident,
                                        },
                                    );
                                }
                            }
                        }
                    }
                    SimpleNote::GuildFinder => {
                        let list = finder_list.unwrap_or_default();
                        let n = list.len();
                        account.social.finder_pending =
                            crate::guild_finder::worth_a_look(
                                &list, &account.social.finder_bonus,
                            );
                        account.social.finder = list;
                        account.social.finder_status = Some(format!(
                            "{n} guilds in the finder - looking up their \
                             bonuses"
                        ));
                        return Command::perform(
                            async {
                                sleep(Duration::from_secs(2)).await;
                            },
                            move |()| Message::GuildFinderStep { ident },
                        );
                    }
                    SimpleNote::GuildFinderLook(name) => {
                        // The other guild's members carry their treasure
                        // and instructor levels; the sums are the bonuses
                        let bonus = {
                            let arc = account.status.clone();
                            let lock = arc.lock_safe();
                            match &*lock {
                                AccountStatus::Idle(_, gs)
                                | AccountStatus::Busy(gs, _) => gs
                                    .lookup
                                    .guilds
                                    .values()
                                    .find(|g| {
                                        g.name.eq_ignore_ascii_case(&name)
                                    })
                                    .map(|g| crate::guild_finder::Bonus {
                                        treasure: g
                                            .members
                                            .iter()
                                            .map(|m| m.treasure_lvl)
                                            .sum(),
                                        instructor: g
                                            .members
                                            .iter()
                                            .map(|m| m.instructor_lvl)
                                            .sum(),
                                    }),
                                _ => None,
                            }
                        };
                        if let Some(b) = bonus {
                            account
                                .social
                                .finder_bonus
                                .insert(name.to_lowercase(), b);
                        }
                        return Command::perform(
                            async {
                                sleep(Duration::from_secs(2)).await;
                            },
                            move |()| Message::GuildFinderStep { ident },
                        );
                    }
                    SimpleNote::GuildJoinLook(name) => {
                        account.social.finder_status =
                            Some(format!("Asking {name} to let us in..."));
                        return self.guild_join_request(ident, name, false);
                    }
                    SimpleNote::GuildJoin { name, .. } => {
                        // An open guild takes the character in at once; a
                        // closed one holds the request for its leader. The
                        // answer does not say which, the next guild refresh
                        // will
                        account.social.finder_status = Some(format!(
                            "Asked {name} to let us in - an open guild takes \
                             you in at once, otherwise the leader decides"
                        ));
                        Self::push_success(
                            &mut self.successes,
                            &account.name,
                            format!("Guild finder: asked {name} to let us in"),
                        );
                        return Command::perform(
                            async {
                                sleep(Duration::from_secs(3)).await;
                            },
                            move |()| Message::GuildRefresh { ident },
                        );
                    }
                    SimpleNote::GuildRaid => {
                        account.social.status =
                            Some("Raid started".to_string());
                        return Command::perform(
                            async {
                                sleep(Duration::from_secs(2)).await;
                            },
                            move |()| Message::GuildRefresh { ident },
                        );
                    }
                    SimpleNote::GuildKick(name) => {
                        account.social.status = Some(format!("Kicked {name}"));
                        return Command::perform(
                            async {
                                sleep(Duration::from_secs(2)).await;
                            },
                            move |()| Message::GuildRefresh { ident },
                        );
                    }
                    SimpleNote::GuildInvite(name) => {
                        account.social.status = Some(format!("Invited {name}"));
                        account.social.invited.push(name.to_lowercase());
                    }
                    SimpleNote::DemonPortal => {
                        if won == Some(true) {
                            Self::push_success(
                                &mut self.successes,
                                &account.name,
                                format!(
                                    "Demonic portal: won{}",
                                    gain_text(fight_gain)
                                ),
                            );
                        }
                        let msg = "Demonic portal - fought".to_string();
                        account.dungeon_info.status = Some(msg.clone());
                        account.dungeon_info.log.push((
                            Local::now(),
                            msg,
                            true,
                        ));
                    }
                    SimpleNote::Expedition(what) => {
                        let ei = &mut account.expedition_info;
                        ei.status = Some(what.clone());
                        // Something went out and came back: the tavern has
                        // work in hand, the queue keeps the rest waiting -
                        // and it is plainly not done for the day
                        ei.stalled = false;
                        ei.tavern_done_at = None;
                        let is_check = what.starts_with("Checked");
                        if is_check {
                            ei.checks_in_a_row =
                                ei.checks_in_a_row.saturating_add(1);
                        } else {
                            ei.checks_in_a_row = 0;
                            ei.log.push((Local::now(), what, true));
                        }
                        // A poll is how a stale expedition screen gets
                        // refreshed, and one or two in a row are normal. Any
                        // more and the answer is not going to change - the
                        // screen is waiting on the game's own clock, so this
                        // waits for the next tick instead of asking forty
                        // times and filling the console with it
                        if ei.checks_in_a_row > 2 {
                            ei.status = Some(
                                "Nothing new on the expedition screen - \
                                 waiting for the timer"
                                    .to_string(),
                            );
                            // ...and the tavern lets go of the queue: with
                            // `stalled` false the scheduler put the
                            // expedition straight back in and the poll
                            // went out every few seconds, forty lines of
                            // "Checked the expedition" and nothing else
                            // running. The checker's own tick (45-75 s)
                            // brings it back
                            ei.stalled = true;
                            return Command::none();
                        }
                        // Straight on to the next step - the state machine
                        // stops by itself when there is nothing to do
                        return Command::perform(
                            async {
                                crate::pace::sleep_between(1500, 3000).await;
                            },
                            move |()| Message::ExpeditionTick { ident },
                        );
                    }
                    SimpleNote::Dailies(what) => {
                        let di = &mut account.dailies_info;
                        di.status = Some(what.clone());
                        di.log.push((Local::now(), what, true));
                        // There may be a second chest waiting
                        return Command::perform(
                            async {
                                crate::pace::sleep_between(1200, 2500).await;
                            },
                            move |()| Message::DailiesTick { ident },
                        );
                    }
                    SimpleNote::IdleCollected(what) => {
                        account.idle_info.collect_refusals = 0;
                        account.idle_info.status = Some(what.clone());
                        account.idle_info.log.push((Local::now(), what, true));
                        return Command::perform(
                            async {
                                crate::pace::sleep_between(1200, 2500).await;
                            },
                            move |()| Message::IdleTick { ident },
                        );
                    }
                    SimpleNote::IdleBought(what) => {
                        account.idle_info.status = Some(what.clone());
                        account.idle_info.log.push((Local::now(), what, true));
                        if let Some((typ, before)) = account.idle_info.pending_upgrade.take() {
                            let changed = match &*account.status.lock_safe() {
                                AccountStatus::Idle(_, gs) | AccountStatus::Busy(gs, _) =>
                                    gs.idle_game.as_ref().is_some_and(|idle| idle.buildings[typ].level > before),
                                _ => false,
                            };
                            if !changed {
                                // No purse check for the next five minutes
                                account.idle_info.probe_paused_until =
                                    Some(Local::now() + chrono::Duration::minutes(5));
                                account.idle_info.status = Some(crate::i18n::t("Upgrade not confirmed; waiting for fresh game data").into());
                                account.queue.retain(|(job, _)| *job != Job::ArenaManager);
                                return Command::none();
                            }
                        }
                        let enabled = account.idle_info.auto_upgrade || account.idle_info.auto_sacrifice;
                        if idle_requeue_after_burst(&mut account.queue, account.idle_info.bought_this_round, enabled, Local::now()) {
                            account.idle_info.status = Some(crate::i18n::t("Upgrades queued; letting other tasks run first").into());
                            return Command::none();
                        }
                        return Command::perform(
                            async {
                                crate::pace::sleep_between(1200, 2500).await;
                            },
                            move |()| Message::IdleTick { ident },
                        );
                    }
                    SimpleNote::PetOpened(_) => {
                        // The screen is open - now the feed itself
                        return Command::perform(
                            async {
                                crate::pace::sleep_between(600, 1200).await;
                            },
                            move |()| Message::PetFeedTick {
                                ident,
                                attempt: 0,
                            },
                        );
                    }
                    SimpleNote::PetDungeonOpened { pet_id, .. } => {
                        // Selected - now the fight, with that pet
                        account.dungeon_info.pet_opened_for = Some(pet_id);
                        return Command::perform(
                            async {
                                crate::pace::sleep_between(600, 1200).await;
                            },
                            move |()| Message::AutoDungeonTick { ident },
                        );
                    }
                    SimpleNote::PetFed { pet, .. } => {
                        // The server refuses honestly when a pet may not
                        // eat, so a yes is a yes - even though the answer
                        // carries no pet data to prove it with
                        if let Some((id, habitat)) = account.pet_info.last_feed
                        {
                            *account
                                .pet_info
                                .fed_today
                                .entry(id)
                                .or_insert(0) += 1;
                            // One fruit of that habitat is gone, whether or
                            // not the game says so in this answer
                            if let Some((_, spent)) =
                                account.pet_info.fruit_used.get_mut(&habitat)
                            {
                                *spent += 1;
                            }
                        }
                        account.pet_info.last_feed = None;
                        account.pet_info.opened_for = None;
                        // Not "Fed" - the game has not proven anything yet.
                        // The next pass looks at the pet data and either
                        // carries on or stops and says so
                        let msg = format!("Feeding {pet}");
                        account.pet_info.status = Some(msg.clone());
                        account.pet_info.log.push((Local::now(), msg, true));
                        if self.outranked_in_queue(ident, Job::Pets) {
                            return self.yield_to_queue(ident, Job::Pets);
                        }
                        return Command::perform(
                            async {
                                crate::pace::sleep_between(1500, 3000).await;
                            },
                            move |()| Message::PetFeedTick {
                                ident,
                                attempt: 0,
                            },
                        );
                    }
                    SimpleNote::BackpackCleared(what)
                    | SimpleNote::BackpackDismantled(what) => {
                        account.shop_info.log.push((
                            Local::now(),
                            what.clone(),
                            true,
                        ));
                        account.dungeon_info.status = Some(what);
                        // There is room now - take the fight we came for
                        return Command::perform(
                            async {
                                crate::pace::sleep_between(900, 1800).await;
                            },
                            move |()| Message::AutoDungeonTick { ident },
                        );
                    }
                    SimpleNote::PetArena { name } => {
                        let won = match &*account.status.lock_safe() {
                            AccountStatus::Idle(_, gs)
                            | AccountStatus::Busy(gs, _) => {
                                gs.last_fight.as_ref().map(|f| f.has_player_won)
                            }
                            _ => None,
                        };
                        if won == Some(true) {
                            Self::push_success(
                                &mut self.successes,
                                &account.name,
                                format!("Pet arena {name}: won"),
                            );
                        }
                        let msg = match won {
                            Some(true) => format!("{name}: won"),
                            Some(false) => format!("{name}: lost"),
                            None => name,
                        };
                        account.pet_info.arena_status = Some(msg.clone());
                        account.pet_info.log.push((
                            Local::now(),
                            msg,
                            won != Some(false),
                        ));
                    }
                    SimpleNote::Dice { what, stage } => {
                        let ti = &mut account.tavern_info;
                        ti.dice_stage = stage;
                        ti.dice_status = Some(what.clone());
                        ti.dice_log.push((Local::now(), what, true));
                        // The day's ten games are played out in one sitting.
                        // This used to step aside for anything ahead of it
                        // in the queue between games - and the daily reward
                        // is ahead of it and goes back in the queue every
                        // sweep, so the dice were interrupted after almost
                        // every game and had to fight their way back.
                        // Ten games is twenty requests over half a minute,
                        // under the per-minute ceiling either way
                        return Command::perform(
                            async {
                                crate::pace::sleep_between(900, 1800).await;
                            },
                            move |()| Message::DiceTick { ident },
                        );
                    }
                    SimpleNote::FortressScout { auto } => {
                        return self.fortress_attack_scouted(ident, auto);
                    }
                    SimpleNote::UnderworldMaint(what) => {
                        // A collect says how much came in, read off the
                        // state after the answer against what was there
                        // before the request. Zero is worth seeing too:
                        // that is how a wrong resource id shows itself
                        let after = {
                            let arc = account.status.clone();
                            let lock = arc.lock_safe();
                            match &*lock {
                                AccountStatus::Idle(_, gs)
                                | AccountStatus::Busy(gs, _) => {
                                    gs.underworld.as_ref().map(|uw| {
                                        (
                                            uw.souls_current, uw.souls_limit,
                                            gs.character.silver,
                                        )
                                    })
                                }
                                _ => None,
                            }
                        };
                        let name = account.name.clone();
                        if let Some(ui) = &mut account.underworld_info {
                            ui.maintain_status = Some(what.clone());
                            if let (
                                Some((typ, before)),
                                Some((souls, limit, silver)),
                            ) = (ui.collect_before.take(), after)
                            {
                                use sf_api::gamestate::underworld::UnderworldResourceType as R;
                                let line = match typ {
                                    R::Souls => format!(
                                        "Underworld: collected {} souls ({souls}/{limit})",
                                        souls.saturating_sub(before)
                                    ),
                                    R::Silver => format!(
                                        "Underworld: collected {} gold from the pit",
                                        silver.saturating_sub(before) / 100
                                    ),
                                    R::ThirstForAdventure => {
                                        "Underworld: collected the thirst for adventure".to_string()
                                    }
                                };
                                ui.maintain_status = Some(line.clone());
                                Self::push_success(
                                    &mut self.successes, &name, line,
                                );
                            } else if what.starts_with("Upgrading") {
                                Self::push_success(
                                    &mut self.successes,
                                    &name,
                                    what.clone(),
                                );
                            }
                        }
                        // One thing per request - there may be more waiting
                        // (silver after souls, a build after a collect)
                        return Command::perform(
                            async {
                                crate::pace::sleep_between(1500, 3000).await;
                            },
                            move |()| Message::UnderworldMaintainTick { ident },
                        );
                    }
                    SimpleNote::FortressMaint(what) => {
                        if let Some(fi) = &mut account.fortress_info {
                            fi.maintain_status = Some(what.clone());
                            fi.log.push((Local::now(), what, true));
                        }
                        // One thing per request. There might be more to do
                        // (collect stone after wood, ...), so go again soon
                        return Command::perform(
                            async {
                                crate::pace::sleep_between(1500, 3000).await;
                            },
                            move |()| Message::FortressMaintainTick { ident },
                        );
                    }
                    SimpleNote::GuildChat => {
                        if self.config.sounds {
                            crate::sound::play(crate::sound::Sound::Message);
                        }
                        account.social.status = Some("Message sent".to_string())
                    }
                    SimpleNote::Mail => account.social.status = None,
                    // A witch enchantment that went through is an
                    // ordinary shop step: the piece took it, so there is
                    // nothing more to remember about it
                    SimpleNote::Recycle { action } => {
                        let pending = account.shop_info.pending_recycle.take();
                        let confirmed = pending.is_some_and(|p| {
                            let lock = account.status.lock_safe();
                            match &*lock {
                                AccountStatus::Idle(_, gs) | AccountStatus::Busy(gs, _) =>
                                    gs.character.inventory.backpack.get(p.bag_pos.backpack_pos())
                                        .and_then(|i| i.as_ref())
                                        .is_none_or(|i| Some(i.command_ident()) != p.old_ident),
                                _ => false,
                            }
                        });
                        let what = if confirmed {
                            match action {
                                crate::shopping::RecycleAction::ExtractGem => crate::i18n::t("Gem recovered from replaced equipment."),
                                crate::shopping::RecycleAction::Dismantle => crate::i18n::t("Replaced equipment dismantled."),
                                crate::shopping::RecycleAction::Sell => crate::i18n::t("Replaced equipment sold."),
                            }
                        } else { crate::i18n::t("Recycling was not confirmed; the item was kept.") }.to_string();
                        account.shop_info.status = Some(what.clone());
                        account.shop_info.log.push((Local::now(), what, confirmed));
                        return Command::perform(crate::pace::sleep_between(800, 1500),
                            move |()| Message::ShopTick { ident });
                    }
                    SimpleNote::Shop(what)
                    | SimpleNote::WitchEnchant { what, .. } => {
                        let smith_level = what.starts_with("Raised the ");
                        // A companion equip is checked against the slot
                        // in the answer, the way the character's own
                        // purchases are: the line in the successes says
                        // whether they wear it now, not that we asked
                        let worn_now = {
                            let arc = account.status.clone();
                            let lock = arc.lock_safe();
                            match (&*lock, &account.shop_info.pending_companion)
                            {
                                (
                                    AccountStatus::Idle(_, gs)
                                    | AccountStatus::Busy(gs, _),
                                    Some(p),
                                ) => Some(
                                    gs.dungeons
                                        .companions
                                        .as_ref()
                                        .is_some_and(|c| {
                                            c[p.companion].equipment.0[p.slot]
                                                .as_ref()
                                                .is_some_and(|i| {
                                                    i.command_ident()
                                                        == p.new_ident
                                                })
                                        }),
                                ),
                                _ => None,
                            }
                        };
                        // A blacksmith level the same way: the piece is a
                        // level higher, or the metal is gone (a companion's
                        // piece may not come back in the answer, the purse
                        // always does) - otherwise the game did not take it
                        let smith_now = {
                            let arc = account.status.clone();
                            let lock = arc.lock_safe();
                            match (&*lock, &account.shop_info.pending_smith) {
                                (
                                    AccountStatus::Idle(_, gs)
                                    | AccountStatus::Busy(gs, _),
                                    Some(p),
                                ) => {
                                    let level = match p.wearer {
                                        None => gs.character.equipment.0
                                            [p.slot]
                                            .as_ref()
                                            .map(|i| i.upgrade_count),
                                        Some(c) => gs
                                            .dungeons
                                            .companions
                                            .as_ref()
                                            .and_then(|cs| {
                                                cs[c].equipment.0[p.slot]
                                                    .as_ref()
                                                    .map(|i| i.upgrade_count)
                                            }),
                                    };
                                    let metal = gs
                                        .blacksmith
                                        .as_ref()
                                        .map_or(0, |b| b.metal);
                                    Some((level, metal))
                                }
                                _ => None,
                            }
                        };
                        let name = account.name.clone();
                        let si = &mut account.shop_info;
                        let mut refused_line = None;
                        if let (Some(p), Some((level, metal))) =
                            (si.pending_smith.take(), smith_now)
                        {
                            let taken = level.is_some_and(|l| l > p.from)
                                || metal < p.metal_before;
                            let line = if taken {
                                p.what.clone()
                            } else {
                                format!(
                                    "The blacksmith did not take the level on the {:?} \
                                     of {} - nothing changed in the game",
                                    p.slot,
                                    crate::smith::wearer_name(p.wearer)
                                )
                            };
                            si.log.push((Local::now(), line.clone(), taken));
                            if taken {
                                Self::push_success(
                                    &mut self.successes,
                                    &name,
                                    line.clone(),
                                );
                                // The goblin every fifth level, not every one
                                if (p.from + 1) % 5 == 0 {
                                    Self::push_goblin(
                                        &mut self.console,
                                        &name,
                                        crate::goblin::Mood::Blacksmith,
                                    );
                                }
                            } else {
                                refused_line = Some(line.clone());
                            }
                            si.smith_status = Some(line);
                        } else if let (Some(p), Some(worn)) =
                            (si.pending_companion.take(), worn_now)
                        {
                            let who =
                                crate::companions::companion_name(p.companion);
                            let line = if worn {
                                format!(
                                    "{who} wears the new {:?} now{}",
                                    p.slot,
                                    if p.had_before {
                                        " - the old one is back in the bag"
                                    } else {
                                        ""
                                    }
                                )
                            } else {
                                format!(
                                    "{who} does NOT wear the new {:?} - the game did not take the swap ({})",
                                    p.slot, p.what
                                )
                            };
                            si.log.push((Local::now(), line.clone(), worn));
                            if worn {
                                Self::push_success(
                                    &mut self.successes,
                                    &name,
                                    line.clone(),
                                );
                            } else {
                                refused_line = Some(line.clone());
                            }
                            si.status = Some(line);
                        } else {
                            si.status = Some(what.clone());
                            si.log.push((Local::now(), what, true));
                        }
                        if let Some(line) = refused_line {
                            self.console(Some(ident), line, Some(false));
                        }
                        // A blacksmith level is followed by a longer pause
                        // than a purchase: twenty levels in a row at two
                        // seconds each is a burst nobody clicks
                        let (lo, hi) = if smith_level {
                            (6000, 14000)
                        } else {
                            (1500, 3000)
                        };
                        return Command::perform(
                            crate::pace::sleep_between(lo, hi),
                            move |()| Message::ShopTick { ident },
                        );
                    }
                    SimpleNote::Attr(what, silver_before) => {
                        let now_silver = {
                            let arc = account.status.clone();
                            let lock = arc.lock_safe();
                            match &*lock {
                                AccountStatus::Idle(_, gs)
                                | AccountStatus::Busy(gs, _) => {
                                    gs.character.silver
                                }
                                _ => silver_before,
                            }
                        };
                        let si = &mut account.shop_info;
                        let cost = silver_before.saturating_sub(now_silver);
                        if cost > 0 {
                            si.attr_last_cost = Some(cost);
                        }
                        let msg = format!("{what} ({} gold)", cost / 100);
                        si.attr_status = Some(msg.clone());
                        si.log.push((Local::now(), msg, true));
                        return Command::perform(
                            crate::pace::sleep_between(1500, 3000),
                            move |()| Message::AttrTick { ident },
                        );
                    }
                    SimpleNote::Beer => {
                        let ei = &mut account.expedition_info;
                        ei.beer_unaffordable_at = None;
                        ei.stalled = false;
                        let msg = "Drank a beer - thirst refilled".to_string();
                        ei.status = Some(msg.clone());
                        ei.log.push((Local::now(), msg, true));
                        return Command::perform(
                            async {
                                sleep(Duration::from_secs(3)).await;
                            },
                            move |()| Message::ExpeditionTick { ident },
                        );
                    }
                    SimpleNote::Wheel(what) => {
                        let ti = &mut account.tavern_info;
                        let won = {
                            let arc = account.status.clone();
                            let lock = arc.lock_safe();
                            match &*lock {
                                AccountStatus::Idle(_, gs)
                                | AccountStatus::Busy(gs, _) => gs
                                    .specials
                                    .wheel
                                    .result
                                    .as_ref()
                                    .map(|r| {
                                        format!(" -> {:?} x{}", r.typ, r.amount)
                                    })
                                    .unwrap_or_default(),
                                _ => String::new(),
                            }
                        };
                        let msg = format!("{what}{won}");
                        ti.wheel_status = Some(msg.clone());
                        ti.wheel_log.push((Local::now(), msg, true));
                        return Command::perform(
                            async {
                                sleep(Duration::from_secs(4)).await;
                            },
                            move |()| Message::WheelTick { ident },
                        );
                    }
                    SimpleNote::Guard(what) => {
                        let gi = &mut account.guard_info;
                        gi.status = Some(what.clone());
                        gi.log.push((Local::now(), what.clone(), true));
                        if what.starts_with("Finished")
                            && (gi.auto || what.contains("quest"))
                        {
                            // The wage is in - straight on to the next shift
                            return Command::perform(
                                async {
                                    sleep(Duration::from_secs(3)).await;
                                },
                                move |()| Message::GuardTimer { ident },
                            );
                        }
                    }
                    SimpleNote::GuildLookup { id } => {
                        let (found, our) = {
                            let arc = account.status.clone();
                            let lock = arc.lock_safe();
                            match &*lock {
                                AccountStatus::Idle(_, gs)
                                | AccountStatus::Busy(gs, _) => {
                                    let our = gs
                                        .guild
                                        .as_ref()
                                        .map(|g| g.name.clone())
                                        .unwrap_or_default();
                                    let before = &account.social.lookup_before;
                                    let fresh = gs
                                        .lookup
                                        .guilds
                                        .values()
                                        .find(|g| !before.contains(&g.name))
                                        .or_else(|| {
                                            gs.lookup.guilds.values().find(
                                                |g| {
                                                    g.attacks.as_deref()
                                                        == Some(&our)
                                                        || g.defends_against
                                                            .as_deref()
                                                            == Some(&our)
                                                },
                                            )
                                        });
                                    (
                                        fresh.map(|g| {
                                            (
                                                g.name.clone(),
                                                crate::guild_fight::other_power(g),
                                                crate::guild_fight::describe_other(g),
                                            )
                                        }),
                                        our,
                                    )
                                }
                                _ => (None, String::new()),
                            }
                        };
                        let _ = our;
                        match (id, found) {
                            (Some(id), Some((name, power, desc))) => {
                                account.social.enemies.retain(|e| e.id != id);
                                account.social.enemies.push(EnemyGuild {
                                    id,
                                    name,
                                    power,
                                    desc,
                                });
                            }
                            (None, Some((name, power, desc))) => {
                                let own = {
                                    let arc = account.status.clone();
                                    let lock = arc.lock_safe();
                                    match &*lock {
                                        AccountStatus::Idle(_, gs)
                                        | AccountStatus::Busy(gs, _) => gs
                                            .guild
                                            .as_ref()
                                            .map(crate::guild_fight::own_power)
                                            .unwrap_or(0.0),
                                        _ => 0.0,
                                    }
                                };
                                let chance =
                                    crate::guild_fight::win_chance(own, power);
                                account.social.compare_result = Some(format!(
                                    "{name}: ~{:.0} % for us ({desc})",
                                    chance * 100.0
                                ));
                            }
                            (_, None) => {
                                account.social.compare_result =
                                    Some("Guild not found".to_string());
                            }
                        }
                    }
                    SimpleNote::MailOpen { pos, .. } => {
                        account.social.open_msg = Some(pos);
                        account.social.open_text = mail_text;
                        account.social.status = None;
                        // Replying is the usual next step, so the sender is
                        // put in as the recipient right away
                        let from = {
                            let arc = account.status.clone();
                            let lock = arc.lock_safe();
                            match &*lock {
                                AccountStatus::Idle(_, gs)
                                | AccountStatus::Busy(gs, _) => gs
                                    .mail
                                    .inbox
                                    .get(pos as usize)
                                    .map(|e| e.from.clone()),
                                _ => None,
                            }
                        };
                        if let Some(from) = from {
                            account.social.reply_to = from;
                        }
                    }
                    SimpleNote::MailSent(to) => {
                        account.social.reply_text.clear();
                        account.social.status = Some(format!(
                            "Message sent to {}",
                            titlecase::titlecase(&to)
                        ));
                    }
                    SimpleNote::ReportSent(_) => {
                        account.social.report_sent += 1;
                        account.social.report_failed = 0;
                        account.social.report_status = Some(format!(
                            "Report: {} sent, {} to go",
                            account.social.report_sent,
                            account.social.report_queue.len()
                        ));
                        // Spread the mails out, so that it does not look like
                        // a flood to the server
                        return Command::perform(
                            async {
                                crate::pace::sleep_between(2500, 4500).await;
                            },
                            move |()| Message::GuildReportNext { ident },
                        );
                    }
                }
            }
            Message::SimpleFailed {
                ident,
                session,
                note,
                error,
            } => {
                let error = friendly_error(&error);
                // "sessionid invalid": the game keeps one session per
                // character and somebody logged in elsewhere - a browser,
                // another device, another copy of this app. Putting the dead
                // session back would just fail the same way on every
                // request; the relogin path takes it from here (and logs
                // the other window out, which is the game's rule, not ours)
                if session_is_dead(&error) {
                    self.console(
                        Some(ident),
                        "Session lost - somebody logged into this character \
                         elsewhere (browser, another device). The game keeps \
                         one session, so this app logs in again in a moment; \
                         the other window will be logged out"
                            .to_string(),
                        Some(false),
                    );
                    return Command::perform(async {}, move |()| {
                        Message::PlayerCommandFailed {
                            ident,
                            session,
                            attempt: 0,
                        }
                    });
                }
                if !(matches!(note, SimpleNote::Beer) && error.to_lowercase().contains("afford")) {
                    self.console(Some(ident), format!("{}: {error}", note.label()), Some(false));
                }
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let arc = account.status.clone();
                let mut lock = arc.lock_safe();
                lock.put_session(session);
                drop(lock);
                // Without the note there is no way to tell which job the
                // server turned down - the log was full of "the free fight is
                // not available yet" with nothing saying what asked for it
                warn!("Command refused for {ident} ({note:?}): {error}");
                match note {
                    SimpleNote::Tower { attempt } if attempt < 2 => {
                        // Wrong floor number - try the next candidate
                        return Command::perform(
                            async {
                                sleep(Duration::from_millis(600)).await;
                            },
                            move |()| {
                                Message::DungeonFight {
                                ident,
                                dungeon: SFDungeon::Light(
                                    sf_api::gamestate::dungeons::LightDungeon::Tower,
                                ),
                                name: "Tower".to_string(),
                                attempt: attempt + 1,
                                mushroom: false,
                            }
                            },
                        );
                    }
                    SimpleNote::Tower { .. } => {
                        let msg = format!("Tower: {error}");
                        account.dungeon_info.status = Some(msg.clone());
                        account.dungeon_info.log.push((
                            Local::now(),
                            msg,
                            false,
                        ));
                    }
                    SimpleNote::Dungeon(name) => {
                        let msg = format!("{name}: {error}");
                        account.dungeon_info.status = Some(msg.clone());
                        account.dungeon_info.log.push((
                            Local::now(),
                            msg,
                            false,
                        ));
                    }
                    SimpleNote::UnderworldMaint(what) => {
                        // A refused step ends the round. The next turn asks
                        // the game again, by which time the answer may well
                        // be different (a build finished, silver came in)
                        if let Some(ui) = &mut account.underworld_info {
                            ui.maintain_status =
                                Some(format!("{what} refused: {error}"));
                        }
                    }
                    SimpleNote::FortressMaint(what) => {
                        let build_done = {
                            let arc = account.status.clone();
                            let lock = arc.lock_safe();
                            match &*lock {
                                AccountStatus::Idle(_, gs)
                                | AccountStatus::Busy(gs, _) => gs
                                    .fortress
                                    .as_ref()
                                    .and_then(|f| f.building_upgrade.finish),
                                _ => None,
                            }
                        };
                        if let Some(fi) = &mut account.fortress_info {
                            let msg = format!("{what} refused: {error}");
                            fi.maintain_status = Some(msg.clone());
                            fi.log.push((Local::now(), msg, false));
                            if what.contains("Hall of Knights") {
                                fi.hok_blocked_until = Some(
                                    Local::now() + chrono::Duration::hours(6),
                                );
                            }
                            if what.contains("gem")
                                && error.contains("building")
                            {
                                // Do not knock on that door every 3 minutes
                                fi.gems_blocked_until =
                                    Some(build_done.unwrap_or_else(|| {
                                        Local::now()
                                            + chrono::Duration::hours(1)
                                    }));
                            }
                        }
                    }
                    SimpleNote::FortressScout { .. } => {
                        if let Some(fi) = &mut account.fortress_info {
                            fi.status = Some(format!(
                                "Could not look at the opponent: {error}"
                            ));
                        }
                    }
                    SimpleNote::GuildSigned(what) => {
                        account.social.status =
                            Some(format!("Could not join {what}: {error}"));
                    }
                    SimpleNote::Sacrifice { what, .. } => {
                        let msg = format!("Sacrifice refused ({what}): {error}");
                        account.shop_info.status = Some(msg.clone());
                        account.shop_info.log.push((Local::now(), msg, false));
                    }
                    SimpleNote::GuildAction(what) => {
                        account.social.status =
                            Some(format!("{what} refused: {error}"));
                    }
                    SimpleNote::GuildPet(what) => {
                        // A price read wrong, a cap the game knows better:
                        // not asked again for six hours
                        account.social.pet_blocked_until =
                            Some(Local::now() + chrono::Duration::hours(6));
                        account.social.status =
                            Some(format!("{what} refused: {error}"));
                    }
                    SimpleNote::PetOpened(pet) => {
                        // Could not even open it - nothing to feed then
                        account.pet_info.opened_for = None;
                        account.pet_info.status =
                            Some(format!("Opening {pet} refused: {error}"));
                    }
                    SimpleNote::PetDungeonOpened { name, .. } => {
                        account.dungeon_info.pet_opened_for = None;
                        account.dungeon_info.status =
                            Some(format!("Selecting {name} refused: {error}"));
                    }
                    SimpleNote::PetFed { pet, .. } => {
                        // A refusal names its reason, and the reason decides
                        // what it means. "no fruit of that kind left" while
                        // the habitat plainly holds fruit is not about the
                        // pet at all - it is the fruit index being the wrong
                        // name for it
                        let msg = format!("Feeding {pet} refused: {error}");
                        account.pet_info.status = Some(msg.clone());
                        account.pet_info.log.push((Local::now(), msg, false));
                        // An honest "no" is not a phantom feed. Mark this pet
                        // done for the day so it is not asked again this
                        // cycle - one refusal per pet, then quiet, whatever
                        // the reason
                        account.pet_info.last_was_refusal = true;
                        if let Some((id, _)) = account.pet_info.last_feed {
                            account.pet_info.done_today.insert(id);
                        }
                        account.pet_info.last_feed = None;
                        account.pet_info.opened_for = None;
                        return Command::perform(
                            async {
                                crate::pace::sleep_between(900, 1800).await;
                            },
                            move |()| Message::PetFeedTick {
                                ident,
                                attempt: 0,
                            },
                        );
                    }
                    SimpleNote::BackpackCleared(what) => {
                        let msg = format!("{what} refused: {error}");
                        account.dungeon_info.status = Some(msg.clone());
                        account.shop_info.log.push((Local::now(), msg, false));
                    }
                    SimpleNote::BackpackDismantled(what) => {
                        // Most likely the day's dismantles are used up. Sell
                        // instead for the next six hours rather than asking
                        // the blacksmith again every round
                        account.shop_info.dismantle_blocked_until =
                            Some(Local::now() + chrono::Duration::hours(6));
                        let msg = format!(
                            "{what} refused: {error} - selling instead for now"
                        );
                        account.dungeon_info.status = Some(msg.clone());
                        account.shop_info.log.push((Local::now(), msg, false));
                    }
                    SimpleNote::PetArena { name } => {
                        let msg = format!("{name} refused: {error}");
                        account.pet_info.arena_status = Some(msg.clone());
                        account.pet_info.log.push((Local::now(), msg, false));
                    }
                    SimpleNote::Dice { what, .. } => {
                        let ti = &mut account.tavern_info;
                        // Whatever went wrong, the game is over for now -
                        // start from the beginning
                        ti.dice_stage = 0;
                        let msg = format!("{what} refused: {error}");
                        ti.dice_status = Some(msg.clone());
                        ti.dice_log.push((Local::now(), msg, false));
                        // "cannot roll for free yet" while our timer said
                        // free: the timer in the state is stale. The next
                        // start pays with a glass, and it comes right away
                        // instead of half an hour later with the checker
                        if error.to_lowercase().contains("free") {
                            ti.dice_force_glass = true;
                            return Command::perform(
                                async {
                                    crate::pace::sleep_between(2000, 4000)
                                        .await;
                                },
                                move |()| Message::DiceTick { ident },
                            );
                        }
                    }
                    SimpleNote::IdleBought(what) => {
                        account.idle_info.pending_upgrade = None;
                        // What the game said the Arena Manager holds, the
                        // moment it refused: without it a refusal is a
                        // guess about money nobody can see
                        if let AccountStatus::Idle(_, gs)
                        | AccountStatus::Busy(gs, _) =
                            &*account.status.lock_safe()
                            && let Some(idle) = gs.idle_game.as_ref()
                        {
                            log::info!(
                                "{ident}: arena manager refused - {}",
                                crate::mechanics::idle_snapshot_line(
                                    idle,
                                    Local::now()
                                )
                            );
                        }
                        // "Need more banknotes": there really is not
                        // enough. No purse check for five minutes, or the
                        // next round asks the same a second later (once a
                        // second on 28 October)
                        account.idle_info.probe_paused_until =
                            Some(Local::now() + chrono::Duration::minutes(5));
                        // A refused sacrifice keeps the guard where it is:
                        // the game's `resets` did not move, so asking
                        // again on the same count would be the same
                        // refusal. It clears itself when the game's own
                        // counter changes - see `sacrificed_at_resets`
                        let msg = format!("{what} refused: {error}");
                        account.idle_info.status = Some(msg.clone());
                        account.idle_info.log.push((Local::now(), msg, false));
                    }
                    SimpleNote::Expedition(what) => {
                        let ei = &mut account.expedition_info;
                        ei.stalled = true;
                        let msg = format!("{what}: {error}");
                        ei.status = Some(msg.clone());
                        ei.log.push((Local::now(), msg, false));
                    }
                    SimpleNote::Dailies(what) => {
                        let di = &mut account.dailies_info;
                        let msg = format!("{what}: {error}");
                        di.status = Some(msg.clone());
                        di.log.push((Local::now(), msg, false));
                    }
                    SimpleNote::WitchEnchant { what, model } => {
                        // "cannot enchant this itemtype": the witch's own
                        // list says which slots have an enchantment
                        // unlocked and says nothing about the piece worn
                        // in one, so the game refusing is the only way to
                        // find out. Remembered against that piece, so the
                        // next round moves on to a slot that will take one
                        // instead of buying the same refusal again
                        let si = &mut account.shop_info;
                        si.witch_refused.insert(model);
                        let msg = format!("{what}: {error}");
                        si.witch_status = Some(msg.clone());
                        si.status = Some(msg.clone());
                        si.log.push((Local::now(), msg, false));
                    }
                    SimpleNote::Recycle { action } => {
                        let line = format!("{}: {error}", crate::i18n::t("Recycling refused"));
                        account.shop_info.status = Some(line.clone());
                        account.shop_info.log.push((Local::now(), line, false));
                        if action == crate::shopping::RecycleAction::ExtractGem
                            && crate::shopping::extraction_unavailable(&error)
                        {
                            return self.recycle_replaced(ident, true);
                        }
                        account.shop_info.pending_recycle = None;
                    }
                    SimpleNote::Shop(what) => {
                        let si = &mut account.shop_info;
                        let msg = format!("{what}: {error}");
                        si.status = Some(msg.clone());
                        if si.pending_smith.take().is_some() {
                            si.smith_status = Some(msg.clone());
                        }
                        si.log.push((Local::now(), msg, false));
                        // Whatever step failed, do not keep a half-done
                        // purchase around - the next round looks again
                        si.pending = None;
                        si.pending_companion = None;
                    }
                    SimpleNote::Attr(what, silver_before) => {
                        let si = &mut account.shop_info;
                        // "need more gold": the price is not in the game
                        // state, only learned from purchases, and the
                        // guess (last price + 5 %) was under it. The whole
                        // purse was not enough, so the purse is the new
                        // floor - the next try waits until the gold has
                        // grown past it instead of asking every minute
                        if error.to_lowercase().contains("gold") {
                            si.attr_last_cost = Some(
                                si.attr_last_cost
                                    .unwrap_or(0)
                                    .max(silver_before),
                            );
                        }
                        let msg = format!("{what}: {error}");
                        si.attr_status = Some(msg.clone());
                        si.log.push((Local::now(), msg, false));
                    }
                    SimpleNote::Beer => {
                        let funds = match &*account.status.lock_safe() {
                            AccountStatus::Idle(_, gs) | AccountStatus::Busy(gs, _) => account.free_mushrooms(gs),
                            _ => 0,
                        };
                        let vip = account.tavern_info.beer_free;
                        let ei = &mut account.expedition_info;
                        ei.stalled = true;
                        if error.to_lowercase().contains("afford") {
                            let state = (funds, vip);
                            let msg = crate::i18n::t("Not enough mushrooms for beer. Waiting for more funds.").to_string();
                            if ei.beer_unaffordable_at != Some(state) {
                                ei.log.push((Local::now(), msg.clone(), false));
                            }
                            ei.beer_unaffordable_at = Some(state);
                            ei.beer_batch = false;
                            ei.status = Some(msg);
                        } else {
                            let msg = format!("Beer refused: {error}");
                            ei.status = Some(msg.clone());
                            ei.log.push((Local::now(), msg, false));
                            if error.to_lowercase().contains("thirsty") {
                                ei.beer_blocked_until = Some(Local::now() + chrono::Duration::minutes(30));
                            }
                        }
                    }
                    SimpleNote::Wheel(what) => {
                        let ti = &mut account.tavern_info;
                        let msg = format!("{what}: {error}");
                        ti.wheel_status = Some(msg.clone());
                        ti.wheel_log.push((Local::now(), msg, false));
                    }
                    SimpleNote::PetDungeon {
                        name,
                        habitat,
                        enemy_pos,
                        pet_id,
                        attempt: 0,
                    } => {
                        // Maybe the game wants the pet's number inside the
                        // habitat instead of the global id
                        let local = (pet_id - 1) % 20 + 1;
                        account.dungeon_info.status = Some(format!(
                            "{name}: {error} - retrying with pet #{local}"
                        ));
                        return self.send_simple(
                            ident,
                            "Dungeon",
                            sf_api::command::Command::FightPetDungeon {
                                use_mush: false,
                                habitat,
                                enemy_pos,
                                player_pet_id: local,
                            },
                            SimpleNote::PetDungeon {
                                name,
                                habitat,
                                enemy_pos,
                                pet_id: local,
                                attempt: 1,
                            },
                        );
                    }
                    SimpleNote::PetDungeon { name, .. } => {
                        // The pet habitats have a timer of their own that the
                        // game never shows us. A refusal used to end the turn
                        // here, with the five minute loop guard already set -
                        // so the normal dungeon never got its free fight and
                        // the log filled up with the same refusal every five
                        // minutes. Leave the pets alone for a while and go
                        // fight a normal dungeon right now instead
                        let di = &mut account.dungeon_info;
                        let msg = format!(
                            "{name} refused: {error} - leaving the pet \
                             habitats alone for 30 min and taking a normal \
                             dungeon instead"
                        );
                        di.status = Some(msg.clone());
                        di.log.push((Local::now(), msg, false));
                        di.pets_blocked_until =
                            Some(Local::now() + Duration::from_secs(30 * 60));
                        di.last_auto_fight = None;
                        log::info!(
                            "{ident}: pet dungeon refused - falling back to a \
                             normal dungeon"
                        );
                        return self.dungeon_auto_fight(ident);
                    }
                    SimpleNote::Guard(what) => {
                        let gi = &mut account.guard_info;
                        let msg = format!("{what}: {error}");
                        gi.status = Some(msg.clone());
                        gi.log.push((Local::now(), msg, false));
                    }
                    SimpleNote::IdleCollected(what) => {
                        let name = account.name.clone();
                        let ii = &mut account.idle_info;
                        ii.collect_refusals += 1;
                        let msg = format!("{what} refused: {error}");
                        ii.log.push((Local::now(), msg.clone(), false));
                        if ii.collect_refusals >= 3 {
                            ii.auto_collect = false;
                            ii.status = Some(
                                "Collecting switched off: the server refused \
                                 it 3 times - this command is not supported"
                                    .to_string(),
                            );
                            // Remember it, or every restart tries again
                            if let Some(conf) = self
                                .config
                                .get_char_conf_mut(&name, ident.server_id)
                            {
                                conf.idle_collect = false;
                                _ = self.config.write();
                            }
                        } else {
                            ii.status = Some(msg);
                            // Try the upgrades anyway
                            return Command::perform(async {}, move |()| {
                                Message::IdleTick { ident }
                            });
                        }
                    }
                    SimpleNote::ArenaOffer { .. } => {
                        if let Some(si) = &mut account.scrapbook_info {
                            si.arena_offer_busy = false;
                            si.auto_status =
                                Some(format!("Arena offer refused: {error}"));
                        }
                    }
                    SimpleNote::GuildRaid => {
                        account.social.status =
                            Some(format!("Raid not possible: {error}"));
                    }
                    SimpleNote::GuildFinder => {
                        account.social.finder_status =
                            Some(format!("Guild finder refused: {error}"));
                    }
                    SimpleNote::GuildFinderLook(name) => {
                        // One guild not readable is no reason to stop the
                        // pass - on to the next
                        account.social.finder_status =
                            Some(format!("Could not look at {name}: {error}"));
                        return Command::perform(
                            async {
                                sleep(Duration::from_secs(2)).await;
                            },
                            move |()| Message::GuildFinderStep { ident },
                        );
                    }
                    SimpleNote::GuildJoinLook(name) => {
                        account.social.finder_status =
                            Some(format!("Could not open {name}: {error}"));
                    }
                    SimpleNote::GuildJoin { name, by_id } => {
                        // "server not available" is the answer to a request
                        // the server did not understand - the name was not
                        // it, so the id goes next; after that the guild is
                        // on the tried list and the automatic pick moves on
                        if !by_id
                            && error.to_lowercase().contains("not available")
                        {
                            account.social.finder_status = Some(format!(
                                "{name}: the name was refused, trying the id"
                            ));
                            return self.guild_join_request(ident, name, true);
                        }
                        account.social.finder_status =
                            Some(format!("{name} refused us: {error}"));
                    }
                    SimpleNote::GuildKick(name) => {
                        account.social.status =
                            Some(format!("Could not kick {name}: {error}"));
                    }
                    SimpleNote::GuildInvite(name) => {
                        account.social.status =
                            Some(format!("Could not invite {name}: {error}"));
                        // Whatever the reason, not again this session
                        account.social.invited.push(name.to_lowercase());
                    }
                    SimpleNote::DemonPortal => {
                        // Almost always "already fought today" - the game is
                        // the authority, so just note it and wait for tomorrow
                        account.dungeon_info.status =
                            Some(format!("Demonic portal: {error}"));
                    }
                    SimpleNote::DungeonsUpdated => {
                        account.dungeon_info.status =
                            Some(format!("Could not open dungeons: {error}"));
                        return Command::perform(async {}, move |()| {
                            Message::DungeonCompute { ident }
                        });
                    }
                    // Asking by id did not work on this server - ask by
                    // position instead
                    SimpleNote::MailOpen { pos, by_id: true } => {
                        return Command::perform(
                            async {
                                sleep(Duration::from_millis(600)).await;
                            },
                            move |()| Message::MailOpen {
                                ident,
                                pos,
                                by_id: false,
                            },
                        );
                    }
                    SimpleNote::MailOpen { .. } => {
                        account.social.open_text = Some(format!(
                            "The server refused to open this message: {error}"
                        ));
                        account.social.status = None;
                    }
                    SimpleNote::MailSent(to) => {
                        account.social.status =
                            Some(format!("Could not send to {to}: {error}"));
                    }
                    SimpleNote::ReportSent(to) => {
                        account.social.report_failed += 1;
                        account.social.report_status =
                            Some(format!("Could not send to {to}: {error}"));
                        return Command::perform(
                            async {
                                sleep(Duration::from_secs(5)).await;
                            },
                            move |()| Message::GuildReportNext { ident },
                        );
                    }
                    _ => account.social.status = Some(error),
                }
            }
            Message::AutoFortress { ident, state } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                if let Some(fi) = &mut account.fortress_info {
                    fi.auto_attack = state;
                    fi.status = None;
                    fi.auto_since = state.then(Local::now);
                    // The first attack is drawn too - switching on at 20:00
                    // and attacking at 20:00:01 is a script's opening move
                    fi.next_due = None;
                    fi.raid = crate::player::RaidPhase::Filling;
                }
            }
            Message::FortressSetInterval { ident, secs } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                if let Some(fi) = &mut account.fortress_info {
                    // Anything below half a minute is neither useful nor
                    // polite towards the server
                    fi.interval = secs.max(30);
                    // A new average means a new draw
                    fi.next_due = None;
                }
            }
            Message::FortressSetNoticeMax { ident, minutes } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let name = account.name.clone();
                // Stored as typed. The floor is applied where the number
                // is used (`stealth::raid_notice_secs`), never here: a
                // field that rounds every keystroke up to five cannot be
                // typed into - clearing it gives 5, typing "1" gives 5,
                // and the only way to 120 is to type it after the 5
                if let Some(fi) = &mut account.fortress_info {
                    fi.notice_max_min = minutes;
                    // A new ceiling means a new draw, if one is pending
                    if fi.raid == crate::player::RaidPhase::Noticed {
                        fi.raid = crate::player::RaidPhase::Filling;
                        fi.next_due = None;
                    }
                }
                if let Some(conf) =
                    self.config.get_char_conf_mut(&name, ident.server_id)
                {
                    conf.fortress_notice_max_min = minutes;
                    _ = self.config.write();
                }
            }
            Message::FortressRaidStep { ident }
            | Message::AutoFortressPossible { ident } => {
                return self.fortress_raid_tick(ident);
            }
            Message::FortressAttack { ident } => {
                return self.fortress_command(
                    ident,
                    FortressActionKind::Attack,
                    false,
                );
            }
            Message::FortressReroll { ident } => {
                return self.fortress_command(
                    ident,
                    FortressActionKind::Reroll,
                    false,
                );
            }
            Message::FortressTrain { ident } => {
                return self.fortress_command(
                    ident,
                    FortressActionKind::Train,
                    false,
                );
            }
            Message::FortressSetOption {
                ident,
                option,
                state,
            } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let name = account.name.clone();
                if let Some(fi) = &mut account.fortress_info {
                    match option {
                        FortressOption::Collect => fi.auto_collect = state,
                        FortressOption::Gems => fi.auto_gems = state,
                        FortressOption::Build => fi.auto_build = state,
                        FortressOption::RecruitAll => fi.recruit_all = state,
                        FortressOption::SmartSoldiers => {
                            fi.smart_soldiers = state
                        }
                        FortressOption::CheapSkip => fi.cheap_skip = state,
                    }
                    fi.maintain_status = None;
                }
                if let Some(conf) =
                    self.config.get_char_conf_mut(&name, ident.server_id)
                {
                    match option {
                        FortressOption::Collect => {
                            conf.fortress_collect = state
                        }
                        FortressOption::Gems => conf.fortress_gems = state,
                        FortressOption::Build => conf.fortress_build = state,
                        FortressOption::RecruitAll => {
                            conf.fortress_recruit_all = state
                        }
                        FortressOption::SmartSoldiers => {
                            conf.fortress_smart_soldiers = state
                        }
                        FortressOption::CheapSkip => {
                            conf.fortress_cheap_skip = state
                        }
                    }
                    _ = self.config.write();
                }
                // Something got switched on - do a pass right away instead of
                // waiting for the timer
                if state {
                    return Command::perform(async {}, move |()| {
                        Message::FortressMaintainTick { ident }
                    });
                }
            }
            Message::UnderworldAutoCollect { ident, state } => {
                return self.set_underworld_auto(ident, Some(state), None);
            }
            Message::UnderworldAutoBuild { ident, state } => {
                return self.set_underworld_auto(ident, None, Some(state));
            }
            Message::UnderworldSetPhase { ident, phase } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let name = account.name.clone();
                if let Some(ui) = &mut account.underworld_info {
                    ui.phase = phase;
                }
                if let Some(conf) =
                    self.config.get_char_conf_mut(&name, ident.server_id)
                {
                    conf.underworld_phase = phase;
                    _ = self.config.write();
                }
            }
            Message::UnderworldMaintainTimer { ident } => {
                if let Some(ui) = self
                    .servers
                    .get_account_mut(&ident)
                    .and_then(|a| a.underworld_info.as_mut())
                {
                    ui.maint_steps = 0;
                }
                return self.underworld_maintain(ident);
            }
            Message::UnderworldMaintainTick { ident } => {
                return self.underworld_maintain(ident);
            }
            Message::FortressMaintainTimer { ident } => {
                if let Some(fi) = self
                    .servers
                    .get_account_mut(&ident)
                    .and_then(|a| a.fortress_info.as_mut())
                {
                    fi.maint_steps = 0;
                }
                return self.fortress_maintain(ident);
            }
            Message::FortressMaintainTick { ident } => {
                return self.fortress_maintain(ident);
            }
            Message::GuildSetRaidAuto { ident, state } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let name = account.name.clone();
                account.social.raid_auto = state;
                if let Some(conf) =
                    self.config.get_char_conf_mut(&name, ident.server_id)
                {
                    conf.guild_raid_auto = state;
                    _ = self.config.write();
                }
            }
            Message::AutoKickSet(v) => {
                self.config.auto_kick = v;
                _ = self.config.write();
            }
            Message::GuildInvite { ident, name } => {
                return self.send_simple(
                    ident,
                    "Guild",
                    sf_api::command::Command::GuildInvitePlayer {
                        name: name.clone(),
                    },
                    SimpleNote::GuildInvite(name),
                );
            }
            Message::GuildFinderRefresh { ident } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                account.social.finder_at = Some(Local::now());
                account.social.finder_status =
                    Some("Reading the guild finder...".to_string());
                return self.send_simple(
                    ident,
                    "Guild finder",
                    sf_api::command::Command::Custom {
                        cmd_name: "GroupJoinList".to_string(),
                        arguments: Vec::new(),
                    },
                    SimpleNote::GuildFinder,
                );
            }
            Message::GuildFinderStep { ident } => {
                return self.guild_finder_step(ident);
            }
            Message::GuildFinderJoin { ident, name } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                account.social.finder_tried.push(name.to_lowercase());
                // Both the name and the id, sent cold, got "server not
                // available" from a character without a guild (1.78 and
                // 1.80, live). The game client opens the guild's page
                // before ENTER, so that is done first; then the name, and
                // on the same refusal the id (see the refusal handler)
                account.social.finder_status =
                    Some(format!("Opening {name} before asking..."));
                return self.send_simple(
                    ident,
                    "Guild finder",
                    sf_api::command::Command::ViewGuild {
                        guild_ident: name.clone(),
                    },
                    SimpleNote::GuildJoinLook(name),
                );
            }
            Message::GuildSetFinderAuto { ident, state } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let name = account.name.clone();
                account.social.finder_auto = state;
                // Switched on: go now, not at the next half-hour tick
                account.social.finder_at = None;
                if let Some(conf) =
                    self.config.get_char_conf_mut(&name, ident.server_id)
                {
                    conf.guild_finder_auto = state;
                    _ = self.config.write();
                }
                if state {
                    return Command::perform(async {}, move |()| {
                        Message::GuildFinderRefresh { ident }
                    });
                }
            }
            Message::GuildKick { ident, name } => {
                if !self.config.admin_unlocked || !self.config.auto_kick {
                    return Command::none();
                }
                return self.send_simple(
                    ident,
                    "Guild",
                    sf_api::command::Command::GuildKickPlayer {
                        name: name.clone(),
                    },
                    SimpleNote::GuildKick(name),
                );
            }
            Message::GuildSetAuto {
                ident,
                defense,
                state,
            } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                if !state && account.social.confirm_off != Some(defense) {
                    // Switching this off can mean missing a raid. Ask first
                    account.social.confirm_off = Some(defense);
                    return Command::none();
                }
                account.social.confirm_off = None;
                let name = account.name.clone();
                if defense {
                    account.social.auto_defense = state;
                } else {
                    account.social.auto_attack = state;
                }
                if let Some(conf) =
                    self.config.get_char_conf_mut(&name, ident.server_id)
                {
                    if defense {
                        conf.guild_defense = state;
                    } else {
                        conf.guild_attack = state;
                    }
                    _ = self.config.write();
                }
                if state {
                    return Command::perform(async {}, move |()| {
                        Message::GuildAutoTick { ident }
                    });
                }
            }
            Message::GuardSetAuto { ident, state } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let name = account.name.clone();
                account.guard_info.auto = state;
                account.guard_info.status = None;
                if let Some(conf) =
                    self.config.get_char_conf_mut(&name, ident.server_id)
                {
                    conf.guard_auto = state;
                    _ = self.config.write();
                }
                if state {
                    return self.guard_step(ident);
                }
            }
            Message::TavernSetOption {
                ident,
                option,
                state,
            } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let name = account.name.clone();
                let ti = &mut account.tavern_info;
                match option {
                    TavernOption::Beer => ti.beer_auto = state,
                    TavernOption::Wheel => ti.wheel_auto = state,
                }
                if let Some(conf) =
                    self.config.get_char_conf_mut(&name, ident.server_id)
                {
                    match option {
                        TavernOption::Beer => conf.beer_auto = state,
                        TavernOption::Wheel => conf.wheel_auto = state,
                    }
                    _ = self.config.write();
                }
                if state {
                    let job = match option {
                        TavernOption::Beer => Job::Expedition,
                        _ => Job::Wheel,
                    };
                    return Command::perform(async {}, move |()| {
                        Message::Enqueue { ident, job }
                    });
                }
            }
            Message::SetBeerFree { ident, state } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let name = account.name.clone();
                account.tavern_info.beer_free = state;
                if let Some(conf) =
                    self.config.get_char_conf_mut(&name, ident.server_id)
                {
                    conf.beer_free = state;
                    _ = self.config.write();
                }
            }
            Message::SetMushroomReserve { ident, n } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let name = account.name.clone();
                account.tavern_info.mushroom_reserve = n;
                if let Some(conf) =
                    self.config.get_char_conf_mut(&name, ident.server_id)
                {
                    conf.mushroom_reserve = n;
                    _ = self.config.write();
                }
            }
            Message::SetMushroomBudget { ident, n } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let name = account.name.clone();
                account.tavern_info.mushroom_budget = n;
                // The budget is a mandate, not just a ceiling: the bot
                // works towards spending it - so many rerolls a shop, the
                // rest on paid dungeon fights (see `mechanics::mushroom_plan`)
                let plan = crate::mechanics::mushroom_plan(n);
                account.shop_info.rolls_per_day = plan.per_shop * 2;
                account.dungeon_info.mushroom = plan.dungeon > 0;
                account.dungeon_info.mushroom_max = plan.dungeon;
                // Beer is part of the plan now, and switched on with it.
                // It used to be a separate switch that did nothing while
                // the budget was zero, so "beer first" was a sentence in
                // the console and nothing in the game
                account.tavern_info.beer_max = plan.beer;
                account.tavern_info.beer_auto = plan.beer > 0;
                if let Some(conf) =
                    self.config.get_char_conf_mut(&name, ident.server_id)
                {
                    conf.mushroom_day_budget = n;
                    conf.shop_rolls_per_day = plan.per_shop * 2;
                    conf.dungeon_mushroom = plan.dungeon > 0;
                    conf.dungeon_mushroom_max = plan.dungeon;
                    conf.beer_max = plan.beer;
                    conf.beer_auto = plan.beer > 0;
                    _ = self.config.write();
                }
                self.console(
                    Some(ident),
                    if n == 0 {
                        "Mushrooms a day set to 0 - beer, shop rerolls, paid \
                         dungeon fights and skipped waits are all off. \
                         Nothing the bot does will cost a mushroom"
                            .to_string()
                    } else {
                        format!(
                            "Mushrooms a day set to {n}: {} beers, then up to \
                             {} rerolls of the weapon shop, {} of the magic \
                             shop and {} paid dungeon fights. What is left \
                             late in the day goes into beer rather than \
                             expiring at midnight",
                            plan.beer,
                            plan.per_shop,
                            plan.per_shop,
                            plan.dungeon
                        )
                    },
                    None,
                );
            }
            Message::BeerSetMax { ident, n } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let name = account.name.clone();
                account.tavern_info.beer_max = n;
                if let Some(conf) =
                    self.config.get_char_conf_mut(&name, ident.server_id)
                {
                    conf.beer_max = n;
                    _ = self.config.write();
                }
            }
            Message::WheelSetSpins { ident, n } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let name = account.name.clone();
                account.tavern_info.wheel_spins = n.clamp(1, 20);
                match self.config.get_char_conf_mut(&name, ident.server_id) {
                    Some(conf) => {
                        conf.wheel_spins = n.clamp(1, 20);
                        if let Err(e) = self.config.write() {
                            log::error!("wheel spins not saved: {e}");
                        }
                    }
                    // A character that is not in helper.toml (logged in
                    // without "remember me") keeps the value for this run
                    // only - say so instead of forgetting it in silence
                    None => {
                        if let Some(account) =
                            self.servers.get_account_mut(&ident)
                        {
                            account.tavern_info.wheel_status = Some(
                                "Not saved: this character is not in \
                                 helper.toml (log in with Remember me)"
                                    .to_string(),
                            );
                        }
                    }
                }
            }
            Message::WheelTick { ident } => {
                // A continuation only, never a manual click: stop when the
                // automation was switched off in the meantime
                if self
                    .servers
                    .get_account_mut(&ident)
                    .is_some_and(|a| !(a.tavern_info.wheel_auto))
                {
                    return Command::none();
                }
                return self.wheel_step(ident);
            }
            Message::DailiesSetAuto { ident, state } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let name = account.name.clone();
                account.dailies_info.auto = state;
                account.dailies_info.status = None;
                if let Some(conf) =
                    self.config.get_char_conf_mut(&name, ident.server_id)
                {
                    conf.dailies_auto = state;
                    _ = self.config.write();
                }
            }
            Message::MountSetAuto { ident, state } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let name = account.name.clone();
                account.dailies_info.mount_auto = state;
                if let Some(conf) =
                    self.config.get_char_conf_mut(&name, ident.server_id)
                {
                    conf.mount_auto = state;
                    _ = self.config.write();
                }
            }
            Message::GemToggleWanted { ident, gem } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let name = account.name.clone();
                let wanted = match &mut account.fortress_info {
                    Some(fi) => {
                        if let Some(at) =
                            fi.gem_wanted.iter().position(|g| *g == gem)
                        {
                            fi.gem_wanted.remove(at);
                        } else {
                            fi.gem_wanted.push(gem);
                        }
                        fi.gem_wanted.clone()
                    }
                    None => return Command::none(),
                };
                if let Some(conf) =
                    self.config.get_char_conf_mut(&name, ident.server_id)
                {
                    conf.gem_wanted = wanted;
                    _ = self.config.write();
                }
            }
            Message::DailiesTick { ident } => {
                // A continuation only: stop if it was switched off meanwhile
                if self
                    .servers
                    .get_account_mut(&ident)
                    .is_some_and(|a| !a.dailies_info.auto)
                {
                    return Command::none();
                }
                return self.dailies_step(ident);
            }
            Message::DailiesTimer { ident } => {
                if let Some(account) = self.servers.get_account_mut(&ident) {
                    account.dailies_info.steps_this_round = 0;
                }
                return self.dailies_step(ident);
            }
            Message::DiceSetAuto { ident, state } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let name = account.name.clone();
                account.tavern_info.dice_auto = state;
                account.tavern_info.dice_status = None;
                account.tavern_info.dice_stage = 0;
                if let Some(conf) =
                    self.config.get_char_conf_mut(&name, ident.server_id)
                {
                    conf.dice_auto = state;
                    _ = self.config.write();
                }
                if state {
                    return Command::perform(async {}, move |()| {
                        Message::Enqueue {
                            ident,
                            job: Job::Dice,
                        }
                    });
                }
            }
            Message::DiceSetSkip { ident, state } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let name = account.name.clone();
                account.tavern_info.dice_skip = state;
                if let Some(conf) =
                    self.config.get_char_conf_mut(&name, ident.server_id)
                {
                    conf.dice_skip = state;
                    _ = self.config.write();
                }
            }
            Message::DiceTick { ident } => {
                if !self.burst_active() {
                    return Command::none();
                }
                if self
                    .servers
                    .get_account_mut(&ident)
                    .is_some_and(|a| !a.tavern_info.dice_auto)
                {
                    return Command::none();
                }
                return self.dice_step(ident);
            }
            Message::DiceTimer { ident } => {
                if !self.burst_active() {
                    return Command::none();
                }
                if let Some(account) = self.servers.get_account_mut(&ident) {
                    account.tavern_info.dice_this_round = 0;
                }
                return self.dice_step(ident);
            }
            Message::WheelTimer { ident } => {
                if !self.burst_active() {
                    return Command::none();
                }
                if let Some(account) = self.servers.get_account_mut(&ident) {
                    account.tavern_info.spins_this_round = 0;
                }
                return self.wheel_step(ident);
            }
            Message::IdleSetSacrifice { ident, state } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let name = account.name.clone();
                account.idle_info.auto_sacrifice = state;
                if let Some(conf) =
                    self.config.get_char_conf_mut(&name, ident.server_id)
                {
                    conf.idle_sacrifice = state;
                    _ = self.config.write();
                }
            }
            Message::IdleSetSacrificeStep { ident, step } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let name = account.name.clone();
                account.idle_info.sacrifice_gain_pct = step.max(1);
                if let Some(conf) =
                    self.config.get_char_conf_mut(&name, ident.server_id)
                {
                    conf.idle_sacrifice_gain_pct = step.max(1);
                    _ = self.config.write();
                }
            }
            Message::FullBot { ident, on } => {
                return self.set_full_bot(ident, on);
            }
            Message::SacrificeAsk { ident, pos } => {
                if let Some(account) = self.servers.get_account_mut(&ident) {
                    account.shop_info.sacrifice_ask = Some(pos);
                }
            }
            Message::GuildFindTargets { ident } => {
                return self.send_simple(
                    ident,
                    "Guild",
                    sf_api::command::Command::GuildGetFightableTargets,
                    SimpleNote::GuildAction("Looked up the guilds that can be attacked".into()),
                );
            }
            Message::GuildDeclareAttack { ident, guild } => {
                return self.send_simple(
                    ident,
                    "Guild",
                    sf_api::command::Command::GuildAttack {
                        guild: guild.clone(),
                    },
                    SimpleNote::GuildAction(format!("Declared an attack on {guild}")),
                );
            }
            Message::GuildLeaderTools(state) => {
                self.config.guild_leader_tools = state;
                _ = self.config.write();
            }
            Message::GuildTwoColumns(state) => {
                self.config.guild_two_columns = state;
                _ = self.config.write();
            }
            Message::SacrificeCancel { ident } => {
                if let Some(account) = self.servers.get_account_mut(&ident) {
                    account.shop_info.sacrifice_ask = None;
                }
            }
            Message::SacrificeConfirm { ident } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let Some(pos) = account.shop_info.sacrifice_ask.take() else {
                    return Command::none();
                };
                return self.sacrifice_step(ident, pos, false);
            }
            Message::SacrificeNext { ident, pos } => {
                return self.sacrifice_step(ident, pos, true);
            }
            Message::GuardSetHours { ident, hours } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let name = account.name.clone();
                // Stored as typed; the shift itself clamps to 1..=10 when
                // it starts. Clamping here made the box untypeable: clear
                // it and it says 1, type 3 and it says 13
                account.guard_info.hours = hours.min(10);
                if let Some(conf) =
                    self.config.get_char_conf_mut(&name, ident.server_id)
                {
                    conf.guard_hours = hours.min(10);
                    _ = self.config.write();
                }
            }
            Message::GuardSetCutoffEnabled { ident, state } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let name = account.name.clone();
                account.guard_info.cutoff_enabled = state;
                if let Some(conf) =
                    self.config.get_char_conf_mut(&name, ident.server_id)
                {
                    conf.guard_cutoff_enabled = state;
                    _ = self.config.write();
                }
            }
            Message::GuardSetCutoff { ident, text } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let name = account.name.clone();
                account.guard_info.cutoff = text.clone();
                if chrono::NaiveTime::parse_from_str(text.trim(), "%H:%M")
                    .is_ok()
                    && let Some(conf) =
                        self.config.get_char_conf_mut(&name, ident.server_id)
                {
                    conf.guard_cutoff = text;
                    _ = self.config.write();
                }
            }
            Message::DungeonSetPetsFirst { ident, state } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let name = account.name.clone();
                account.dungeon_info.pets_first = state;
                if let Some(conf) =
                    self.config.get_char_conf_mut(&name, ident.server_id)
                {
                    conf.dungeon_pets_first = state;
                    _ = self.config.write();
                }
            }
            Message::GuardTimer { ident } => {
                return self.guard_step(ident);
            }
            Message::GuardCancel { ident } => {
                return self.send_simple(
                    ident,
                    "Guard",
                    sf_api::command::Command::CancelWork,
                    SimpleNote::Guard("Shift cancelled".to_string()),
                );
            }
            Message::GuildLookupEnemies { ident } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                // Which battle partner do we not know yet?
                let (todo, names) = {
                    let arc = account.status.clone();
                    let lock = arc.lock_safe();
                    match &*lock {
                        AccountStatus::Idle(_, gs) => {
                            let names: Vec<String> =
                                gs.lookup.guilds.keys().cloned().collect();
                            let ids = gs
                                .guild
                                .as_ref()
                                .map(|g| {
                                    [g.attacking.as_ref(), g.defending.as_ref()]
                                        .into_iter()
                                        .flatten()
                                        .filter(|b| !b.is_raid())
                                        .map(|b| b.other)
                                        .collect::<Vec<u32>>()
                                })
                                .unwrap_or_default();
                            let todo = ids.into_iter().find(|id| {
                                !account
                                    .social
                                    .enemies
                                    .iter()
                                    .any(|e| e.id == *id)
                            });
                            (todo, names)
                        }
                        _ => (None, Vec::new()),
                    }
                };
                let Some(id) = todo else {
                    return Command::none();
                };
                account.social.lookup_before = names;
                return self.send_simple(
                    ident,
                    "Guild",
                    sf_api::command::Command::ViewGuild {
                        guild_ident: id.to_string(),
                    },
                    SimpleNote::GuildLookup { id: Some(id) },
                );
            }
            Message::GuildCompareInput { ident, text } => {
                if let Some(account) = self.servers.get_account_mut(&ident) {
                    account.social.compare_input = text;
                }
            }
            Message::GuildCompare { ident } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let name = account.social.compare_input.trim().to_string();
                if name.is_empty() {
                    return Command::none();
                }
                let names: Vec<String> = {
                    let arc = account.status.clone();
                    let lock = arc.lock_safe();
                    match &*lock {
                        AccountStatus::Idle(_, gs)
                        | AccountStatus::Busy(gs, _) => {
                            // A repeated lookup of the same guild must count
                            // as new, so drop the old copy from the snapshot
                            gs.lookup
                                .guilds
                                .keys()
                                .filter(|k| !k.eq_ignore_ascii_case(&name))
                                .cloned()
                                .collect()
                        }
                        _ => Vec::new(),
                    }
                };
                account.social.lookup_before = names;
                account.social.compare_result =
                    Some("Looking up...".to_string());
                return self.send_simple(
                    ident,
                    "Guild",
                    sf_api::command::Command::ViewGuild { guild_ident: name },
                    SimpleNote::GuildLookup { id: None },
                );
            }
            Message::GuildAutoCancelOff { ident } => {
                if let Some(account) = self.servers.get_account_mut(&ident) {
                    account.social.confirm_off = None;
                }
            }
            Message::GuildAutoTick { ident } => {
                if let Some(account) = self.servers.get_account_mut(&ident) {
                    account.social.sign_attempts = 0;
                }
                return Command::perform(async {}, move |()| {
                    Message::GuildRefresh { ident }
                });
            }
            Message::GuildStartRaid { ident } => {
                return self.send_simple(
                    ident,
                    "Raid",
                    sf_api::command::Command::GuildRaid,
                    SimpleNote::GuildRaid,
                );
            }
            Message::FortressSetSoldierMargin { ident, margin } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let name = account.name.clone();
                if let Some(fi) = &mut account.fortress_info {
                    fi.soldier_margin = margin;
                }
                if let Some(conf) =
                    self.config.get_char_conf_mut(&name, ident.server_id)
                {
                    conf.fortress_soldier_margin = margin;
                    _ = self.config.write();
                }
            }
            Message::FortressSetAttackSoldiers { ident, count } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let name = account.name.clone();
                if let Some(fi) = &mut account.fortress_info {
                    fi.attack_soldiers = count;
                }
                if let Some(conf) =
                    self.config.get_char_conf_mut(&name, ident.server_id)
                {
                    conf.fortress_attack_soldiers = count;
                    _ = self.config.write();
                }
            }
            Message::DungeonSetAuto { ident, state } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let name = account.name.clone();
                account.dungeon_info.auto = state;
                if let Some(conf) =
                    self.config.get_char_conf_mut(&name, ident.server_id)
                {
                    conf.dungeon_auto = state;
                    _ = self.config.write();
                }
                if state {
                    return Command::perform(async {}, move |()| {
                        Message::AutoDungeonTick { ident }
                    });
                }
            }
            Message::DungeonSetMinChance { ident, percent } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let name = account.name.clone();
                account.dungeon_info.min_chance = percent.min(100);
                if let Some(conf) =
                    self.config.get_char_conf_mut(&name, ident.server_id)
                {
                    conf.dungeon_min_chance = percent.min(100);
                    _ = self.config.write();
                }
            }
            Message::DungeonSetMushroom { ident, state } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let name = account.name.clone();
                account.dungeon_info.mushroom = state;
                if let Some(conf) =
                    self.config.get_char_conf_mut(&name, ident.server_id)
                {
                    conf.dungeon_mushroom = state;
                    _ = self.config.write();
                }
            }
            Message::DungeonSetMushroomMin { ident, percent } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let name = account.name.clone();
                account.dungeon_info.mushroom_min = percent.min(100);
                if let Some(conf) =
                    self.config.get_char_conf_mut(&name, ident.server_id)
                {
                    conf.dungeon_mushroom_min = percent.min(100);
                    _ = self.config.write();
                }
            }
            Message::DungeonSetPortal { ident, state } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let name = account.name.clone();
                account.dungeon_info.portal_auto = state;
                if let Some(conf) =
                    self.config.get_char_conf_mut(&name, ident.server_id)
                {
                    conf.dungeon_portal = state;
                    _ = self.config.write();
                }
                if state {
                    return Command::perform(async {}, move |()| {
                        Message::AutoDungeonTick { ident }
                    });
                }
            }
            Message::ArenaSetMushroom { ident, state } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let name = account.name.clone();
                if let Some(si) = &mut account.scrapbook_info {
                    si.mushroom = state;
                }
                if let Some(conf) =
                    self.config.get_char_conf_mut(&name, ident.server_id)
                {
                    conf.arena_mushroom = state;
                    _ = self.config.write();
                }
            }
            Message::AutoDungeonTick { ident } => {
                // Fresh data first; the compute step then picks the fight
                return Command::perform(async {}, move |()| {
                    Message::DungeonRefresh {
                        ident,
                        force: false,
                    }
                });
            }
            Message::PetSimHabitat { .. }
            | Message::PetSimTheirPet { .. }
            | Message::PetSimField { .. } => {
                self.pet_sim_input(message);
            }
            Message::PetSetAutoArena { ident, state } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let name = account.name.clone();
                account.pet_info.auto_arena = state;
                account.pet_info.arena_status = None;
                if let Some(conf) =
                    self.config.get_char_conf_mut(&name, ident.server_id)
                {
                    conf.pets_arena = state;
                    _ = self.config.write();
                }
                if state {
                    return Command::perform(async {}, move |()| {
                        Message::Enqueue {
                            ident,
                            job: Job::Pets,
                        }
                    });
                }
            }
            Message::PetSetAutoFeed { ident, state } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let name = account.name.clone();
                account.pet_info.auto_feed = state;
                account.pet_info.status = None;
                if let Some(conf) =
                    self.config.get_char_conf_mut(&name, ident.server_id)
                {
                    conf.pets_feed = state;
                    _ = self.config.write();
                }
                if state {
                    account.pet_info.fed_this_round = 0;
                    return Command::perform(async {}, move |()| {
                        Message::PetFeedTick { ident, attempt: 0 }
                    });
                }
            }
            Message::PetFeedTick { ident, attempt } => {
                // Continuation: stop once the automation is off, unless this
                // is the first step of a manual "feed now"
                if attempt > 0
                    && self
                        .servers
                        .get_account_mut(&ident)
                        .is_some_and(|a| !a.pet_info.auto_feed)
                {
                    return Command::none();
                }
                return self.pet_feed(ident, attempt);
            }
            Message::IdleSetAuto { ident, state } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let name = account.name.clone();
                account.idle_info.auto_upgrade = state;
                account.idle_info.status = None;
                if let Some(conf) =
                    self.config.get_char_conf_mut(&name, ident.server_id)
                {
                    conf.idle_upgrade = state;
                    _ = self.config.write();
                }
                if state {
                    return Command::perform(async {}, move |()| {
                        Message::IdleTimer { ident }
                    });
                }
            }
            Message::ExpeditionSetOption {
                ident,
                option,
                state,
            } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let name = account.name.clone();
                let ei = &mut account.expedition_info;
                match option {
                    ExpeditionOption::Auto => ei.auto = state,
                    ExpeditionOption::Gold => {
                        ei.goal = if state {
                            crate::expedition::ExpeditionGoal::Gold
                        } else {
                            crate::expedition::ExpeditionGoal::Xp
                        }
                    }
                    ExpeditionOption::SkipGlass => ei.skip_glass = state,
                }
                ei.status = None;
                if let Some(conf) =
                    self.config.get_char_conf_mut(&name, ident.server_id)
                {
                    match option {
                        ExpeditionOption::Auto => conf.expedition_auto = state,
                        ExpeditionOption::Gold => conf.expedition_gold = state,
                        ExpeditionOption::SkipGlass => {
                            conf.expedition_skip_glass = state
                        }
                    }
                    _ = self.config.write();
                }
                if option == ExpeditionOption::Auto && state {
                    return Command::perform(async {}, move |()| {
                        Message::ExpeditionTimer { ident }
                    });
                }
            }
            Message::ExpeditionTick { ident } => {
                return self.expedition_step(ident);
            }
            Message::ExpeditionTimer { ident } => {
                if let Some(account) = self.servers.get_account_mut(&ident) {
                    account.expedition_info.steps_this_round = 0;
                    account.expedition_info.checks_in_a_row = 0;
                }
                return self.expedition_step(ident);
            }
            Message::IdleTick { ident } => {
                // A continuation only, never a manual click: stop when the
                // automation was switched off in the meantime
                if self.servers.get_account_mut(&ident).is_some_and(|a| {
                    !(a.idle_info.auto_upgrade
                        || a.idle_info.auto_collect
                        || a.idle_info.auto_sacrifice)
                }) {
                    return Command::none();
                }
                return self.idle_upgrade(ident);
            }
            Message::IdleTimer { ident } => {
                if let Some(account) = self.servers.get_account_mut(&ident) {
                    account.idle_info.bought_this_round = 0;
                }
                return self.idle_upgrade(ident);
            }
            Message::PetFeedTimer { ident } => {
                if let Some(account) = self.servers.get_account_mut(&ident) {
                    account.pet_info.fed_this_round = 0;
                    account.pet_info.last_feed = None;
                }
                return self.pet_feed(ident, 0);
            }
            Message::FortressSetTrain { ident, state } => {
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                if let Some(fi) = &mut account.fortress_info {
                    fi.auto_train = state;
                }
            }
            Message::FortressActionFailed {
                ident,
                session,
                action,
                error,
            } => {
                // A dead session is not a refused fortress action - it is
                // every request from here on. It used to be logged as
                // "Fortress Train: sessionid invalid" and the dead session
                // handed straight back to the next job, so the character
                // sat there failing at everything while the other one on
                // the account worked fine
                if session_is_dead(&error) {
                    self.console(
                        Some(ident),
                        "Session lost - somebody logged into this character \
                         elsewhere (browser, another device). The game keeps \
                         one session, so this app logs in again in a moment; \
                         the other window will be logged out"
                            .to_string(),
                        Some(false),
                    );
                    return Command::perform(async {}, move |()| {
                        Message::PlayerCommandFailed {
                            ident,
                            session,
                            attempt: 0,
                        }
                    });
                }
                self.console(
                    Some(ident),
                    format!("Fortress {action:?}: {error}"),
                    Some(false),
                );
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                let arc = account.status.clone();
                let mut lock = arc.lock_safe();
                lock.put_session(session);
                drop(lock);

                let what = match action {
                    FortressActionKind::Attack => "Attack",
                    FortressActionKind::Reroll
                    | FortressActionKind::PaidReroll => "New opponent",
                    FortressActionKind::Train => "Recruiting",
                };
                warn!("Fortress action refused for {ident}: {error}");
                let more_units =
                    error.to_lowercase().contains("need more units");
                let error = friendly_error(&error);
                if let Some(fi) = &mut account.fortress_info {
                    fi.log.push((
                        Local::now(),
                        format!("{what} refused: {error}"),
                        false,
                    ));
                    fi.status = Some(format!("{what} refused: {error}"));
                    // A refusal in a raid run ends the run. The old answer
                    // to "need more units" was to send everybody, which
                    // is the one thing a run never does; the barracks are
                    // still full, so the cycle draws a new moment and
                    // comes back to it
                    if matches!(fi.raid, crate::player::RaidPhase::Raiding { .. })
                    {
                        fi.raid = crate::player::RaidPhase::Filling;
                        fi.next_due = None;
                        fi.retried_full = false;
                        return Command::none();
                    }
                    // The game's own recommendation was not enough for the
                    // game itself - send everybody once, straight away
                    if action == FortressActionKind::Attack
                        && more_units
                        && !fi.retried_full
                    {
                        fi.retried_full = true;
                        fi.scouted_soldiers = Some((
                            u32::MAX,
                            "the recommendation was refused - all soldiers"
                                .to_string(),
                        ));
                        fi.status = Some(
                            "The game wanted more soldiers than it advised - \
                             attacking again with everybody"
                                .to_string(),
                        );
                        return Command::perform(
                            async {
                                sleep(Duration::from_secs(3)).await;
                            },
                            move |()| Message::FortressAttack { ident },
                        );
                    }
                    fi.retried_full = false;
                }
            }
            Message::FortressActionResult {
                ident,
                session,
                resp,
                action,
            } => {
                self.console(
                    Some(ident),
                    format!("Fortress {action:?}"),
                    Some(true),
                );
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };

                let arc = account.status.clone();
                let mut lock = arc.lock_safe();
                let AccountStatus::Busy(gs, _) = &mut *lock else {
                    return Command::none();
                };

                // `last_fight` is the last fight the game reported at all -
                // an arena battle a minute ago counts. If the fortress attack
                // does not happen, it stays untouched and reading it means
                // reporting somebody else's result as this attack's. So take
                // a fingerprint first and see whether it moves
                let fight_before = fight_fingerprint(gs);
                let soldiers_before = gs.fortress.as_ref().map(|f| {
                    f.units
                        [sf_api::gamestate::fortress::FortressUnitType::Soldier]
                        .count
                });
                let loot_before = fortress_stock(gs);

                if let Err(e) = gs.update(*resp) {
                    *lock = AccountStatus::FatalError(e.to_string());
                    return Command::none();
                }

                let fight_after = fight_fingerprint(gs);
                let soldiers_after = gs.fortress.as_ref().map(|f| {
                    f.units
                        [sf_api::gamestate::fortress::FortressUnitType::Soldier]
                        .count
                });
                let loot_after = fortress_stock(gs);
                // What the raid brought home. The stock can only grow from an
                // attack, so a drop (a building started) is reported as zero
                let looted = (
                    loot_after.0.saturating_sub(loot_before.0),
                    loot_after.1.saturating_sub(loot_before.1),
                );
                // Either a new fight was reported, or the attack cost
                // soldiers. Without one of the two, nothing happened
                let fought = (fight_after.is_some()
                    && fight_after != fight_before)
                    || (soldiers_before != soldiers_after);
                let outcome = fought
                    .then(|| gs.last_fight.as_ref().map(|a| a.has_player_won))
                    .flatten();
                // The session always goes back, no matter what the response
                // looked like
                lock.put_session(session);
                drop(lock);

                let mut train_next = false;
                if let Some(fi) = &mut account.fortress_info {
                    fi.status = None;
                    fi.retried_full = false;
                    match action {
                        FortressActionKind::Attack => {
                            let haul = match looted {
                                (0, 0) => String::new(),
                                (w, 0) => format!(" - {w} wood"),
                                (0, st) => format!(" - {st} stone"),
                                (w, st) => format!(" - {w} wood, {st} stone"),
                            };
                            let (msg, ok) = match outcome {
                                Some(true) => ("Attack won", true),
                                Some(false) => ("Attack lost", false),
                                // The command went through but the game
                                // reported no fight and lost no soldiers.
                                // Saying "won" here is how the log ended up
                                // full of attacks that never happened
                                None => (
                                    "The server did not report a fight - \
                                     nothing happened",
                                    false,
                                ),
                            };
                            if outcome == Some(true) {
                                Self::push_success(
                                    &mut self.successes,
                                    &account.name,
                                    format!("Fortress attack won{haul}"),
                                );
                            }
                            if fought {
                                fi.last_attack = Some(Local::now());
                                // In a run, the attacks are counted and
                                // the next one follows after a pause a
                                // person takes between two fights. The
                                // old clock ("about every x minutes") is
                                // gone: the run ends when the soldiers do
                                if let crate::player::RaidPhase::Raiding {
                                    attacks,
                                    ..
                                } = &mut fi.raid
                                {
                                    *attacks = attacks.saturating_add(1);
                                }
                            } else {
                                log::warn!(
                                    "{ident}: fortress attack sent but the \
                                     game reported no fight"
                                );
                            }
                            fi.log.push((
                                Local::now(),
                                format!("{msg}{haul}"),
                                ok,
                            ));
                            // A lost fight costs soldiers just like a won one,
                            // so the replacement gets queued either way -
                            // outside a run; a run refills in one go at
                            // the end
                            train_next = fought
                                && fi.auto_train
                                && !matches!(
                                    fi.raid,
                                    crate::player::RaidPhase::Raiding { .. }
                                );
                        }
                        FortressActionKind::Reroll
                        | FortressActionKind::PaidReroll => fi.log.push((
                            Local::now(),
                            "New opponent".to_string(),
                            true,
                        )),
                        FortressActionKind::Train => fi.log.push((
                            Local::now(),
                            "Soldier queued".to_string(),
                            true,
                        )),
                    }
                    // A run goes on by itself: the next opponent after the
                    // pause a person takes between two fights, the refill
                    // once the soldiers are gone. The tick sorts out which
                    if matches!(fi.raid, crate::player::RaidPhase::Raiding { .. })
                    {
                        return Command::perform(
                            async {
                                crate::pace::sleep_between(8_000, 40_000).await;
                            },
                            move |()| Message::FortressRaidStep { ident },
                        );
                    }
                }

                if train_next {
                    return Command::perform(
                        async {
                            crate::pace::sleep_between(1500, 3000).await;
                        },
                        move |()| Message::FortressTrain { ident },
                    );
                }
            }
            Message::ConfigSetAutoFortress { name, server, nv } => {
                if let Some(config) =
                    self.config.get_char_conf_mut(&name, server)
                {
                    config.auto_fortress = nv;
                    _ = self.config.write();
                }
            }
            Message::PlayerRelogSuccess { ident, gs, session } => {
                info!("Relogin success");
                let Some(server) = self.servers.0.get_mut(&ident.server_id)
                else {
                    return Command::none();
                };
                let Some(player) = server.accounts.get_mut(&ident.account)
                else {
                    return Command::none();
                };

                let mut lock = player.status.lock_safe();
                *lock = AccountStatus::Busy(gs, "Waiting".into());
                drop(lock);
                // For some reason the game does not like sending requests
                // immediately
                return Command::perform(
                    async {
                        sleep(Duration::from_secs(10)).await;
                    },
                    move |_| Message::PlayerRelogDelay { ident, session },
                );
            }
            Message::SSOLoginFailure { name, error } => {
                self
                    .login_state
                    .active_sso
                    .retain(|a| !matches!(&a.ident, SSOIdent::SF(s) if s.as_str() == name.as_str()));
                self.login_state.login_typ = LoginType::SFAccount;
                self.login_state.error = Some(error)
            }
            Message::PlayerLure { ident, target } => {
                let Some(server) = self.servers.get_mut(&ident.server_id)
                else {
                    return Command::none();
                };
                let Some(account) = server.accounts.get_mut(&ident.account)
                else {
                    return Command::none();
                };
                let Some(ud) = &account.underworld_info else {
                    return Command::none();
                };
                if crate::mechanics::remaining_lures(&ud.underworld) == 0 {
                    return Command::none();
                }

                let CrawlingStatus::Crawling { .. } = &server.crawling else {
                    return Command::none();
                };

                let mut status = account.status.lock_safe();
                let Some(mut session) = status.take_session("Luring") else {
                    return Command::none();
                };
                drop(status);
                let ident = account.ident;
                let tid = target.uid;
                return Command::perform(
                    async move {
                        let cmd = sf_api::command::Command::UnderworldAttack {
                            player_id: tid,
                        };
                        let resp = session.send_command(&cmd).await;
                        (resp, session)
                    },
                    move |r| match r.0 {
                        Ok(resp) => Message::PlayerLureResult {
                            ident,
                            session: r.1,
                            against: target,
                            resp: Box::new(resp),
                        },
                        Err(_) => Message::PlayerCommandFailed {
                            ident,
                            session: r.1,
                            attempt: 0,
                        },
                    },
                );
            }
            Message::PlayerLureResult {
                ident,
                session,
                against,
                resp,
            } => {
                let Some(server) = self.servers.0.get_mut(&ident.server_id)
                else {
                    return Command::none();
                };
                let Some(account) = server.accounts.get_mut(&ident.account)
                else {
                    return Command::none();
                };

                let v = account.status.clone();
                let mut lock = v.lock_safe();

                let AccountStatus::Busy(s, _) = &mut *lock else {
                    return Command::none();
                };

                if let Err(e) = s.update(*resp) {
                    // it would *probably* be ok to just ignore this in most
                    // cases, but whatever
                    *lock = AccountStatus::FatalError(e.to_string());
                    return Command::none();
                };

                let won = s.last_fight.as_ref().map(|a| a.has_player_won);
                let underworld = s.underworld.clone();
                // The session goes back before any early return, so that a
                // weird response can not permanently block the character
                lock.put_session(session);
                drop(lock);

                let Some(si) = &mut account.underworld_info else {
                    return Command::none();
                };

                if let Some(won) = won {
                    si.attack_log.push((Local::now(), against.name, won));
                }

                if let Some(underworld) = underworld {
                    si.underworld = underworld;
                }
            }
            Message::PlayerSetMaxUndergroundLvl { ident, lvl } => {
                let Some(server) = self.servers.get_mut(&ident.server_id)
                else {
                    return Command::none();
                };
                let Some(account) = server.accounts.get_mut(&ident.account)
                else {
                    return Command::none();
                };
                let Some(si) = &mut account.underworld_info else {
                    return Command::none();
                };
                si.max_level = lvl;
                return self.update_best(ident, false);
            }
            Message::UpdateResult(should_update) => {
                self.should_update = should_update;
            }
            Message::AdvancedLevelRestrict(val) => {
                self.config.show_crawling_restrict = val;
                _ = self.config.write();
            }
            Message::CrawlerSetMinMax { server, min, max } => {
                let Some(server) = self.servers.get_mut(&server) else {
                    return Command::none();
                };
                if let CrawlingStatus::Crawling { que, .. } = &server.crawling {
                    let mut que = que.lock_safe();
                    que.min_level = min.max(1);
                    que.max_level = max.max(min).min(9999);

                    debug!(
                        "Changed MinMax to {}/{}",
                        que.min_level, que.max_level
                    );
                    let mut to_remove = IntSet::default();
                    for (lvl, _) in
                        que.lvl_skipped_accounts.range(0..que.min_level)
                    {
                        to_remove.insert(*lvl);
                    }
                    for (lvl, _) in
                        que.lvl_skipped_accounts.range(que.max_level + 1..)
                    {
                        to_remove.insert(*lvl);
                    }
                    for lvl in to_remove {
                        let Some(mut todo) =
                            que.lvl_skipped_accounts.remove(&lvl)
                        else {
                            continue;
                        };
                        que.todo_accounts.append(&mut todo);
                    }
                }
            }
            Message::ShowClasses(val) => {
                self.config.show_class_icons = val;
                _ = self.config.write();
            }
            Message::NextCLICrawling => {
                let Some(cli) = &mut self.cli_crawling else {
                    return Command::none();
                };
                let pb = cli.mbp.add(ProgressBar::new_spinner());

                let Some(url) = cli.todo_servers.pop() else {
                    cli.active -= 1;
                    if cli.active == 0 {
                        pb.println("Finished Crawling all servers");
                        pb.finish_and_clear();
                        std::process::exit(0);
                    }
                    pb.finish_and_clear();
                    return Command::none();
                };
                let threads = cli.threads;
                return match self.force_init_crawling(&url, threads, pb.clone())
                {
                    Some(s) => s,
                    None => {
                        pb.println(format!(
                            "Could not init crawling on: {url}"
                        ));
                        pb.finish_and_clear();
                        return Command::perform(async {}, |_| {
                            Message::NextCLICrawling
                        });
                    }
                };
            }
            Message::CrawlAllRes {
                servers,
                concurrency,
            } => {
                let Some(cli) = &mut self.cli_crawling else {
                    return Command::none();
                };
                let Some(servers) = servers else {
                    _ = cli.mbp.println("Could not fetch server list");
                    std::process::exit(1);
                };
                cli.todo_servers = servers;
                let mut res = vec![];
                for _ in 0..concurrency {
                    res.push(Command::perform(async {}, |_| {
                        Message::NextCLICrawling
                    }))
                }
                return Command::batch(res);
            }
            Message::FontLoaded(_) => {}
            Message::PlayerRelogDelay { ident, session } => {
                let Some(server) = self.servers.get_mut(&ident.server_id)
                else {
                    return Command::none();
                };
                let Some(account) = server.accounts.get_mut(&ident.account)
                else {
                    return Command::none();
                };

                let mut lock = account.status.lock_safe();
                lock.put_session(session);
                drop(lock);
                // A fresh session, so the arena's three are re-read - the
                // ones on the page belong to the session that is gone
                return Command::perform(
                    async {
                        sleep(Duration::from_secs(5)).await;
                    },
                    move |()| Message::ArenaOfferRefresh {
                        ident,
                        then_fight: false,
                    },
                );
            }
            Message::SSOImportAuto { ident } => {
                let i_name = ident.name.to_lowercase();
                let i_server = ServerIdent::new(&ident.server);

                let pos = self.login_state.import_que.iter().position(|char| {
                    let server = ServerIdent::new(char.server_url().as_str());
                    let name = char.username().to_lowercase();
                    server == i_server && name == i_name
                });
                let Some(pos) = pos else {
                    return Command::none();
                };
                let account = self.login_state.import_que.remove(pos);
                return self.login(account, false, PlayerAuth::SSO, true);
            }
            Message::SetOverviewSelected { ident, val } => {
                let View::Overview { selected, action } =
                    &mut self.current_view
                else {
                    return Command::none();
                };
                *action = None;
                if val {
                    for v in ident {
                        selected.insert(v);
                    }
                } else {
                    for v in ident {
                        selected.remove(&v);
                    }
                }
            }
            Message::ConfigSetAutoLogin { name, server, nv } => {
                let Some(config) = self.config.get_char_conf_mut(&name, server)
                else {
                    return Command::none();
                };
                config.login = nv;
                _ = self.config.write();
            }
            Message::ConfigSetAutoBattle { name, server, nv } => {
                let Some(config) = self.config.get_char_conf_mut(&name, server)
                else {
                    return Command::none();
                };
                config.auto_battle = nv;
                _ = self.config.write();
            }
            Message::SetBlacklistThr(nv) => {
                self.config.blacklist_threshold = nv.max(1);
                _ = self.config.write();
            }
            Message::AutoLureIdle => {}
            Message::AutoLurePossible { ident } => {
                let refetch = self.update_best(ident, true);

                let Some(server) = self.servers.0.get_mut(&ident.server_id)
                else {
                    return Command::none();
                };
                let Some(account) = server.accounts.get_mut(&ident.account)
                else {
                    return Command::none();
                };

                let CrawlingStatus::Crawling { .. } = &server.crawling else {
                    return Command::none();
                };

                let mut status = account.status.lock_safe();
                let AccountStatus::Idle(_, gs) = &*status else {
                    return refetch;
                };

                // The daily cap is the Gate's level, not a flat five
                let can_lure = gs
                    .underworld
                    .as_ref()
                    .is_some_and(|u| crate::mechanics::remaining_lures(u) > 0);
                if !can_lure {
                    return refetch;
                }
                // Read before the session is taken out of the lock
                let wanted_level =
                    gs.underworld.as_ref().map_or(0, |u| u.lure_level);
                // The Keeper decides the fight. 20 levels under it, or 30
                // once a lure has been lost in the last day
                let keeper = gs.underworld.as_ref().map_or(0, |u| {
                    u.units[sf_api::gamestate::underworld::UnderworldUnitType::Keeper]
                        .level
                });

                let Some(mut session) = status.take_session("Luring") else {
                    return refetch;
                };

                let Some(ui) = &account.underworld_info else {
                    status.put_session(session);
                    return refetch;
                };

                // There used to be a gate here that wanted nine tenths of
                // the crawled list to have been looked at today before it
                // would lure at all. On a list of a few thousand that
                // share never arrives, so the answer was always "not yet"
                // and nothing was ever lured - which is exactly what it
                // looked like from the outside. One fresh target is all a
                // lure needs, and `pick_lure_target` returning `None` is
                // what puts the crawler to work
                let lost_lately = ui.attack_log.iter().rev().take(10).any(
                    |(at, _, won)| {
                        !won && Local::now() - *at < chrono::Duration::hours(24)
                    },
                );
                let margin = if lost_lately { 30 } else { 20 };
                let ceiling =
                    (keeper > 0).then(|| keeper.saturating_sub(margin));
                let Some(target) =
                    pick_lure_target_capped(&ui.best, wanted_level, ceiling)
                else {
                    // No target crawled today yet. Bailing here is what puts
                    // the crawler to work on them - it is not the dead end
                    // the old 90%-fresh gate was
                    status.put_session(session);
                    return refetch;
                };
                drop(status);
                info!("Auto Underworld attack {ident}");
                let fight = Command::perform(
                    async move {
                        let cmd = sf_api::command::Command::UnderworldAttack {
                            player_id: target.uid,
                        };
                        let resp = session.send_command(&cmd).await;
                        (resp, session)
                    },
                    move |r| match r.0 {
                        Ok(resp) => Message::PlayerLureResult {
                            ident,
                            session: r.1,
                            against: LureTarget {
                                uid: target.uid,
                                name: target.name,
                            },
                            resp: Box::new(resp),
                        },
                        Err(_) => Message::PlayerCommandFailed {
                            ident,
                            session: r.1,
                            attempt: 0,
                        },
                    },
                );

                return Command::batch([refetch, fight]);
            }
            Message::ConfigSetAutoLure { name, server, nv } => {
                let Some(config) = self.config.get_char_conf_mut(&name, server)
                else {
                    return Command::none();
                };
                config.auto_lure = nv;
                _ = self.config.write();
            }
            Message::AutoLure { ident, state } => {
                let Some(server) = self.servers.0.get_mut(&ident.server_id)
                else {
                    return Command::none();
                };
                let Some(player) = server.accounts.get_mut(&ident.account)
                else {
                    return Command::none();
                };

                let Some(si) = &mut player.underworld_info else {
                    return Command::none();
                };

                si.auto_lure = state;
            }
            Message::CopyBestLures { ident } => {
                let Some(server) = self.servers.0.get_mut(&ident.server_id)
                else {
                    return Command::none();
                };
                let Some(player) = server.accounts.get_mut(&ident.account)
                else {
                    return Command::none();
                };

                let Some(si) = &mut player.underworld_info else {
                    return Command::none();
                };

                let mut res = format!(
                    "Best lure targets on {}. Max Lvl = {}\n",
                    server.ident.url, si.max_level
                );

                for a in &si.best {
                    if a.is_old() {
                        continue;
                    }
                    _ = res.write_fmt(format_args!(
                        "lvl: {:3}, items: {}, name: {}\n",
                        a.level,
                        a.equipment.len(),
                        a.name,
                    ));
                }

                return iced::clipboard::write(res);
            }
            Message::SetAction(a) => {
                let View::Overview { action, .. } = &mut self.current_view
                else {
                    return Command::none();
                };
                *action = a;
            }
            Message::MultiAction { action } => {
                let View::Overview {
                    action: ac,
                    selected,
                } = &mut self.current_view
                else {
                    return Command::none();
                };
                let targets = match ac {
                    Some(ActionSelection::Multi) => {
                        selected.iter().copied().collect()
                    }
                    Some(ActionSelection::Character(c)) => vec![*c],
                    None => return Command::none(),
                };

                *ac = None;

                let messages = targets
                    .into_iter()
                    .map(|a| match action {
                        OverviewAction::Logout => {
                            Message::RemoveAccount { ident: a }
                        }
                        OverviewAction::AutoBattle(nv) => Message::AutoBattle {
                            ident: a,
                            state: nv,
                        },
                    })
                    .map(|a| Command::perform(async {}, move |_| a));

                return Command::batch(messages);
            }
        }
        Command::none()
    }
}

/// What one look at every character found: lines to celebrate in the
/// console, level popups (name, level) and epic popups (name, what)
type LevelWatch = (
    Vec<(AccountIdent, String)>,
    Vec<(String, u16)>,
    Vec<(String, String)>,
);

impl Helper {
    /// Whether every character is sitting still - nothing queued, nothing in
    /// flight. An update only takes over at such a moment
    fn quiet_enough(&self) -> bool {
        // A new version should reach everybody in minutes, not whenever the
        // queues happen to run dry - with the full bot on, they never do. So
        // the only thing that holds the handover up is a request actually in
        // flight; a queued job is picked up again by the new process a
        // second later. The short pause is there so a chain of steps (an
        // expedition clearing, a run of feeds) gets to finish first
        // No settling and no waiting for queues: the queues go over with
        // the handover, and a chain of steps just continues in the new
        // process. The only thing worth a wait is a request in flight
        for server in self.servers.0.values() {
            for account in server.accounts.values() {
                if !matches!(
                    &*account.status.lock_safe(),
                    AccountStatus::Idle(..) | AccountStatus::FatalError(_)
                ) {
                    return false;
                }
            }
        }
        true
    }

    /// Sends a command, that only needs the game state updated afterwards
    fn send_simple(
        &mut self,
        ident: AccountIdent,
        reason: &'static str,
        cmd: sf_api::command::Command,
        note: SimpleNote,
    ) -> Command<Message> {
        // STOP: nothing goes to the game, not even a step chained from an
        // answer that came in a second ago (the tavern's next clearing, the
        // guard's next shift). The queues are cleared on STOP, but those
        // chains bypass the queue - and one request on a dead session
        // starts a relogin that logs the player out of their own game
        if let Some(why) = self.halt_reason() {
            log::info!("{ident}: {reason} not sent - {why}");
            return Command::none();
        }
        // The ceiling on requests a minute holds here, for every request -
        // the queue kept to it, but the chains that bypass the queue (the
        // next expedition step, the next blacksmith level) did not, and the
        // panel read 14/12. A request over the ceiling waits its turn
        // before it goes out
        let cap = crate::stealth::request_cap(
            self.config.requests_per_min,
            self.config.pace,
            &self.profile(),
        );
        let slot_wait = self.next_send_at;
        let Some(account) = self.servers.get_account_mut(&ident) else {
            return Command::none();
        };
        let now = Local::now();
        let mut wait_ms = account.wait_for_cap(now, cap);
        // ...and the slot no two characters may share. `last_job` already
        // spaces the queue out, but the chains that bypass the queue keep
        // their own clocks, so the arena manager of one character and the
        // attribute buying of another were landing on the same second -
        // which from the server's side is one person with two accounts
        let slot_ms =
            slot_wait.map_or(0, |t| (t - now).num_milliseconds()).max(0);
        wait_ms = wait_ms.max(slot_ms);
        account.note_request(now + chrono::Duration::milliseconds(wait_ms));
        if wait_ms > 0 {
            log::info!("{ident}: {reason} waits {wait_ms} ms for its turn");
        }
        let arc = account.status.clone();
        let mut status = arc.lock_safe();
        let Some(mut session) = status.take_session(reason) else {
            drop(status);
            let msg = "Character is busy - try again in a moment".to_string();
            match note {
                SimpleNote::Dungeon(_)
                | SimpleNote::DungeonsUpdated
                | SimpleNote::Tower { .. } => {
                    account.dungeon_info.status = Some(msg)
                }
                SimpleNote::FortressMaint(_)
                | SimpleNote::FortressScout { .. } => {
                    if let Some(fi) = &mut account.fortress_info {
                        fi.maintain_status = Some(msg);
                    }
                }
                SimpleNote::PetFed { .. } => {
                    account.pet_info.status = Some(msg)
                }
                SimpleNote::PetArena { .. } => {
                    account.pet_info.arena_status = Some(msg)
                }
                SimpleNote::BackpackCleared(_)
                | SimpleNote::BackpackDismantled(_) => {
                    account.dungeon_info.status = Some(msg)
                }
                SimpleNote::Dice { .. } => {
                    account.tavern_info.dice_status = Some(msg)
                }
                SimpleNote::IdleBought(_) | SimpleNote::IdleCollected(_) => {
                    account.idle_info.status = Some(msg)
                }
                SimpleNote::Expedition(_) => {
                    account.expedition_info.status = Some(msg)
                }
                SimpleNote::Guard(_) => account.guard_info.status = Some(msg),
                SimpleNote::Dailies(_) => {
                    account.dailies_info.status = Some(msg)
                }
                // The fetch never started, so it is not in flight - leaving
                // the flag up was what froze the panel on "fetching..."
                SimpleNote::ArenaOffer { then_fight, .. } => {
                    if let Some(si) = &mut account.scrapbook_info {
                        si.arena_offer_busy = false;
                    }
                    return Command::perform(
                        async {
                            sleep(Duration::from_secs(15)).await;
                        },
                        move |()| Message::ArenaOfferRefresh {
                            ident,
                            then_fight,
                        },
                    );
                }
                SimpleNote::PetDungeon { .. } => {
                    account.dungeon_info.status = Some(msg)
                }
                SimpleNote::Beer => account.expedition_info.status = Some(msg),
                SimpleNote::Shop(_) | SimpleNote::WitchEnchant { .. } => {
                    account.shop_info.status = Some(msg)
                }
                SimpleNote::Recycle { .. } => {
                    account.shop_info.pending_recycle = None;
                    account.shop_info.status = Some(msg);
                }
                SimpleNote::Attr(..) => {
                    account.shop_info.attr_status = Some(msg)
                }
                SimpleNote::Wheel(_) => {
                    account.tavern_info.wheel_status = Some(msg)
                }
                // A busy moment must not end the finder pass half way
                // through the lookups - it waits and carries on
                SimpleNote::GuildFinder | SimpleNote::GuildFinderLook(_) => {
                    account.social.finder_status = Some(msg);
                    return Command::perform(
                        async {
                            sleep(Duration::from_secs(10)).await;
                        },
                        move |()| Message::GuildFinderStep { ident },
                    );
                }
                SimpleNote::GuildJoin { .. } | SimpleNote::GuildJoinLook(_) => {
                    account.social.finder_status = Some(msg);
                }
                SimpleNote::ReportSent(to) => {
                    // Put the member back and try again in a moment
                    account.social.report_queue.push(to);
                    account.social.report_status = Some(msg);
                    return Command::perform(
                        async {
                            sleep(Duration::from_secs(8)).await;
                        },
                        move |()| Message::GuildReportNext { ident },
                    );
                }
                _ => account.social.status = Some(msg),
            }
            return Command::none();
        };
        drop(status);
        // The slot is taken now that the request is certainly going out:
        // the next one, whoever it belongs to, queues behind it. Only here
        // - a request that never left (busy character, STOP) must not push
        // everybody else back
        self.next_send_at = Some(
            Local::now()
                + chrono::Duration::milliseconds(wait_ms)
                + chrono::Duration::seconds(crate::pace::global_gap()),
        );
        // No console line on the way out. The answer follows within
        // milliseconds and carries the same words plus how it went - printing
        // both meant every single action appeared twice

        Command::perform(
            async move {
                if wait_ms > 0 {
                    sleep(Duration::from_millis(wait_ms as u64)).await;
                }
                let resp = session.send_command(&cmd).await;
                (resp, session)
            },
            move |r| match r.0 {
                Ok(resp) => Message::SimpleResult {
                    ident,
                    session: r.1,
                    resp: Box::new(resp),
                    note: note.clone(),
                },
                Err(sf_api::error::SFError::ServerError(e)) => {
                    Message::SimpleFailed {
                        ident,
                        session: r.1,
                        note: note.clone(),
                        error: e,
                    }
                }
                Err(_) => Message::PlayerCommandFailed {
                    ident,
                    session: r.1,
                    attempt: 0,
                },
            },
        )
    }

    /// The recovery page is done with: `config` is what the app runs on
    /// from now, writes are allowed again, and the saved accounts log in
    /// the way they do at start
    fn config_recovered(
        &mut self,
        config: crate::config::Config,
    ) -> Command<Message> {
        crate::config::Config::unblock_writes();
        crate::i18n::set_lang(config.lang);
        crate::pace::set(config.pace);
        Job::set_order(&config.job_order);
        self.config = config;
        self.config_problem = None;
        _ = self.config.write();
        self.login_state.login_typ = if self.config.accounts.is_empty() {
            crate::login::LoginType::Regular
        } else {
            crate::login::LoginType::Saved
        };
        let (loading, logins) = self.saved_logins(&[]);
        self.current_view = if loading > 0 {
            crate::View::Overview {
                selected: Default::default(),
                action: Default::default(),
            }
        } else {
            crate::View::Login
        };
        Command::batch(logins)
    }

    /// The join request itself: `GroupJoin` with the guild's name, or with
    /// its id from the finder list on the second try
    fn guild_join_request(
        &mut self,
        ident: AccountIdent,
        name: String,
        by_id: bool,
    ) -> Command<Message> {
        let Some(account) = self.servers.get_account_mut(&ident) else {
            return Command::none();
        };
        let argument = if by_id {
            let Some(id) = account
                .social
                .finder
                .iter()
                .find(|g| g.name.eq_ignore_ascii_case(&name))
                .map(|g| g.id)
            else {
                account.social.finder_status =
                    Some(format!("{name} is not in the finder list any more"));
                return Command::none();
            };
            id.to_string()
        } else {
            name.clone()
        };
        self.send_simple(
            ident,
            "Guild finder",
            sf_api::command::Command::Custom {
                cmd_name: "GroupJoin".to_string(),
                arguments: vec![argument],
            },
            SimpleNote::GuildJoin { name, by_id },
        )
    }

    /// The sweep, every ten minutes: every enabled job of every free
    /// character goes into its queue once, whatever that job's own checker
    /// is still sleeping on. Each step asks the game's state before it
    /// acts, so a job with nothing to do costs one look and nothing else -
    /// while a job whose checker slept through a change (a pet unlocked, a
    /// build finished, gold that came in) runs within minutes instead of
    /// on the next tick of its own clock. The guild leader's "central
    /// intelligence"
    fn sweep(&mut self) {
        if self.halted() {
            return;
        }
        if !self.burst_active() {
            return;
        }
        let now = Local::now();
        // The ten-minute hold is for the middle of an ordinary session.
        // While "do it now" runs, work that becomes due is picked up
        // within the minute - that is the whole point of pressing it
        let hold = if self.boosting() { 1 } else { 10 };
        if self
            .last_sweep
            .is_some_and(|t| now - t < chrono::Duration::minutes(hold))
        {
            return;
        }
        self.last_sweep = Some(now);
        for server in self.servers.0.values_mut() {
            for account in server.accounts.values_mut() {
                if !matches!(
                    &*account.status.lock_safe(),
                    AccountStatus::Idle(..)
                ) {
                    continue;
                }
                for job in Job::FULL_BOT_START {
                    if account_job_enabled(account, job)
                        && !account.queue.iter().any(|(j, _)| *j == job)
                    {
                        account.queue.push((job, now));
                    }
                }
            }
        }
    }

    /// One step of the guild finder pass: the next guild waiting for its
    /// bonus lookup, or - once every one is known - the join request for
    /// the best of them, when the automatic pick is on. Only ever one join
    /// per pass: a closed guild keeps the request for its leader, and
    /// knocking on ten doors at once is not how a player behaves
    fn guild_finder_step(&mut self, ident: AccountIdent) -> Command<Message> {
        let Some(account) = self.servers.get_account_mut(&ident) else {
            return Command::none();
        };
        // The finder is for a character without a guild - a join that
        // came through mid-pass ends the pass
        let in_guild = match &*account.status.lock_safe() {
            AccountStatus::Idle(_, gs) | AccountStatus::Busy(gs, _) => {
                gs.guild.is_some()
            }
            _ => false,
        };
        if in_guild {
            account.social.finder_pending.clear();
            account.social.finder_status = None;
            return Command::none();
        }
        if !account.social.finder_pending.is_empty() {
            let name = account.social.finder_pending.remove(0);
            let left = account.social.finder_pending.len();
            account.social.finder_status =
                Some(format!("Looking at {name} ({left} more to go)"));
            return self.send_simple(
                ident,
                "Guild finder",
                sf_api::command::Command::ViewGuild {
                    guild_ident: name.clone(),
                },
                SimpleNote::GuildFinderLook(name),
            );
        }
        let pick = crate::guild_finder::best(
            &account.social.finder, &account.social.finder_bonus,
            &account.social.finder_tried,
        )
        .map(|g| g.name.clone());
        match pick {
            Some(name) if account.social.finder_auto => {
                Command::perform(async {}, move |()| Message::GuildFinderJoin {
                    ident,
                    name,
                })
            }
            Some(name) => {
                account.social.finder_status = Some(format!(
                    "Bonuses read - {name} is the best pick (Join, or switch \
                     the automatic pick on)"
                ));
                Command::none()
            }
            None => {
                account.social.finder_status = Some(
                    "Bonuses read - no guild with a free seat left to ask"
                        .to_string(),
                );
                Command::none()
            }
        }
    }

    /// Flips one of the two underworld housekeeping switches, remembers it in
    /// the character's config and, when something was switched on, does a
    /// pass right away rather than waiting for the job's turn in the queue
    fn set_underworld_auto(
        &mut self,
        ident: AccountIdent,
        collect: Option<bool>,
        build: Option<bool>,
    ) -> Command<Message> {
        let Some(account) = self.servers.get_account_mut(&ident) else {
            return Command::none();
        };
        let name = account.name.clone();
        let Some(ui) = &mut account.underworld_info else {
            return Command::none();
        };
        if let Some(c) = collect {
            ui.auto_collect = c;
        }
        if let Some(b) = build {
            ui.auto_build = b;
        }
        ui.maint_steps = 0;
        let on = ui.auto_collect || ui.auto_build;
        if let Some(conf) =
            self.config.get_char_conf_mut(&name, ident.server_id)
        {
            if let Some(c) = collect {
                conf.underworld_collect = c;
            }
            if let Some(b) = build {
                conf.underworld_build = b;
            }
            _ = self.config.write();
        }
        if on {
            return Command::perform(async {}, move |()| {
                Message::UnderworldMaintainTick { ident }
            });
        }
        Command::none()
    }

    /// One step of underworld housekeeping: take a finished upgrade off the
    /// board, collect what a full store would otherwise throw away, start
    /// the next building. One request per call, the result handler comes
    /// back for the next one.
    ///
    /// The build order is deliberately not a return-on-investment sum like
    /// the arena manager's. Down here the numbers are not what decides:
    /// **the Heart of Darkness caps the level of every other building**, so
    /// whatever produces most, the heart is the only thing that ever unblocks
    /// anything. That part is not hardcoded game lore - it is read straight
    /// off the levels (`level >= heart`), so if the game ever changes the
    /// rule, the code notices.
    ///
    /// The rest is ordered by what it is worth to a character that plays
    /// around the clock, which is not what a guide for human players says:
    ///
    /// - the Gladiator Trainer raises critical damage for the character, the
    ///   companions **and** the pets - that is every fight the app takes
    /// - the Soul Extractor is the engine. Souls buy every other upgrade
    ///   down here and are good for nothing else
    /// - the Gate hands out one more lure per day up to level 5 and nothing
    ///   whatsoever after that, so it stops at 5
    /// - the Torture Chamber multiplies the souls a won lure brings, so it
    ///   only starts paying once the gate really hands out five fights
    /// - the Gold Pit is the only building whose output leaves the underworld
    /// - keeper, trolls and goblins are defence: they matter when somebody
    ///   lures *us*, which is not something we control
    /// - the Adventuromatic stores the thirst for adventure that is left
    ///   over at midnight. A character this app plays has none left over, so
    ///   here it is the least useful building in the game - exactly the
    ///   opposite of every guide's advice, and the reason this list is not
    ///   simply copied from one
    fn underworld_maintain(&mut self, ident: AccountIdent) -> Command<Message> {
        // Outside a session - the quiet hours, a stretch away - nothing
        // happens. This asks `burst_active`, not the quiet hours alone:
        // full power farms through the night by the player's own word
        if !self.burst_active() {
            return Command::none();
        }

        use sf_api::{
            command::Command as SFCommand,
            gamestate::underworld::{
                UnderworldBuildingType as B, UnderworldResourceType,
                UnderworldUnitType as U,
            },
        };

        let Some(account) = self.servers.get_account_mut(&ident) else {
            return Command::none();
        };
        let Some(ui) = &mut account.underworld_info else {
            return Command::none();
        };
        // Eight requests is plenty for one turn - the rest of the queue is
        // waiting, and the next turn picks up exactly where this one stopped
        if ui.maint_steps >= 8 {
            ui.maintain_status =
                Some("Enough for this round - continues next time".into());
            return Command::none();
        }
        ui.maint_steps += 1;
        let (collect, build) = (ui.auto_collect, ui.auto_build);
        // When each resource was last collected, copied out before the lock so
        // the collect loop can leave a just-taken one alone for a few minutes
        let recent = ui.collected_at;
        let collect_frac = ui.collect_frac;
        if !collect && !build {
            return Command::none();
        }

        let arc = account.status.clone();
        let status = arc.lock_safe();
        let AccountStatus::Idle(_, gs) = &*status else {
            // Busy characters are simply skipped, the next tick tries again
            return Command::none();
        };
        let Some(uw) = gs.underworld.as_ref() else {
            return Command::none();
        };
        let now = Local::now();
        // The game's day, not the machine's - the reset the once-a-day
        // rules hang off is the server's
        let today = gs.server_time().current().date();
        let silver = gs.character.silver;
        let souls = uw.souls_current;

        let mut action: Option<(SFCommand, String)> = None;
        // Set when this step starts a Gold Pit, so its day can be stamped
        // once the lock is gone
        let mut started_gold_pit = false;
        // Which resource this call collects, so its cooldown can be stamped
        // once the lock is dropped
        let mut collected: Option<UnderworldResourceType> = None;

        // 1) A finished upgrade has to be taken before the next one can
        //    start, so it goes first - otherwise a done building would sit
        //    there all night blocking the whole underworld
        if build
            && let Some(building) = uw.upgrade_building
            && uw.upgrade_finish.is_some_and(|t| t <= now)
        {
            action = Some((
                SFCommand::UnderworldUpgradeFinish {
                    building,
                    mushrooms: 0,
                },
                format!("Finished {building:?}"),
            ));
        }

        // 2) Collect before a full store stops the production. The server
        //    only sends the collectable amount now and then, so what has
        //    accrued since is added from the hourly rate - the same trick
        //    the fortress uses
        if action.is_none() && collect {
            let since_h = uw
                .last_collectable_update
                .map(|t| (now - t).num_seconds().max(0) as f64 / 3600.0)
                .unwrap_or(0.0);
            for (typ, prod) in &uw.production {
                if prod.limit == 0 || prod.per_hour == 0 {
                    continue;
                }
                // The adventuromatic's figure is per day, not per hour
                let per_hour =
                    if typ == UnderworldResourceType::ThirstForAdventure {
                        prod.per_hour as f64 / 24.0
                    } else {
                        prod.per_hour as f64
                    };
                // Each store has its OWN small capacity and fills in
                // limit / production hours (2.5 h at a low soul extractor,
                // ~24 h at a maxed one). The game's stored "collectable" count
                // is documented as always out of date - it never drops after a
                // collect - so trusting it fires the collect on every tick,
                // dozens a second. Instead, once we take a resource we leave
                // it alone until the store is `frac` full again - a share
                // drawn anew after every collect (see `collect_frac`), so
                // the pickups come often but never on a clock: a person
                // drops by the extractor at odd moments, a script that
                // empties it at exactly 90 % every 2 h 15 min is what gets
                // looked at
                let frac = f64::from(collect_frac[typ as usize]);
                let fill_secs = (prod.limit as f64 / per_hour * 3600.0)
                    .clamp(300.0, 86_400.0);
                let cooldown =
                    chrono::Duration::seconds((fill_secs * frac * 0.8) as i64);
                if recent[typ as usize].is_some_and(|t| now - t < cooldown) {
                    continue;
                }
                let estimated = (prod.last_collectable as f64
                    + per_hour * since_h)
                    .min(prod.limit as f64);
                if estimated >= prod.limit as f64 * frac {
                    collected = Some(typ);
                    action = Some((
                        SFCommand::UnderworldCollect { resource: typ },
                        format!("Collected {typ:?}"),
                    ));
                    break;
                }
            }
        }

        // The order the buildings go up in - one of two, by the phase the
        // underworld is in (see `mechanics::underworld_order`): souls first
        // while it is being built up, the character's own bonuses later.
        // The Heart of Darkness is the cap on the others in both, so it is
        // only ever raised when something has hit that cap - never picked
        // greedily for being first, which would pour every soul into it and
        // starve the buildings meant to catch up to it
        let heart = uw.buildings[B::HeartOfDarkness].level;
        let phase = crate::mechanics::underworld_phase(
            account
                .underworld_info
                .as_ref()
                .map_or(Default::default(), |u| u.phase),
            heart,
        );
        let order = crate::mechanics::underworld_order(phase);
        // The same rule the screen uses, including the Gold Pit's "only
        // when there is nothing else to build" - one source of truth
        let eligible = |b: B| crate::mechanics::underworld_eligible(uw, b);
        let affordable = |b: B| {
            let cost = &uw.buildings[b].upgrade_cost;
            cost.silver <= silver && cost.souls <= souls
        };
        // What the souls are being SAVED for. Everything behind this
        // target, and every unit, only ever spends what is left ABOVE it -
        // so a cheap goblin pit never eats the extractor's souls. The rule
        // itself lives in `mechanics`, because the screen has to be able
        // to say the same thing without waiting for this loop to run
        let saving = crate::mechanics::underworld_saving_for(uw, phase);
        let save_target = saving.map(|(_, s)| s).unwrap_or(0);

        // 3) Start the next building. Only one may be going up at a time,
        //    which is what `upgrade_building` says
        if action.is_none() && build && uw.upgrade_building.is_none() {
            // Read the cap off the data rather than trusting a wiki: if
            // anything already sits at the heart's level, the heart is what
            // stands in the way and it goes next
            let blocked = order.iter().any(|&b| {
                b != B::HeartOfDarkness
                    && b != B::GoldPit
                    && uw.buildings[b].level >= heart
            });
            // The Gold Pit's other half: at most one level a day. It is
            // the cheapest thing down here to raise, so without a limit a
            // rich evening would put ten levels into the one building
            // that pays in silver instead of in anything the character
            // takes into a fight
            let pit_done_today = account
                .underworld_info
                .as_ref()
                .and_then(|u| u.gold_pit_day)
                == Some(today);
            let pick = if blocked && affordable(B::HeartOfDarkness) {
                Some(B::HeartOfDarkness)
            } else {
                // The first of the order goes up as soon as it is
                // affordable; anything behind it only out of the surplus
                order.iter().copied().find(|&b| {
                    b != B::HeartOfDarkness
                        && !(b == B::GoldPit && pit_done_today)
                        && eligible(b)
                        && affordable(b)
                        && (saving.is_some_and(|(t, _)| t == b)
                            || souls.saturating_sub(
                                uw.buildings[b].upgrade_cost.souls,
                            ) >= save_target)
                })
            };
            if let Some(building) = pick {
                if building == B::GoldPit {
                    started_gold_pit = true;
                }
                let cost = &uw.buildings[building].upgrade_cost;
                action = Some((
                    SFCommand::UnderworldUpgradeStart {
                        building,
                        mushrooms: 0,
                    },
                    format!(
                        "Upgrading {building:?} to level {} ({} silver, {} \
                         souls)",
                        uw.buildings[building].level + 1,
                        human_money(cost.silver as f64),
                        human_money(cost.souls as f64)
                    ),
                ));
            }
        }

        // 4) Goblins, trolls and the keeper. They come after the buildings
        //    and only out of the souls the next building does not need.
        //    While the souls are being gathered the keeper comes first,
        //    alongside its building - it is what beats the stronger lured
        //    heroes, who bring the most souls. Later the lowest level goes
        //    first, so the three come up together instead of one running
        //    away; the keeper wins a tie because it is the strongest of them
        if action.is_none() && build {
            const UNITS: [U; 3] = [U::Keeper, U::Troll, U::Goblin];
            let pick = UNITS
                .into_iter()
                .filter(|&u| {
                    let cost = &uw.units[u].upgrade_cost;
                    cost.silver <= silver
                        && cost.souls.saturating_add(save_target) <= souls
                })
                .min_by_key(|&u| {
                    if phase == crate::mechanics::UnderworldPhase::Souls
                        && u == U::Keeper
                    {
                        0
                    } else {
                        uw.units[u].level
                    }
                });
            if let Some(unit) = pick {
                let unit_data = &uw.units[unit];
                action = Some((
                    SFCommand::UnderworldUnitUpgrade { unit },
                    format!(
                        "Upgrading the {unit:?} to level {} ({} souls)",
                        unit_data.upgrade_next_lvl,
                        human_money(unit_data.upgrade_cost.souls as f64)
                    ),
                ));
            }
        }

        // The underworld as it stands, for the status line below - the
        // lock goes away with the next line
        let uw_snapshot = uw.clone();
        drop(status);

        if started_gold_pit && let Some(ui) = &mut account.underworld_info {
            ui.gold_pit_day = Some(today);
        }
        // Remember when this resource was collected, so the loop above leaves
        // it be until it has had time to fill again
        if let Some(r) = collected
            && let Some(ui) = &mut account.underworld_info
        {
            ui.collected_at[r as usize] = Some(now);
            ui.collect_frac[r as usize] =
                crate::player::UnderworldInfo::draw_frac();
            ui.collect_before = Some((
                r,
                if r == UnderworldResourceType::Souls {
                    souls
                } else {
                    silver
                },
            ));
        }
        let Some((cmd, what)) = action else {
            if let Some(ui) = &mut account.underworld_info {
                ui.maintain_status = Some(match saving {
                    // Building is on and the target is out of reach: say what
                    // the souls are being held for, so a quiet page reads as
                    // "saving", not "broken"
                    Some((target, cost)) if build && souls < cost => {
                        let eta = crate::mechanics::souls_eta_secs(
                            &uw_snapshot, cost,
                        )
                        .map(|s| {
                            format!(
                                ", about {} at the current rate",
                                crate::util::long_wait(s)
                            )
                        })
                        .unwrap_or_default();
                        format!(
                            "Saving for the {}: {} of {} souls{eta}",
                            crate::util::split_camel(&format!("{target:?}")),
                            human_money(souls as f64),
                            human_money(cost as f64),
                        )
                    }
                    _ => format!("Nothing to do ({})", now.format("%H:%M")),
                });
            }
            return Command::none();
        };

        self.send_simple(
            ident,
            "Underworld",
            cmd,
            SimpleNote::UnderworldMaint(what),
        )
    }

    /// One step of fortress housekeeping. Picks the first thing that is
    /// due - collecting, finishing a build, gems, upgrading - and sends it.
    /// The result handler calls this again, so a whole backlog gets worked
    /// through one request at a time
    fn fortress_maintain(&mut self, ident: AccountIdent) -> Command<Message> {
        // Outside a session - the quiet hours, a stretch away - nothing
        // happens. This asks `burst_active`, not the quiet hours alone:
        // full power farms through the night by the player's own word
        if !self.burst_active() {
            return Command::none();
        }

        use sf_api::{
            command::Command as SFCommand,
            gamestate::fortress::{FortressBuildingType, FortressResourceType},
        };

        let Some(account) = self.servers.get_account_mut(&ident) else {
            return Command::none();
        };
        if let Some(fi) = &mut account.fortress_info {
            if fi.maint_steps >= 12 {
                fi.maintain_status =
                    Some("Enough for this round - continues next time".into());
                return Command::none();
            }
            fi.maint_steps += 1;
        }
        let (collect, gems, build, recruit_all) = match &account.fortress_info {
            Some(fi) if fi.maintains() => {
                (fi.auto_collect, fi.auto_gems, fi.auto_build, fi.recruit_all)
            }
            _ => return Command::none(),
        };

        let arc = account.status.clone();
        let status = arc.lock_safe();
        let AccountStatus::Idle(_, gs) = &*status else {
            // Busy characters are simply skipped, the next tick tries again
            return Command::none();
        };
        let Some(fortress) = gs.fortress.as_ref() else {
            return Command::none();
        };
        let now = Local::now();
        let silver = gs.character.silver;

        let mut action: Option<(SFCommand, String)> = None;

        // 1) Collect, once a production building is nearly full. The server
        //    only reports the collectable amount now and then, so it gets
        //    extrapolated from the hourly production
        if collect {
            let since_h = fortress
                .last_collectable_updated
                .map(|t| (now - t).num_seconds().max(0) as f64 / 3600.0)
                .unwrap_or(0.0);
            for (typ, res) in &fortress.resources {
                let prod = &res.production;
                if prod.limit == 0 {
                    continue;
                }
                let estimated = (prod.last_collectable as f64
                    + prod.per_hour as f64 * since_h)
                    .min(prod.limit as f64);
                let storage_full = typ != FortressResourceType::Experience
                    && res.current >= res.limit;
                if estimated >= prod.limit as f64 * 0.9 && !storage_full {
                    action = Some((
                        SFCommand::FortressGather { resource: typ },
                        format!("Collected {typ:?}"),
                    ));
                    break;
                }
            }
        }

        // 2) A finished upgrade has to be confirmed before anything else can
        //    be built
        if action.is_none()
            && let Some(target) = fortress.building_upgrade.target
            && fortress.building_upgrade.finish.is_some_and(|f| f <= now)
        {
            action = Some((
                SFCommand::FortressBuildFinish {
                    f_type: target,
                    mushrooms: 0,
                },
                format!("Finished upgrade of {target:?}"),
            ));
        }

        // 3) Gems: confirm a finished search, or start a new one. The game
        //    refuses to mine while the mine itself is being upgraded, and
        //    after any such refusal we wait until the current build is done
        let mine_upgrading = fortress.building_upgrade.target
            == Some(FortressBuildingType::GemMine);
        let gems_blocked = account
            .fortress_info
            .as_ref()
            .and_then(|a| a.gems_blocked_until)
            .is_some_and(|t| t > now);
        if action.is_none() && gems && !mine_upgrading && !gems_blocked {
            let mine_built =
                fortress.buildings[FortressBuildingType::GemMine].level > 0;
            match fortress.gem_search.finish {
                Some(finish) if finish <= now => {
                    action = Some((
                        SFCommand::FortressGemStoneSearchFinish {
                            mushrooms: 0,
                        },
                        "Collected the gem".to_string(),
                    ));
                }
                Some(_) => {}
                None if mine_built => {
                    let cost = fortress.gem_search.cost;
                    let affordable = cost.wood
                        <= fortress.resources[FortressResourceType::Wood]
                            .current
                        && cost.stone
                            <= fortress.resources[FortressResourceType::Stone]
                                .current
                        && cost.silver <= silver;
                    if affordable {
                        action = Some((
                            SFCommand::FortressGemStoneSearch,
                            "Started a gem search".to_string(),
                        ));
                    }
                }
                None => {}
            }
        }

        // 3b) The gems that have been dug up. The mine can not be steered, so
        //     the sorting happens here: a gem goes into an empty slot of
        //     someone who fights with that attribute, and what is left over
        //     and was not asked for is sold. The rare ones are never sold
        if action.is_none()
            && let Some(fi) = account.fortress_info.as_ref()
            && fi.gem_socket
        {
            trace!("Gem processor: checking inventory for gems to process");
            let mut wearers = vec![crate::gems::Wearer {
                owner: crate::gems::GemOwner::Character,
                class: gs.character.class,
                equipment: &gs.character.equipment,
            }];
            if let Some(companions) = &gs.dungeons.companions {
                for (typ, companion) in companions {
                    wearers.push(crate::gems::Wearer {
                        owner: crate::gems::GemOwner::Companion(typ),
                        class: sf_api::gamestate::character::Class::from(typ),
                        equipment: &companion.equipment,
                    });
                }
            }
            trace!(
                "Gem processor: evaluating {} potential wearers",
                wearers.len()
            );
            if let Some(next) = crate::gems::next_action(
                &gs.character.inventory, &wearers, &fi.gem_wanted,
                fi.gem_socket,
            ) {
                action = Some(match next {
                    crate::gems::GemAction::Socket {
                        bag_pos,
                        owner,
                        slot,
                        gem,
                        replaces,
                    } => {
                        trace!(
                            "Gem processor: socketing {} into {}'s {slot:?}",
                            crate::gems::gem_name(gem),
                            owner.label()
                        );
                        let item_ident = gs
                            .character
                            .inventory
                            .backpack
                            .get(bag_pos.backpack_pos())
                            .and_then(|i| i.as_ref())
                            .map(sf_api::gamestate::items::Item::command_ident);
                        let Some(item_ident) = item_ident else {
                            return Command::none();
                        };
                        let what = match replaces {
                            Some(old) => format!(
                                "Put the {} into {}'s {slot:?}, over the old \
                                 one ({})",
                                crate::gems::gem_name(gem),
                                owner.label(),
                                old.value
                            ),
                            None => format!(
                                "Put the {} into {}'s {slot:?}",
                                crate::gems::gem_name(gem),
                                owner.label()
                            ),
                        };
                        match owner {
                            crate::gems::GemOwner::Character => (
                                SFCommand::PlayerItemMove {
                                    from: bag_pos.into(),
                                    to: slot.into(),
                                    item_ident,
                                },
                                what,
                            ),
                            crate::gems::GemOwner::Companion(to_companion) => (
                                SFCommand::EquipCompanion {
                                    from_pos: bag_pos.into(),
                                    to_slot: slot,
                                    item_ident,
                                    to_companion,
                                },
                                what,
                            ),
                        }
                    }
                    crate::gems::GemAction::Sell { bag_pos, gem } => {
                        trace!(
                            "Gem processor: selling {} (not wanted)",
                            crate::gems::gem_name(gem)
                        );
                        let item_ident = gs
                            .character
                            .inventory
                            .backpack
                            .get(bag_pos.backpack_pos())
                            .and_then(|i| i.as_ref())
                            .map(sf_api::gamestate::items::Item::command_ident);
                        let Some(item_ident) = item_ident else {
                            return Command::none();
                        };
                        (
                            SFCommand::SellShop {
                                item_pos: bag_pos.into(),
                                item_ident,
                            },
                            format!(
                                "Sold the {} - not on the keep list",
                                crate::gems::gem_name(gem)
                            ),
                        )
                    }
                });
            }
        }

        // 3b) Make room. A full backpack silently stops the dungeons - the
        //     game will not hand out loot it cannot put anywhere - and gem
        //     mining fills it fast, so the weakest unwanted piece goes
        //     An emergency dismantle goes through `backpack_sale`, which
        //     has its own note for a refusal (see `next_sale`)
        let mut dismantle_for_room = false;
        if action.is_none() && account.shop_info.clear_backpack {
            let protection = account.shop_info.protection();
            let can_dismantle = gs.blacksmith.is_some()
                && account
                    .shop_info
                    .dismantle_blocked_until
                    .is_none_or(|t| t <= Local::now());
            if let Some(sale) = crate::backpack::next_sale(
                gs,
                &protection,
                usize::from(account.shop_info.keep_free.max(1)),
                can_dismantle,
            ) {
                let item_ident = gs
                    .character
                    .inventory
                    .backpack
                    .get(sale.pos.backpack_pos())
                    .and_then(|i| i.as_ref())
                    .map(sf_api::gamestate::items::Item::command_ident);
                match (sale.disposal, item_ident) {
                    (crate::backpack::Disposal::Dismantle, Some(_)) => {
                        dismantle_for_room = true;
                    }
                    (crate::backpack::Disposal::Sell, Some(item_ident)) => {
                        let (used, total) = crate::backpack::space(gs);
                        action = Some((
                            SFCommand::SellShop {
                                item_pos: sale.pos.into(),
                                item_ident,
                            },
                            format!(
                                "Backpack {used}/{total} - sold the {} to \
                                 keep the dungeons running",
                                sale.what
                            ),
                        ));
                    }
                    (_, None) => {}
                }
            }
        }

        // 4) Fill the army: whichever unit type is missing the most gets
        //    queued, as many as the resources allow
        if action.is_none() && recruit_all {
            use sf_api::gamestate::fortress::FortressUnitType;
            let mut best: Option<(u16, FortressUnitType, u32)> = None;
            for unit_typ in [
                FortressUnitType::Soldier,
                FortressUnitType::Archer,
                FortressUnitType::Magician,
            ] {
                let unit = &fortress.units[unit_typ];
                if unit.limit == 0 || unit.in_training > 0 {
                    continue;
                }
                let missing = unit.limit.saturating_sub(unit.count);
                if missing == 0 {
                    continue;
                }
                let cost = unit.training.cost;
                let wood =
                    fortress.resources[FortressResourceType::Wood].current;
                let stone =
                    fortress.resources[FortressResourceType::Stone].current;
                let affordable = [
                    (cost.wood, wood),
                    (cost.stone, stone),
                    (cost.silver, silver),
                ]
                .iter()
                .map(|(c, have)| if *c == 0 { u64::MAX } else { have / c })
                .min()
                .unwrap_or(0)
                .min(u64::from(missing))
                    as u32;
                if affordable == 0 {
                    continue;
                }
                if best.is_none_or(|(m, _, _)| missing > m) {
                    best = Some((missing, unit_typ, affordable));
                }
            }
            if let Some((_, unit, count)) = best {
                action = Some((
                    SFCommand::FortressBuildUnit { unit, count },
                    format!("Recruiting {count} x {unit:?}"),
                ));
            }
        }

        // 5) Build, in the guild leader's order (5.4): the Fortress itself
        //    first (it caps every other building), then the Hall of
        //    Knights (the guild's battle bonus, below), then the gem mine;
        //    then the economy and the military, lowest level first within
        //    each; the Treasury last - it only buys bag room, and the
        //    backpack step keeps room now
        if action.is_none()
            && build
            && fortress.building_upgrade.target.is_none()
        {
            const TIERS: [&[FortressBuildingType]; 5] = [
                &[FortressBuildingType::Fortress],
                &[FortressBuildingType::GemMine],
                &[
                    FortressBuildingType::WoodcuttersHut,
                    FortressBuildingType::Quarry,
                    FortressBuildingType::LaborersQuarters,
                    FortressBuildingType::Academy,
                ],
                &[
                    FortressBuildingType::Wall,
                    FortressBuildingType::Barracks,
                    FortressBuildingType::ArcheryGuild,
                    FortressBuildingType::MagesTower,
                    FortressBuildingType::Smithy,
                ],
                &[FortressBuildingType::Treasury],
            ];
            let pick = TIERS.iter().find_map(|tier| {
                let mut in_tier: Vec<(u16, usize, FortressBuildingType)> = tier
                    .iter()
                    .enumerate()
                    .map(|(i, b)| (fortress.buildings[*b].level, i, *b))
                    .collect();
                in_tier.sort_by_key(|(level, prio, _)| (*level, *prio));
                in_tier
                    .into_iter()
                    .find(|(_, _, b)| fortress.can_build(*b, silver))
            });
            // The Hall of Knights comes right after the Fortress itself:
            // it is the guild's battle bonus, and nothing in the app ever
            // raised it before (5.4)
            let hok = &fortress.hall_of_knights_upgrade_price;
            let wood = fortress.resources[FortressResourceType::Wood].current;
            let stone =
                fortress.resources[FortressResourceType::Stone].current;
            let hok_blocked = account
                .fortress_info
                .as_ref()
                .and_then(|f| f.hok_blocked_until)
                .is_some_and(|t| t > now);
            let hok_ok = !hok_blocked
                && (hok.wood > 0 || hok.stone > 0 || hok.silver > 0)
                && hok.wood <= wood
                && hok.stone <= stone
                && hok.silver <= silver
                && fortress.hall_of_knights_level
                    < fortress.buildings[FortressBuildingType::Fortress].level;
            let fortress_first =
                matches!(pick, Some((_, _, FortressBuildingType::Fortress)));
            if hok_ok && !fortress_first {
                action = Some((
                    SFCommand::FortressUpgradeHallOfKnights,
                    format!(
                        "Upgrading the Hall of Knights to level {}",
                        fortress.hall_of_knights_level + 1
                    ),
                ));
            } else if let Some((_, _, b)) = pick {
                action = Some((
                    SFCommand::FortressBuild { f_type: b },
                    format!(
                        "Upgrading {b:?} to level {}",
                        fortress.buildings[b].level + 1
                    ),
                ));
            }
        }

        drop(status);

        let Some((cmd, what)) = action else {
            if dismantle_for_room
                && let Some(command) = self.backpack_sale(ident)
            {
                return command;
            }
            let Some(account) = self.servers.get_account_mut(&ident) else {
                return Command::none();
            };
            if let Some(fi) = &mut account.fortress_info {
                fi.maintain_status =
                    Some(format!("Nothing to do ({})", now.format("%H:%M")));
            }
            return Command::none();
        };

        self.send_simple(
            ident,
            "Fortress",
            cmd,
            SimpleNote::FortressMaint(what),
        )
    }

    /// Spends the free dungeon fight on the enterable dungeon with the best
    /// simulated odds, as long as those odds clear the configured minimum
    fn dungeon_auto_fight(&mut self, ident: AccountIdent) -> Command<Message> {
        // The on-disk ledger's key for the mushroom count, taken before the
        // account is borrowed
        let key = self.char_key(&ident);
        let Some(account) = self.servers.get_account_mut(&ident) else {
            return Command::none();
        };
        let di = &mut account.dungeon_info;
        // Every way out of this function says why. Three of them used to
        // return in silence, which from the outside is indistinguishable
        // from a broken auto dungeon: the job runs, the log stays empty and
        // there is nothing to debug with
        if !di.auto {
            log::info!("{ident}: auto dungeon - switched off");
            return Command::none();
        }
        // The demonic portal is its own daily fight - separate from the free
        // dungeon fight and its cooldown - so it is handled first and on its
        // own once-a-day guard. `can_fight` from the game is the authority;
        // the date guard only stops us asking again on every tick
        let today = Local::now().date_naive();
        if di.portal_auto && di.portal_tried != Some(today) {
            let portal = {
                let arc = account.status.clone();
                let lock = arc.lock_safe();
                match &*lock {
                    AccountStatus::Idle(_, gs) | AccountStatus::Busy(gs, _) => {
                        gs.dungeons
                            .portal
                            .as_ref()
                            .map(|p| (p.can_fight, p.enemy_hp_percentage))
                    }
                    _ => None,
                }
            };
            if let Some((true, hp)) = portal
                && hp > 0
            {
                account.dungeon_info.portal_tried = Some(today);
                log::info!(
                    "{ident}: auto dungeon - fighting the demonic portal"
                );
                return self.send_simple(
                    ident,
                    "Portal",
                    sf_api::command::Command::FightPortal,
                    SimpleNote::DemonPortal,
                );
            }
        }
        let di = &mut account.dungeon_info;
        if let Some(last) = di.last_auto_fight
            && Local::now() - last < chrono::Duration::minutes(5)
        {
            log::info!(
                "{ident}: auto dungeon - the last fight was {}s ago, holding \
                 for the 5 minute guard",
                (Local::now() - last).num_seconds()
            );
            return Command::none();
        }

        // `due` is None for two very different reasons: the game says there
        // is no cooldown at all (then `free` is true), or the character is
        // not in a state that has a game state to ask (then `free` is
        // false). The second one is the one worth naming in the log
        let (free, due) = {
            let arc = account.status.clone();
            let lock = arc.lock_safe();
            match &*lock {
                AccountStatus::Idle(_, gs) | AccountStatus::Busy(gs, _) => {
                    let due = gs.dungeons.next_free_fight;
                    (due.is_none_or(|t| t <= Local::now()), due)
                }
                _ => (false, None),
            }
        };
        // A full backpack is the quiet killer here: the game refuses the
        // fight because the loot has nowhere to go, and from the outside it
        // looks exactly like the dungeons "just stopped"
        let (bag, server_day, free_mush) = {
            let arc = account.status.clone();
            let lock = arc.lock_safe();
            match &*lock {
                AccountStatus::Idle(_, gs) | AccountStatus::Busy(gs, _) => (
                    Some(crate::backpack::space(gs)),
                    Some(gs.server_time().current().date()),
                    account.free_mushrooms(gs),
                ),
                _ => (None, None, 0),
            }
        };
        // The mushroom allowance renews with the server's day, and what
        // was spent of it is on disk - a restart used to start the count
        // over and buy the whole allowance again
        let today = server_day.unwrap_or_else(|| Local::now().date_naive());
        let spent = key.as_deref().map(|k| self.spend.today(k, today).dungeon);
        let di = &mut account.dungeon_info;
        if di.mushroom_day != Some(today) {
            di.mushroom_day = Some(today);
            di.mushroom_today = 0;
        }
        if let Some(spent) = spent {
            di.mushroom_today = di.mushroom_today.max(spent);
        }
        // `use_mushroom` decides the whole rest of this call. The free fight
        // is always taken first; only once it is gone does a mushroom come
        // into play, and only if the user turned it on and the daily ceiling
        // is not yet hit
        // ...and the day's mushroom budget (see `free_mushrooms`) has one left
        let use_mushroom = if free {
            false
        } else if di.mushroom
            && di.mushroom_today < di.mushroom_max
            && free_mush > 0
        {
            true
        } else {
            let msg = match due {
                Some(t) => format!(
                    "Auto dungeon: waiting for the free fight (due {})",
                    t.format("%H:%M:%S")
                ),
                None => "Auto dungeon: the character has no dungeon data \
                         right now - not logged in, or busy with another \
                         request"
                    .to_string(),
            };
            log::info!("{ident}: auto dungeon - {msg}");
            di.status = Some(msg);
            return Command::none();
        };
        if let Some((used, total)) = bag
            && used >= total
        {
            // Make room and come straight back, rather than letting the
            // hourly fight expire because of a bag nobody emptied
            if account.shop_info.clear_backpack
                && let Some(sale) = self.backpack_sale(ident)
            {
                return sale;
            }
            let Some(account) = self.servers.get_account_mut(&ident) else {
                return Command::none();
            };
            let msg = format!(
                "Auto dungeon: the backpack is full ({used}/{total}) and \
                 nothing in it may be sold - the game will not hand out loot \
                 it cannot store"
            );
            account.dungeon_info.status = Some(msg);
            log::info!(
                "{ident}: auto dungeon - backpack full ({used}/{total})"
            );
            return Command::none();
        }
        let Some(account) = self.servers.get_account_mut(&ident) else {
            return Command::none();
        };
        let di = &mut account.dungeon_info;

        // Pet habitat dungeons share the cooldown - and every won fight
        // there unlocks a pet, so they go first. Our strongest pet of the
        // habitat fights, but only against an enemy of at most its level
        let pets_blocked =
            di.pets_blocked_until.is_some_and(|t| t > Local::now());
        if di.pets_first && !pets_blocked {
            let min_chance = di.min_chance;
            let pick = {
                let arc = account.status.clone();
                let lock = arc.lock_safe();
                match &*lock {
                    AccountStatus::Idle(_, gs) => gs.pets.as_ref().and_then(|pets| {
                        use sf_api::gamestate::unlockables::HabitatExploration;
                        // The pet dungeons run on their own clock, not the
                        // normal dungeon's. Asking while it is still ticking
                        // only earned "the free fight is not available yet"
                        if pets
                            .next_free_exploration
                            .is_some_and(|t| t > Local::now())
                        {
                            return None;
                        }
                        let gladiator: u32 = gs
                            .underworld
                            .as_ref()
                            .map(|uw| {
                                uw.buildings[sf_api::gamestate::underworld::UnderworldBuildingType::GladiatorTrainer]
                                    .level
                                    .into()
                            })
                            .unwrap_or(0);
                        // The fight is fought in the simulator first, with
                        // every pet of every habitat: the habitat and pet
                        // with the best odds go, and only when those odds
                        // reach the minimum set for dungeons. The five
                        // habitats share the one free fight, so the first
                        // habitat that could fight must not take it from
                        // one that would win. "Level at least the enemy's"
                        // was the rule before there was a simulator - a
                        // mage pet two levels under a warrior boss can be
                        // an easy win, and the other way round a sure loss
                        pets.habitats
                            .iter()
                            .filter_map(|(habitat, info)| {
                                let HabitatExploration::Exploring {
                                    fights_won,
                                    next_fight_lvl,
                                } = info.exploration
                                else {
                                    return None;
                                };
                                let boss = crate::pet_sim::PetSpec::boss(
                                    habitat,
                                    fights_won as usize,
                                    (next_fight_lvl > 0).then_some(next_fight_lvl),
                                );
                                let ranked = crate::pet_sim::ranked(
                                    pets, habitat, gladiator, &boss,
                                );
                                let best = ranked.first()?;
                                let odds = (best.odds * 100.0).round() as u8;
                                (odds >= min_chance).then_some((
                                    habitat,
                                    fights_won + 1,
                                    best.pet_id,
                                    best.ours.level,
                                    next_fight_lvl,
                                    odds,
                                ))
                            })
                            .max_by_key(|pick| pick.5)
                    }),
                    _ => None,
                }
            };
            if let Some((habitat, enemy_pos, pet_id, lvl, enemy_lvl, odds)) = pick {
                let di = &mut account.dungeon_info;
                // The game fights with the pet that is clicked, so the
                // chosen one is selected first - the one the simulator
                // ranked best. Same as feeding does
                if di.pet_opened_for != Some(pet_id)
                    && let Ok(view_id) = u16::try_from(pet_id)
                {
                    di.status = Some(format!(
                        "Auto dungeon: selecting pet #{pet_id} (lvl {lvl}) for {habitat:?}"
                    ));
                    return self.send_simple(
                        ident,
                        "Dungeon",
                        sf_api::command::Command::ViewPet { pet_id: view_id },
                        SimpleNote::PetDungeonOpened {
                            pet_id,
                            name: format!(
                                "pet #{pet_id} (lvl {lvl}, {habitat:?})"
                            ),
                        },
                    );
                }
                di.pet_opened_for = None;
                di.last_auto_fight = Some(Local::now());
                di.status = Some(format!(
                    "Auto dungeon: pet dungeon {habitat:?} #{enemy_pos} (our lvl {lvl} vs {enemy_lvl}, ~{odds} %)"
                ));
                return self.send_simple(
                    ident,
                    "Dungeon",
                    sf_api::command::Command::FightPetDungeon {
                        use_mush: false,
                        habitat,
                        enemy_pos,
                        player_pet_id: pet_id,
                    },
                    SimpleNote::PetDungeon {
                        name: format!("Pet dungeon {habitat:?} #{enemy_pos}"),
                        habitat,
                        enemy_pos,
                        pet_id,
                        attempt: 0,
                    },
                );
            }
        }
        let di = &mut account.dungeon_info;

        // Once a day, spend the free fight learning the odds of an enemy the
        // simulation can not rate - the twister and its kind, whose damage
        // figures the library simply does not have. Never a mushroom, and
        // only for a floor the normal best-odds path would not already pick
        // (unknown, or estimated below the bar): a floor we win comfortably
        // gets fought through that path instead, on its own estimate. This is
        // the only way an estimate is ever gathered - one swing, one data
        // point, sharper every day
        if free && !use_mushroom && di.probe_tried != Some(today) {
            let bar = f64::from(di.min_chance) / 100.0;
            let probe = di
                .chances
                .iter()
                .find(|c| {
                    c.open && c.no_data && !(c.win.is_finite() && c.win >= bar)
                })
                .cloned();
            if let Some(probe) = probe {
                di.probe_tried = Some(today);
                di.pending_probe =
                    Some((probe.dungeon, probe.stage, probe.level_gap));
                di.last_auto_fight = Some(Local::now());
                let odds = if probe.win.is_finite() {
                    format!("~{:.0}% so far", probe.win * 100.0)
                } else {
                    "odds still unknown".to_string()
                };
                di.status = Some(format!(
                    "Auto dungeon: daily probe on {} ({odds}) - the game gives \
                     no data to simulate this one, so we learn it by fighting",
                    probe.name
                ));
                log::info!(
                    "{ident}: auto dungeon - daily probe on {} ({odds})",
                    probe.name
                );
                return Command::perform(async {}, move |()| {
                    Message::DungeonFight {
                        ident,
                        dungeon: probe.dungeon,
                        name: probe.name.clone(),
                        attempt: 0,
                        mushroom: false,
                    }
                });
            }
        }

        // A mushroom fight has a higher bar than the free one and never
        // settles for unknown odds - the free fight is expendable, a mushroom
        // is not. A mushroom also never goes on an enemy we can only guess at
        // (no_data): a guess is not worth a premium currency
        let min = if use_mushroom {
            f64::from(di.mushroom_min) / 100.0
        } else {
            f64::from(di.min_chance) / 100.0
        };
        let best = di
            .chances
            .iter()
            .filter(|c| {
                c.open
                    && c.win.is_finite()
                    && c.win >= min
                    && (!use_mushroom || !c.no_data)
            })
            .max_by(|a, b| a.win.partial_cmp(&b.win).unwrap())
            .cloned();

        // Losing a dungeon fight costs nothing but the fight itself, and the
        // fight is gone in an hour whether it is used or not. So when nothing
        // clears the minimum, the best odds on offer still beat letting the
        // hour pass - including an enemy the simulation knows nothing about
        // (win = NaN), which is how the twister and every freshly opened
        // shadow dungeon used to sit there untouched forever
        let mut settled_for_less = false;
        let best = best.or_else(|| {
            // Never spend a mushroom on a fight that did not clear the bar
            if use_mushroom {
                return None;
            }
            settled_for_less = true;
            let rated = di
                .chances
                .iter()
                .filter(|c| c.open && c.win.is_finite())
                .max_by(|a, b| a.win.partial_cmp(&b.win).unwrap())
                .cloned();
            rated.or_else(|| {
                di.chances
                    .iter()
                    .find(|c| c.open && !c.win.is_finite())
                    .cloned()
            })
        });

        let Some(best) = best else {
            let msg = if use_mushroom {
                format!(
                    "Auto dungeon: no dungeon reaches {}% - not spending a \
                     mushroom",
                    di.mushroom_min
                )
            } else {
                "Auto dungeon: no dungeon is open - waiting".to_string()
            };
            log::info!("{ident}: auto dungeon - {msg}");
            di.status = Some(msg);
            return Command::none();
        };
        if settled_for_less {
            log::info!(
                "{ident}: auto dungeon - nothing reaches {}%, taking the best \
                 odds anyway rather than wasting the fight",
                di.min_chance
            );
        }

        di.last_auto_fight = Some(Local::now());
        if use_mushroom {
            di.mushroom_today = di.mushroom_today.saturating_add(1);
            // On disk before the request goes out
            if let Some(k) = key.as_deref() {
                let on_disk = self
                    .spend
                    .bump(k, today, crate::spend_ledger::Spend::DungeonMushroom)
                    .dungeon;
                di.mushroom_today = di.mushroom_today.max(on_disk);
            }
        }
        // A fight on an enemy we can only estimate still teaches us something,
        // so credit its result to the floor once it comes back
        di.pending_probe = if best.no_data {
            Some((best.dungeon, best.stage, best.level_gap))
        } else {
            None
        };
        let odds = if best.win.is_finite() {
            format!("{:.0}%", best.win * 100.0)
        } else {
            "odds unknown".to_string()
        };
        di.status = Some(if use_mushroom {
            format!(
                "Auto dungeon: mushroom fight on {} ({odds}) - {}/{} today",
                best.name, di.mushroom_today, di.mushroom_max
            )
        } else if settled_for_less {
            format!(
                "Auto dungeon: fighting {} ({odds}) - nothing reached {}%, \
                 but the fight would expire unused",
                best.name, di.min_chance
            )
        } else {
            format!("Auto dungeon: fighting {} ({odds})", best.name)
        });
        log::info!(
            "{ident}: auto dungeon - {}{} ({odds})",
            if use_mushroom { "mushroom " } else { "" },
            best.name
        );
        Command::perform(async {}, move |()| Message::DungeonFight {
            ident,
            dungeon: best.dungeon,
            name: best.name.clone(),
            attempt: 0,
            mushroom: use_mushroom,
        })
    }

    /// One step of the expedition state machine: start one, pick a path,
    /// fight the boss, take a reward, skip or sit out the wait. Every step is
    /// one request; the result handler calls this again
    fn expedition_step(&mut self, ident: AccountIdent) -> Command<Message> {
        use sf_api::{
            command::{Command as SFCommand, ExpeditionSetting, TimeSkip},
            gamestate::tavern::{CurrentAction, ExpeditionStage},
        };

        // The thirst for adventure is capped at 100 min, and each beer adds
        // 20 min. A beer is only accepted while the current thirst leaves room
        // for that full 20 min (<= 80 min); above it the game answers "you are
        // not thirsty" and the mushroom paid for the beer is wasted.
        const BEER_ROOM_SEC: u32 = (100 - 20) * 60;

        let Some(account) = self.servers.get_account_mut(&ident) else {
            return Command::none();
        };
        if !account.expedition_info.auto {
            return Command::none();
        }
        // Until something actually goes out, the tavern is stalled and the
        // rest of the queue may run. Every early return below (a guard
        // shift, a quest, a busy character, the loop guard) leaves it that
        // way; only a request that comes back clears it. Setting it on one
        // path and forgetting the others is how a night on the city guard
        // once froze every other job of the character until morning
        account.expedition_info.stalled = true;
        let ei = &account.expedition_info;
        if ei.steps_this_round >= 40 {
            account.expedition_info.status =
                Some("Pausing until the next round".to_string());
            return Command::none();
        }
        let (goal, skip_glass, skip_mushroom, quest_auto) =
            (ei.goal, ei.skip_glass, ei.skip_mushroom, ei.quest_auto);

        let arc = account.status.clone();
        let status = arc.lock_safe();
        let AccountStatus::Idle(_, gs) = &*status else {
            return Command::none();
        };
        let tavern = &gs.tavern;
        // Mushrooms the tavern may burn - the reserve stays untouched. A VIP
        // character pays nothing for beer, so the reserve does not gate it
        let free_mush = account.free_mushrooms(gs);
        let beer_paid_for = account.expedition_info.beer_affordable(free_mush, account.tavern_info.beer_free);
        let now = Local::now();

        // A new server day: the tavern in the state is yesterday's (the
        // poll never refreshes it), so the character logs in again once,
        // a few minutes after midnight, and reads the real thing. The first
        // look after a login only notes the day
        let server_now = gs.server_time().current();
        let server_day = server_now.date();
        // Not at 00:03 on the dot for everybody - each character has its
        // own minute of the night, fixed for the day (see `stealth`)
        let past_reset = server_now.time()
            >= chrono::NaiveTime::from_hms_opt(
                0,
                crate::stealth::relogin_minute(&account.name, server_day),
                0,
            )
            .unwrap_or_default();
        match account.expedition_info.day_read {
            None => account.expedition_info.day_read = Some(server_day),
            Some(d) if d != server_day && past_reset => {
                drop(status);
                account.expedition_info.day_read = Some(server_day);
                account.expedition_info.beer_blocked_until = None;
                let mut lock = arc.lock_safe();
                if let Some(session) = lock.take_session("New day") {
                    drop(lock);
                    self.console.push_back(crate::ConsoleLine {
                        at: now,
                        who: titlecase::titlecase(&account.name),
                        text: "New day - logging in again to read today's \
                               tavern (the poll does not carry it)"
                            .to_string(),
                        ok: None,
                        dup: false,
                    });
                    log::info!(
                        "{ident}: new server day - relogin for a fresh state"
                    );
                    return Command::perform(async {}, move |()| {
                        Message::PlayerCommandFailed {
                            ident,
                            session,
                            attempt: 0,
                        }
                    });
                }
                return Command::none();
            }
            _ => {}
        }
        let beer_blocked = account
            .expedition_info
            .beer_blocked_until
            .is_some_and(|t| t > now);

        // Something else is using the character's time
        if let CurrentAction::Quest { busy_until, .. } = &tavern.current_action
            && *busy_until <= now
        {
            drop(status);
            account.expedition_info.steps_this_round += 1;
            return self.send_simple(
                ident,
                "Expedition",
                SFCommand::FinishQuest { skip: None },
                SimpleNote::Expedition(
                    "Finished the quest that was over".into(),
                ),
            );
        }
        // A quest still running is skipped with a quicksand glass, the same
        // switch as the expedition waits. The glasses are there to be used -
        // a day's thirst done in minutes instead of hours is the point of
        // having them, and the shop buys new ones whenever it has them
        if let CurrentAction::Quest { busy_until, .. } = &tavern.current_action
            && *busy_until > now
            && skip_glass
            && tavern.quicksand_glasses > 0
        {
            drop(status);
            account.expedition_info.steps_this_round += 1;
            return self.send_simple(
                ident,
                "Expedition",
                SFCommand::FinishQuest {
                    skip: Some(TimeSkip::Glass),
                },
                SimpleNote::Expedition(
                    "Finished the quest with a quicksand glass".into(),
                ),
            );
        }
        // A shift or quest that is over blocks the tavern until it is
        // collected - the tavern step is the one that looks most often, so
        // it hands that to the guard job right away instead of leaving it
        // to the guard checker's half-hour tick
        let (busy, over) = match &tavern.current_action {
            CurrentAction::Quest { busy_until, .. } => (
                Some(format!(
                    "On a quest until {}",
                    busy_until.format("%H:%M")
                )),
                *busy_until <= now,
            ),
            CurrentAction::CityGuard { busy_until, .. } => (
                Some(format!(
                    "Working as city guard until {}",
                    busy_until.format("%H:%M")
                )),
                *busy_until <= now,
            ),
            _ => (None, false),
        };
        if let Some(msg) = busy {
            drop(status);
            if over {
                account.expedition_info.status =
                    Some("Shift over - collecting the wage".to_string());
                return Command::perform(async {}, move |()| {
                    Message::Enqueue {
                        ident,
                        job: Job::Guard,
                    }
                });
            }
            account.expedition_info.status = Some(msg);
            return Command::none();
        }

        // Beer comes in batches, the guild leader's way: play the thirst
        // down to nothing, then five beers in a row (a full 100 min), play
        // that down, five more, and the eleventh with the last quest. The
        // batch starts below, where nothing fits into the thirst any more;
        // this block only keeps a started batch going until the cap
        {
            let allowance = account.beer_allowance(gs);
            let ti = &account.tavern_info;
            let beers_left = allowance
                .min(tavern.beer_max)
                .saturating_sub(tavern.beer_drunk);
            let can_drink = ti.beer_auto
                && !beer_blocked
                && beers_left > 0
                && beer_paid_for
                && tavern.expeditions.active().is_none()
                && account.expedition_info.steps_this_round < 30
                // The game refuses a beer with "you are not thirsty" unless
                // there is room for the full +20 min under the 100 min thirst
                // cap - so the batch ends at 80 min
                && tavern.thirst_for_adventure_sec <= BEER_ROOM_SEC;
            if account.expedition_info.beer_batch && can_drink {
                drop(status);
                account.expedition_info.steps_this_round += 1;
                account.expedition_info.status = Some(format!(
                    "Filling the thirst - {beers_left} beers to go"
                ));
                return self.send_simple(
                    ident,
                    "Tavern",
                    SFCommand::BuyBeer,
                    SimpleNote::Beer,
                );
            }
            // The batch is over: the thirst is full, or the beers ran out
            account.expedition_info.beer_batch = false;
        }

        let mut wait_msg: Option<String> = None;
        // Set when the thirst is simply too short for anything on offer - a
        // beer refills it, and only then is buying one the right move
        let mut want_beer = false;
        // Is the tavern finished for today? This is read straight off the
        // state, not set inside one of the branches below - the first
        // version only set it on the path through the beer check, so a
        // character with quests switched off, or one that had just finished
        // an expedition, never reached it and the shift never started
        let tavern_finished = {
            let allowance = account.beer_allowance(gs);
            let ti = &account.tavern_info;
            let beers_left = allowance
                .min(tavern.beer_max)
                .saturating_sub(tavern.beer_drunk);
            let beer_possible = ti.beer_auto && beers_left > 0 && beer_paid_for;
            tavern.thirst_for_adventure_sec == 0
                && !beer_possible
                && tavern.expeditions.active().is_none()
        };
        let action: Option<(SFCommand, String)> = if let Some(active) =
            tavern.expeditions.active()
        {
            let floor = active.current_floor;
            match active.current_stage() {
                ExpeditionStage::Encounters(list) => {
                    let crossroads = crate::expedition::Crossroads {
                        // The four item slots decide everything: a chain is
                        // only worth following while its earlier links are
                        // actually being carried
                        items: &active.items,
                        target: active.target_thing,
                        target_missing: active.target_current
                            < active.target_amount,
                        // `current_floor` is the clearing being decided now,
                        // so it counts as one of the ones still ahead
                        floors_left: 11u8.saturating_sub(floor),
                        heroism: active.heroism,
                        goal,
                    };
                    match crate::expedition::pick_encounter(&list, &crossroads)
                    {
                        Some((pos, _score, why)) => {
                            let after = active.heroism + list[pos].heroism;
                            let mut msg = format!(
                                "Clearing {floor}/10: took the {} ({:+} \
                                 heroism) - {why}",
                                crate::expedition::thing_name(list[pos].typ),
                                list[pos].heroism
                            );
                            // Call it out the moment 40 is reached, even mid
                            // expedition - that is the +35 % XP bonus locked in
                            if active.heroism < 40 && after >= 40 {
                                msg.push_str(
                                    " >>> 40 heroism reached, +35% XP secured!",
                                );
                            }
                            Some((
                                SFCommand::ExpeditionPickEncounter { pos },
                                msg,
                            ))
                        }
                        None => Some((
                            SFCommand::Update,
                            "Checked the expedition".to_string(),
                        )),
                    }
                }
                ExpeditionStage::Boss(_) => Some((
                    SFCommand::ExpeditionContinue,
                    format!("Clearing {floor}: fought the boss"),
                )),
                ExpeditionStage::Rewards(rewards) => {
                    if rewards.len() <= 1 {
                        Some((
                            SFCommand::ExpeditionContinue,
                            "Collected the reward".to_string(),
                        ))
                    } else {
                        let pos =
                            crate::expedition::pick_reward(&rewards, goal);
                        Some((
                            SFCommand::ExpeditionPickReward { pos },
                            format!(
                                "Boss reward: took {}",
                                crate::expedition::reward_name(&rewards[pos])
                            ),
                        ))
                    }
                }
                ExpeditionStage::Waiting { busy_until, .. } => {
                    if busy_until <= now {
                        Some((
                            SFCommand::Update,
                            "Checked the expedition".into(),
                        ))
                    } else if skip_glass && tavern.quicksand_glasses > 0 {
                        Some((
                            SFCommand::ExpeditionSkipWait {
                                typ: TimeSkip::Glass,
                            },
                            "Skipped the wait with a quicksand glass".into(),
                        ))
                    } else if skip_mushroom
                        && tavern.mushroom_skip_allowed
                        && gs.character.mushrooms > 0
                    {
                        Some((
                            SFCommand::ExpeditionSkipWait {
                                typ: TimeSkip::Mushroom,
                            },
                            "Skipped the wait with a mushroom".into(),
                        ))
                    } else {
                        wait_msg = Some(format!(
                            "Waiting until {} (heroism {}, clearing {}/10)",
                            busy_until.format("%H:%M:%S"),
                            active.heroism,
                            floor
                        ));
                        None
                    }
                }
                ExpeditionStage::Finished | ExpeditionStage::Unknown => {
                    Some((SFCommand::Update, "Checked the expedition".into()))
                }
            }
        } else if tavern.current_action == CurrentAction::Expedition {
            // The last timer ran out - the server has to tell us it is over
            Some((SFCommand::Update, "Checked the expedition".into()))
        } else {
            let event = tavern.expeditions.is_event_ongoing();
            let on_quests =
                tavern.questing_preference == ExpeditionSetting::PreferQuests;
            if event && on_quests && tavern.can_change_questing_preference() {
                // Expeditions beat quests, and the setting can still be moved
                Some((
                    SFCommand::SetQuestsInsteadOfExpeditions {
                        value: ExpeditionSetting::PreferExpeditions,
                    },
                    "Switched the tavern to expeditions".into(),
                ))
            } else if event && !on_quests {
                // An expedition event is running, so quests are not on offer
                // at all - the thirst goes into expeditions
                match crate::expedition::pick_expedition(
                    &tavern.expeditions.available,
                    tavern.thirst_for_adventure_sec, goal,
                ) {
                    Some((pos, why)) => {
                        let e = &tavern.expeditions.available[pos];
                        Some((
                            SFCommand::ExpeditionStart { pos },
                            format!(
                                "Started an expedition for the {} ({} min) - \
                                 {why}",
                                crate::expedition::thing_name(e.target),
                                e.thirst_for_adventure_sec / 60
                            ),
                        ))
                    }
                    None => {
                        want_beer = true;
                        None
                    }
                }
            } else if !quest_auto {
                wait_msg = Some(
                    if event {
                        "The tavern is set to quests for today and quests are \
                         switched off"
                    } else {
                        "No expedition event right now and quests are \
                         switched off - the thirst is going to waste"
                    }
                    .to_string(),
                );
                None
            } else {
                // Quests are the only way to spend the thirst: either no
                // event is running, or the tavern is locked to quests for
                // today
                match crate::quest::pick_quest(
                    &tavern.quests, tavern.thirst_for_adventure_sec, goal,
                ) {
                    Some(pick) => Some((
                        SFCommand::StartQuest {
                            quest_pos: pick.pos,
                            overwrite_inv: false,
                        },
                        format!(
                            "Started quest {} - {}",
                            pick.pos + 1,
                            pick.why
                        ),
                    )),
                    None => {
                        want_beer = true;
                        None
                    }
                }
            }
        };

        // Nothing fits into the thirst that is left. A beer buys 20 more
        // minutes of it, and that is always better than letting it sit
        let action = match action {
            Some(a) => Some(a),
            None if !want_beer => None,
            None => {
                let allowance = account.beer_allowance(gs);
                let ti = &account.tavern_info;
                if ti.beer_auto
                    && !beer_blocked
                    && tavern.beer_drunk < allowance.min(tavern.beer_max)
                    && beer_paid_for
                    // Same cap as above: a beer only goes down when the thirst
                    // has room for the full +20 min, otherwise the game just
                    // answers "you are not thirsty" and the mushroom is wasted.
                    && tavern.thirst_for_adventure_sec <= BEER_ROOM_SEC
                {
                    drop(status);
                    account.expedition_info.steps_this_round += 1;
                    // First beer of a batch - the block at the top keeps
                    // pouring until the thirst is full again
                    account.expedition_info.beer_batch = true;
                    return self.send_simple(
                        ident,
                        "Tavern",
                        SFCommand::BuyBeer,
                        SimpleNote::Beer,
                    );
                }
                if wait_msg.is_none() && ti.beer_auto && !beer_paid_for {
                    wait_msg = Some(crate::i18n::t("Not enough mushrooms for beer. Waiting for more funds.").to_string());
                }
                if wait_msg.is_none() {
                    wait_msg = Some(format!(
                        "Nothing left to do with {} min of thirst{}",
                        tavern.thirst_for_adventure_sec / 60,
                        if ti.beer_auto && !beer_paid_for {
                            " - no mushrooms free for beer (today's +1 is kept, and the dragon's 25 when its renewal is within 3 days)"
                        } else if ti.beer_auto
                            && tavern.thirst_for_adventure_sec > BEER_ROOM_SEC
                        {
                            " - too much thirst left to drink another beer"
                        } else if ti.beer_auto {
                            " - beer limit reached"
                        } else {
                            ""
                        }
                    ));
                }
                None
            }
        };
        drop(status);

        let Some((cmd, what)) = action else {
            if let Some(msg) = wait_msg {
                account.expedition_info.status = Some(msg);
            }
            // Nothing to send: the other jobs may have the character now
            account.expedition_info.stalled = true;
            account.expedition_info.steps_this_round = 0;
            // The verdict the guard waits for - and only the tavern's own
            account.expedition_info.tavern_done_at =
                tavern_finished.then_some(now);
            // The hand-over is one line and one queued job, not one every
            // tavern check - the guard job may well be waiting for its
            // cut-off time, and the log said "done for today" every minute
            let handed_recently = account
                .expedition_info
                .guard_handoff
                .is_some_and(|t| now - t < chrono::Duration::minutes(15));
            if tavern_finished && account.guard_info.auto && !handed_recently {
                account.expedition_info.guard_handoff = Some(now);
                account.expedition_info.status = Some(
                    "Done for today - handing over to the city guard"
                        .to_string(),
                );
                if self.config.sounds {
                    crate::sound::play(crate::sound::Sound::Celebrate);
                }
                log::info!(
                    "{ident}: the tavern is done for today - handing the \
                     character over to the city guard"
                );
                return Command::perform(async {}, move |()| {
                    Message::Enqueue {
                        ident,
                        job: Job::Guard,
                    }
                });
            }
            return Command::none();
        };
        account.expedition_info.steps_this_round += 1;
        self.send_simple(ident, "Expedition", cmd, SimpleNote::Expedition(what))
    }

    /// The game's money figure covers nothing: buy one level of the
    /// cheapest building, so the game works the money out and says what is
    /// really in hand (see `mechanics::idle_cheapest_level`). At most once
    /// in five minutes after a refusal; otherwise the round ends here
    fn idle_purse_check(
        &mut self,
        ident: AccountIdent,
        check: Option<(sf_api::gamestate::idle::IdleBuildingType, f64, u32)>,
        purse: &str,
        now: DateTime<Local>,
    ) -> Command<Message> {
        use sf_api::command::{Command as SFCommand, IdleUpgradeAmount};
        let Some(account) = self.servers.get_account_mut(&ident) else {
            return Command::none();
        };
        let paused = account
            .idle_info
            .probe_paused_until
            .is_some_and(|t| now < t);
        if let Some((typ, cost, level)) = check
            && !paused
        {
            account.idle_info.pending_upgrade = Some((typ, level));
            account.idle_info.bought_this_round += 1;
            log::info!(
                "{ident}: arena manager - the game's figure ({purse}) covers \
                 nothing; buying one {typ:?} level for {} so the game works \
                 the money out",
                human_money(cost)
            );
            return self.send_simple(
                ident,
                "Arena Manager",
                SFCommand::IdleUpgrade {
                    typ,
                    amount: IdleUpgradeAmount::One,
                },
                SimpleNote::IdleBought(format!(
                    "Upgraded {typ:?} x1 for {} (purse check)",
                    human_money(cost)
                )),
            );
        }
        log::info!(
            "{ident}: arena manager - nothing affordable with {purse} in hand{}",
            if paused { " (the game said no a moment ago)" } else { "" }
        );
        account.idle_info.status = Some(format!(
            "{} ({})",
            crate::i18n::t("Nothing affordable right now"),
            now.format("%H:%M")
        ));
        account.idle_info.bought_this_round = 0;
        Command::none()
    }

    /// Buys the Arena Manager upgrade with the best return on investment
    /// that the current money covers. Return = extra earnings per second per
    /// coin spent; earnings are assumed to grow linearly with the level
    fn idle_upgrade(&mut self, ident: AccountIdent) -> Command<Message> {
        use num_traits::ToPrimitive;
        use sf_api::command::{Command as SFCommand, IdleUpgradeAmount};

        let Some(account) = self.servers.get_account_mut(&ident) else {
            return Command::none();
        };
        let (upgrade, collect) = (
            account.idle_info.auto_upgrade, account.idle_info.auto_collect,
        );
        if !upgrade && !collect && !account.idle_info.auto_sacrifice {
            return Command::none();
        }
        // Twenty-five in a row per turn (it was ten, and four before
        // yielding to the queue): the arena manager is what the guild
        // leader runs this app for, and "it buys too little" was the
        // complaint. Bundles below make each of those count
        if account.idle_info.bought_this_round >= 25 {
            log::info!(
                "{ident}: arena manager - 25 purchases this turn, letting the \
                 rest of the queue through"
            );
            account.idle_info.status =
                Some("Pausing until the next round".to_string());
            return Command::none();
        }

        let arc = account.status.clone();
        let status = arc.lock_safe();
        let AccountStatus::Idle(_, gs) = &*status else {
            // It used to leave without a word, so the log showed "running
            // Arena Manager" and then nothing at all - the one line that
            // says why is the difference between a bug report and a guess
            drop(status);
            // Debug only: the ticks come in bursts and this line came out
            // six times a second
            log::debug!(
                "{ident}: arena manager - the character is busy with another \
                 request, trying again next round"
            );
            return Command::none();
        };
        let Some(idle) = gs.idle_game.as_ref() else {
            drop(status);
            log::info!(
                "{ident}: arena manager - the game did not send Arena \
                 Manager data, trying again next round"
            );
            account.idle_info.status = Some(
                "The game did not send Arena Manager data for this character"
                    .to_string(),
            );
            return Command::none();
        };

        // Sacrifice (reset) when the reset itself is worth at least the
        // step the player set. **`sacrifice_runes` is what a reset would
        // hand over right now** - it is what the game's own button shows,
        // it grows with the money earned since the last reset, and it
        // drops back to nothing the moment one happens.
        //
        // What stood here compared `current_runes` - the runes already
        // *owned* - against multiples of the step. With 87 711 runes in
        // the bank that test came true again every twenty runes, for
        // ever, so the app asked for a reset that had nothing to give and
        // the game answered "does not allow this right now", over and
        // over. The guard below is the game's own `resets` counter: we do
        // not ask twice on the same count, so a refusal costs one request
        // and not a loop
        //
        // First: a reset the app did not make. The counter moved since the
        // last look and it was not us who asked - so somebody reset by
        // hand, and the offer that stood here on the last look is what
        // that reset handed over. It goes on the ladder like one of ours,
        // or an account run by hand for months has nothing on record and
        // the bar falls back to the floor for ever
        {
            let offer_now = u64::try_from(crate::mechanics::runes_of(&idle.sacrifice_runes))
                .unwrap_or(u64::MAX);
            let ii = &mut account.idle_info;
            let by_hand = ii
                .seen_resets
                .is_some_and(|seen| idle.resets > seen)
                && ii.sacrificed_at_resets != Some(idle.resets)
                && ii.seen_offer > 0;
            if by_hand {
                let handed = ii.seen_offer;
                ii.last_sacrifice_runes = handed;
                let msg =
                    format!("Reset by hand noticed - {handed} runes handed over");
                ii.log.push((Local::now(), msg, true));
                let name = account.name.clone();
                let last = account.idle_info.last_sacrifice_runes;
                if let Some(conf) =
                    self.config.get_char_conf_mut(&name, ident.server_id)
                {
                    conf.idle_last_sacrifice = last;
                    _ = self.config.write();
                }
            }
            let ii = &mut account.idle_info;
            ii.seen_resets = Some(idle.resets);
            ii.seen_offer = offer_now;
        }
        if account.idle_info.auto_sacrifice {
            // Use exactly the threshold shown in the settings.
            let bar = account.idle_info.sacrifice_target();
            let offer = crate::mechanics::runes_of(&idle.sacrifice_runes);
            let owned_now = crate::mechanics::runes_of(&idle.current_runes);
            let asked_already =
                account.idle_info.sacrificed_at_resets == Some(idle.resets);
            let worth =
                crate::mechanics::should_sacrifice(owned_now, offer, bar);
            if worth && !asked_already {
                account.idle_info.sacrificed_at_resets = Some(idle.resets);
                // Kept for the log only - the bar is a share of the runes
                // owned and needs nothing remembered
                account.idle_info.last_sacrifice_runes =
                    u64::try_from(offer).unwrap_or(u64::MAX);
                let owned = owned_now;
                let gain =
                    crate::mechanics::sacrifice_gain_pct(owned, offer);
                let what = format!(
                    "Sacrificed for {offer} runes - that is +{gain:.1} % on \
                     top of what the {owned} runes already earn"
                );
                drop(status);
                // Remembered in the config, so a restart does not ask again
                let name = account.name.clone();
                let last = account.idle_info.last_sacrifice_runes;
                let at = u64::from(
                    account.idle_info.sacrificed_at_resets.unwrap_or(0),
                );
                if let Some(conf) =
                    self.config.get_char_conf_mut(&name, ident.server_id)
                {
                    conf.idle_last_sacrifice = last;
                    conf.idle_sacrificed_units = at;
                    _ = self.config.write();
                }
                let Some(account) = self.servers.get_account_mut(&ident) else {
                    return Command::none();
                };
                account.idle_info.bought_this_round += 1;
                return self.send_simple(
                    ident,
                    "Arena Manager",
                    SFCommand::IdleSacrifice,
                    SimpleNote::IdleBought(what),
                );
            }
        }

        // Collect first: a building whose cycle is over sits idle until it is
        // clicked. The official client sends this as its own command, which
        // the library does not wrap, hence the raw form
        if collect {
            let now = Local::now();
            let ready = idle
                .buildings
                .iter()
                .filter(|(_, b)| b.level > 0)
                .find(|(_, b)| b.cycle_end.is_some_and(|e| e <= now))
                .map(|(t, _)| t);
            if let Some(typ) = ready {
                drop(status);
                account.idle_info.bought_this_round += 1;
                return self.send_simple(
                    ident,
                    "Arena Manager",
                    SFCommand::Custom {
                        cmd_name: "IdleCollect".to_string(),
                        arguments: vec![(typ as usize).to_string()],
                    },
                    SimpleNote::IdleCollected(format!("Collected {typ:?}")),
                );
            }
        }
        if !upgrade {
            drop(status);
            log::info!("{ident}: arena manager - buying is switched off");
            account.idle_info.status = Some(format!(
                "Nothing to collect ({})",
                Local::now().format("%H:%M")
            ));
            account.idle_info.bought_this_round = 0;
            return Command::none();
        }

        // **The game's money figure is a snapshot, not a purse.** The
        // server works the Arena Manager money out only when it answers a
        // purchase, and every other answer, the login included, hands back
        // the same old number. Once it covered nothing, nothing was bought,
        // so nothing came back and the number never moved - seventeen
        // hours on 27-28 October. When it covers nothing now, the app buys
        // the cheapest single level instead (`idle_purse_check`): the
        // answer carries the real money, and the round goes on with it.
        // Estimating the income on the side was tried in 6.2.7 and 6.2.8
        // and bought with money the game had not paid out
        let now = Local::now();
        let money = idle.current_money.to_f64().unwrap_or(0.0);
        let purse = human_money(money);
        if !money.is_finite() || money <= 0.0 {
            let check = crate::mechanics::idle_cheapest_level(idle);
            drop(status);
            return self.idle_purse_check(ident, check, &purse, now);
        }
        // The guild leader's optimal build path comes first: bring the next
        // building up to its next breakpoint, in the fixed order. Only once
        // the whole path is walked does the return-on-investment logic below
        // take over for whatever is left
        if let Some((typ, need)) = idle_next_on_path(idle) {
            let b = &idle.buildings[typ];
            let cost1 = b.upgrade_cost.to_f64().unwrap_or(f64::INFINITY);
            let c10 = b.upgrade_cost_10x.to_f64().unwrap_or(f64::INFINITY);
            let c25 = b.upgrade_cost_25x.to_f64().unwrap_or(f64::INFINITY);
            let c100 = b.upgrade_cost_100x.to_f64().unwrap_or(f64::INFINITY);
            // The biggest bundle that neither overshoots the breakpoint (which
            // would spend on a building the plan wants to leave for later) nor
            // costs more than the money in hand
            // Rich enough that the breakpoint does not matter: overshooting
            // it by a bundle costs a sliver of the money in hand, while
            // walking up to it one level per request took dozens of
            // requests ("Seat x1 for 26 out of 44 400 Q")
            let choice = idle_path_bundle(money, need, [cost1, c10, c25, c100]);
            // **The build path only gets its way when it can pay today.**
            // It used to sit on the money until it could, and money in
            // the arena manager does nothing whatsoever: the only thing
            // it buys is levels, and the levels are what raise both the
            // earnings and what a sacrifice is worth. Saving for a
            // breakpoint that costs a trillion while a million sits in
            // hand is the app watching the one number it is there to grow
            // stand still - which is exactly what "it just sits there"
            // looked like. So when the path cannot be paid, it stands
            // aside and the return-on-investment pass below spends what
            // the money does cover
            if let Some((amount, count, spent)) = choice {
                account.idle_info.pending_upgrade = Some((typ, b.level));
                drop(status);
                account.idle_info.bought_this_round += 1;
                log::info!(
                    "{ident}: arena manager - build path: {typ:?} x{count} \
                     for {} out of {purse}",
                    human_money(spent),
                );
                return self.send_simple(
                    ident,
                    "Arena Manager",
                    SFCommand::IdleUpgrade { typ, amount },
                    SimpleNote::IdleBought(format!(
                        "Upgraded {typ:?} x{count} for {} (build path)",
                        human_money(spent)
                    )),
                );
            }
            log::info!(
                "{ident}: arena manager - the build path wants {typ:?} next \
                 and it is out of reach ({purse} in hand) - spending on \
                 whatever the money covers instead"
            );
        }

        let mut best: Option<(f64, _, f64)> = None;
        let mut cheapest: Option<(f64, _)> = None;
        // A building that has never been built earns nothing and sits at
        // level 0, so the return-on-investment sum below cannot score it and
        // used to skip it. It would then only ever be bought when nothing
        // else was affordable - which, while there is any cheap upgrade
        // left, is never. A new building is the biggest jump on offer, so it
        // goes first
        let mut unbuilt: Option<(f64, _)> = None;
        // Bulk prices per building, so that a rich player does not have to
        // wait for hundreds of single purchases
        let mut bulk: std::collections::HashMap<usize, (f64, f64, f64)> =
            Default::default();
        for (typ, b) in &idle.buildings {
            let cost = b.upgrade_cost.to_f64().unwrap_or(f64::INFINITY);
            bulk.insert(
                typ as usize,
                (
                    b.upgrade_cost_10x.to_f64().unwrap_or(f64::INFINITY),
                    b.upgrade_cost_25x.to_f64().unwrap_or(f64::INFINITY),
                    b.upgrade_cost_100x.to_f64().unwrap_or(f64::INFINITY),
                ),
            );
            if cost <= 0.0 || cost > money {
                continue;
            }
            if cheapest.is_none_or(|(c, _)| cost < c) {
                cheapest = Some((cost, typ));
            }
            let cycle = match (b.cycle_start, b.cycle_end) {
                (Some(s), Some(e)) => (e - s).num_seconds().max(1) as f64,
                _ => 60.0,
            };
            let earning = b.earning.to_f64().unwrap_or(0.0);
            if b.level == 0 {
                if unbuilt.is_none_or(|(c, _)| cost < c) {
                    unbuilt = Some((cost, typ));
                }
                continue;
            }
            if earning <= 0.0 {
                continue;
            }
            let per_level = earning / f64::from(b.level);
            let roi = per_level / cycle / cost;
            if best.is_none_or(|(r, _, _)| roi > r) {
                best = Some((roi, typ, cost));
            }
        }
        let pick = unbuilt
            .map(|(c, t)| (t, c))
            .or_else(|| best.map(|(_, t, c)| (t, c)))
            .or_else(|| cheapest.map(|(c, t)| (t, c)));
        let Some((typ, cost)) = pick else {
            let check = crate::mechanics::idle_cheapest_level(idle);
            drop(status);
            return self.idle_purse_check(ident, check, &purse, now);
        };

        // The biggest bundle the money covers. Buying x100 of the building
        // with the best return is still the best return, just in one request
        let (c10, c25, c100) = bulk.get(&(typ as usize)).copied().unwrap_or((
            f64::INFINITY,
            f64::INFINITY,
            f64::INFINITY,
        ));
        let (amount, count, spent) = if c100 > 0.0 && c100 <= money {
            (IdleUpgradeAmount::Hundred, 100, c100)
        } else if c25 > 0.0 && c25 <= money {
            (IdleUpgradeAmount::TwentyFive, 25, c25)
        } else if c10 > 0.0 && c10 <= money {
            (IdleUpgradeAmount::Ten, 10, c10)
        } else {
            (IdleUpgradeAmount::One, 1, cost)
        };

        account.idle_info.pending_upgrade = Some((typ, idle.buildings[typ].level));
        drop(status);
        account.idle_info.bought_this_round += 1;
        log::info!(
            "{ident}: arena manager - buying {typ:?} x{count} for {} out of \
             {purse}",
            human_money(spent),
        );
        self.send_simple(
            ident,
            "Arena Manager",
            SFCommand::IdleUpgrade { typ, amount },
            SimpleNote::IdleBought(format!(
                "Upgraded {typ:?} x{count} for {}",
                human_money(spent)
            )),
        )
    }

    /// City guard: finish a shift that is over, start a new one when the
    /// character has nothing better to do. Quests and expeditions come
    /// first - a shift only starts once the thirst for adventure is used up
    /// (or expeditions are not automated at all, then the guard is what the
    /// character does with its time)
    fn guard_step(&mut self, ident: AccountIdent) -> Command<Message> {
        use sf_api::gamestate::tavern::CurrentAction;
        let Some(account) = self.servers.get_account_mut(&ident) else {
            return Command::none();
        };
        let arc = account.status.clone();
        let status = arc.lock_safe();
        let AccountStatus::Idle(_, gs) = &*status else {
            return Command::none();
        };
        let tavern = &gs.tavern;
        let now = Local::now();
        let hours = account.guard_info.hours.clamp(1, 10);
        let thirst = tavern.thirst_for_adventure_sec;
        // The shift blocks the tavern for hours, so it may only start once
        // the tavern is completely done: no thirst left and no way to buy
        // more of it. Whether that thirst goes into expeditions or into
        // quests does not matter - it is the same thirst
        let tavern_auto = account.expedition_info.auto;
        let expedition_running = tavern.expeditions.active().is_some();
        let beers_left = account
            .beer_allowance(gs)
            .min(tavern.beer_max)
            .saturating_sub(tavern.beer_drunk);
        let beer_possible = account.tavern_info.beer_auto
            && beers_left > 0
            && account.expedition_info.beer_affordable(account.free_mushrooms(gs), account.tavern_info.beer_free);
        // Seconds until the game's day rolls over and the thirst comes
        // back. A shift started just before that locks the character out of
        // the whole next day's tavern - which is exactly what happened when
        // one started at 23:5x and ran until ten in the morning with 320
        // minutes of thirst sitting there unused
        let until_reset = gs.server_time().next_midnight().as_secs();
        // How close to the daily reset is too close to start a shift
        const RESET_BLACKOUT_SEC: u64 = 90 * 60;
        // ...and right after it the tavern in the state is still
        // yesterday's until the tavern step has logged in again for the new
        // day (see `day_read`). "Thirst 0" then means nothing
        let server_now = gs.server_time().current();
        let just_after_reset = server_now.time()
            < chrono::NaiveTime::from_hms_opt(0, 30, 0).unwrap_or_default()
            || account.expedition_info.day_read != Some(server_now.date());

        enum Todo {
            Finish,
            FinishQuest,
            Start,
            Wait(String),
        }
        let todo = match &tavern.current_action {
            CurrentAction::CityGuard { busy_until, hours } => {
                if *busy_until <= now {
                    Todo::Finish
                } else {
                    Todo::Wait(format!(
                        "On duty ({hours} h) until {}",
                        busy_until.format("%H:%M")
                    ))
                }
            }
            CurrentAction::Quest { busy_until, .. } if *busy_until <= now => {
                Todo::FinishQuest
            }
            CurrentAction::Quest { busy_until, .. } => Todo::Wait(format!(
                "On a quest until {}",
                busy_until.format("%H:%M")
            )),
            CurrentAction::Expedition => {
                Todo::Wait("On an expedition".to_string())
            }
            CurrentAction::Idle if expedition_running => {
                Todo::Wait("On an expedition".to_string())
            }
            // Beer is thirst you can still buy, and the shift blocks the
            // tavern for hours. No timeout on this: a shift that starts
            // while eight beers are sitting there throws them away
            CurrentAction::Idle if tavern_auto && beer_possible => Todo::Wait(
                format!("Waiting: {} beers still buyable", beers_left),
            ),
            CurrentAction::Idle if tavern_auto && thirst > 0 => {
                Todo::Wait(format!(
                    "Waiting: {} min of thirst for adventure left in the \
                     tavern",
                    thirst / 60
                ))
            }
            CurrentAction::Idle if tavern_auto && just_after_reset => {
                Todo::Wait(
                    "Waiting: the day has just turned - the tavern is read \
                     again before any shift"
                        .to_string(),
                )
            }
            // The tavern itself has to have said "done for today" within
            // the last ten minutes - its verdict counts beers, batches and
            // the VIP box. Pressing ON used to start a ten hour shift on a
            // stale zero before the tavern had even looked
            CurrentAction::Idle
                if tavern_auto
                    && !account.expedition_info.tavern_done_at.is_some_and(
                        |t| now - t < chrono::Duration::minutes(10),
                    ) =>
            {
                Todo::Wait(
                    "Waiting for the tavern to report the day done (thirst \
                     and beer used up)"
                        .to_string(),
                )
            }
            // The tavern is empty now, but it will not be for long
            CurrentAction::Idle
                if tavern_auto && until_reset < RESET_BLACKOUT_SEC =>
            {
                Todo::Wait(format!(
                    "Waiting: the new day's thirst arrives in {} min - a \
                     shift started now would sit on it",
                    until_reset / 60
                ))
            }
            CurrentAction::Idle => Todo::Start,
            CurrentAction::Unknown(_) => {
                Todo::Wait("Busy with something unknown".to_string())
            }
        };
        let wage = tavern.guard_wage;
        drop(status);

        // Shifts end before the cutoff (so the new day's thirst is not
        // wasted asleep). Between the cutoff and midnight nothing starts
        let mut hours = hours;
        let mut todo = todo;
        if let Todo::Start = todo
            && account.guard_info.cutoff_enabled
            && let Ok(cut) = chrono::NaiveTime::parse_from_str(
                account.guard_info.cutoff.trim(),
                "%H:%M",
            )
        {
            let now_t = now.time();
            if now_t >= cut {
                todo = Todo::Wait(format!(
                    "Waiting for midnight - no shift after {}",
                    cut.format("%H:%M")
                ));
            } else {
                let left = (cut - now_t).num_minutes().max(0) / 60;
                if left < 1 {
                    todo = Todo::Wait(format!(
                        "Less than an hour until {} - waiting for midnight",
                        cut.format("%H:%M")
                    ));
                } else {
                    hours = hours.min(left as u8);
                }
            }
        }

        let gi = &mut account.guard_info;
        match todo {
            Todo::Wait(msg) => {
                gi.status = Some(msg);
                Command::none()
            }
            Todo::Finish => self.send_simple(
                ident,
                "Guard",
                sf_api::command::Command::FinishWork,
                SimpleNote::Guard(
                    "Finished the shift, wage collected".to_string(),
                ),
            ),
            // A quest that is over blocks everything until it is collected
            Todo::FinishQuest => self.send_simple(
                ident,
                "Guard",
                sf_api::command::Command::FinishQuest { skip: None },
                SimpleNote::Guard(
                    "Finished the quest that was over".to_string(),
                ),
            ),
            Todo::Start => self.send_simple(
                ident,
                "Guard",
                sf_api::command::Command::StartWork { hours },
                SimpleNote::Guard(format!(
                    "Started a {hours} h shift ({} silver/h)",
                    wage
                )),
            ),
        }
    }

    fn recycle_replaced(&mut self, ident: AccountIdent, extraction_refused: bool) -> Command<Message> {
        use crate::shopping::RecycleAction;
        let Some(account) = self.servers.get_account_mut(&ident) else { return Command::none(); };
        let Some(p) = account.shop_info.pending_recycle.clone() else { return Command::none(); };
        let arc = account.status.clone();
        let lock = arc.lock_safe();
        let AccountStatus::Idle(_, gs) = &*lock else { return Command::none(); };
        let action = crate::shopping::replaced_item_action(gs, &p,
            account.shop_info.auto && account.shop_info.sell_old, extraction_refused);
        drop(lock);
        let Some(action) = action else {
            account.shop_info.pending_recycle = None;
            account.shop_info.status = Some(crate::i18n::t("Replaced equipment kept in the backpack.").into());
            return self.shop_step(ident);
        };
        let Some(item_ident) = p.old_ident else { return Command::none(); };
        let command = match action {
            RecycleAction::ExtractGem | RecycleAction::Dismantle => sf_api::command::Command::Blacksmith {
                item_pos: p.bag_pos.into(), item_ident,
                action: if action == RecycleAction::ExtractGem {
                    sf_api::command::BlacksmithAction::GemExtract
                } else { sf_api::command::BlacksmithAction::Dismantle },
            },
            RecycleAction::Sell => sf_api::command::Command::SellShop { item_pos: p.bag_pos.into(), item_ident },
        };
        account.shop_info.steps_this_round += 1;
        self.send_simple(ident, "Shop", command, SimpleNote::Recycle { action })
    }

    /// Shopping, one request per step: buy the best upgrade into the bag,
    /// move it onto the character, sell the replaced piece. The result
    /// handler calls this again until nothing is left to do
    fn shop_step(&mut self, ident: AccountIdent) -> Command<Message> {
        use sf_api::command::Command as SFCommand;
        let key = self.char_key(&ident);
        let Some(account) = self.servers.get_account_mut(&ident) else {
            return Command::none();
        };
        if account.shop_info.steps_this_round >= 9 {
            account.shop_info.status =
                Some("Enough for this round - continues next time".to_string());
            return Command::none();
        }
        let arc = account.status.clone();
        let status = arc.lock_safe();
        let AccountStatus::Idle(_, gs) = &*status else {
            return Command::none();
        };
        let si = &account.shop_info;

        // ---- the character first: the best piece in the bag goes on
        // (5.3). Found gear used to stay in the bag for good - only what
        // the shop step had just bought was ever put on
        if si.pending.is_none()
            && si.pending_companion.is_none()
            && let Some((bag_pos, slot, gain)) =
                crate::shopping::character_bag_upgrade(
                    gs,
                    &si.protection(),
                    f64::from(si.min_gain),
                )
            && let Some(item) = gs
                .character
                .inventory
                .backpack
                .get(bag_pos.backpack_pos())
                .and_then(|i| i.as_ref())
        {
            let item_ident = item.command_ident();
            let legendary_off = gs.character.equipment.0[slot]
                .as_ref()
                .is_some_and(sf_api::gamestate::items::Item::is_legendary);
            let old_ident = gs.character.equipment.0[slot].as_ref()
                .map(sf_api::gamestate::items::Item::command_ident);
            drop(status);
            account.shop_info.pending = Some(crate::player::PendingBuy {
                bag_pos, slot, new_ident: item_ident, old_ident,
                equipped: true, started: Local::now(),
            });
            account.shop_info.steps_this_round += 1;
            let what = if legendary_off {
                format!(
                    "Put on a better {slot:?} from the bag (+{gain:.0} %) - \
                     the legendary goes into the bag, kept"
                )
            } else {
                format!("Put on a better {slot:?} from the bag (+{gain:.0} %)")
            };
            return self.send_simple(
                ident,
                "Shop",
                SFCommand::PlayerItemMove {
                    from: bag_pos.into(),
                    to: slot.into(),
                    item_ident,
                },
                SimpleNote::Shop(what),
            );
        }

        // ---- dress the companions out of the bag first
        // Their gear only ever comes from what the character finds - there
        // is no shop for them - so a piece the character does not want is
        // worth putting on one of them before anything else happens to it
        if si.equip_companions
            && si.pending.is_none()
            && let Some(mv) = crate::companions::next_move(gs, &si.protection())
        {
            let item_ident = gs
                .character
                .inventory
                .backpack
                .get(mv.bag_pos.backpack_pos())
                .and_then(|i| i.as_ref())
                .map(sf_api::gamestate::items::Item::command_ident);
            if let Some(item_ident) = item_ident {
                let had_before =
                    gs.dungeons.companions.as_ref().is_some_and(|c| {
                        c[mv.companion].equipment.0[mv.slot].is_some()
                    });
                drop(status);
                account.shop_info.steps_this_round += 1;
                account.shop_info.pending_companion =
                    Some(crate::player::PendingCompanion {
                        companion: mv.companion,
                        slot: mv.slot,
                        new_ident: item_ident,
                        had_before,
                        what: mv.what.clone(),
                    });
                return self.send_simple(
                    ident,
                    "Shop",
                    SFCommand::EquipCompanion {
                        from_pos: mv.bag_pos.into(),
                        to_slot: mv.slot,
                        item_ident,
                        to_companion: mv.companion,
                    },
                    SimpleNote::Shop(mv.what),
                );
            }
        }

        // ---- a purchase in progress
        if let Some(p) = si.pending.clone() {
            if !si.auto {
                drop(status);
                account.shop_info.pending = None;
                account.shop_info.status = Some(
                    "Buying stopped; purchased items stay in the backpack"
                        .into(),
                );
                return Command::none();
            }
            if Local::now() - p.started > chrono::Duration::minutes(10) {
                drop(status);
                account.shop_info.pending = None;
                account.shop_info.status =
                    Some("A stale purchase was dropped".to_string());
                return self.shop_step(ident);
            }
            let at_bag = gs
                .character
                .inventory
                .backpack
                .get(p.bag_pos.backpack_pos())
                .cloned()
                .flatten();
            if !p.equipped {
                // The new item should be in the bag - put it on. Only the
                // item we bought, never whatever else may sit there
                let Some(item) =
                    at_bag.filter(|i| i.command_ident() == p.new_ident)
                else {
                    drop(status);
                    account.shop_info.pending = None;
                    account.shop_info.status = Some(
                        "The bought item is not in the bag - giving up".into(),
                    );
                    return Command::none();
                };
                drop(status);
                let mut p2 = p.clone();
                p2.equipped = true;
                account.shop_info.pending = Some(p2);
                account.shop_info.steps_this_round += 1;
                return self.send_simple(
                    ident,
                    "Shop",
                    SFCommand::PlayerItemMove {
                        from: p.bag_pos.into(),
                        to: p.slot.into(),
                        item_ident: item.command_ident(),
                    },
                    SimpleNote::Shop(format!("Equipped {:?}", p.slot)),
                );
            }
            // Never extract from worn equipment: extraction destroys it.
            // The disposal helper verifies both sides of the completed swap.
            drop(status);
            account.shop_info.pending = None;
            account.shop_info.pending_recycle = Some(p);
            return self.recycle_replaced(ident, false);
        }

        // Housekeeping is allowed to run with buying OFF. It must never
        // fall through into a purchase or a paid shop refresh.
        if !si.auto {
            let clear = si.clear_backpack;
            drop(status);
            if clear && let Some(command) = self.backpack_sale(ident) {
                return command;
            }
            return Command::none();
        }

        // ---- look for the best upgrade
        let silver = gs.character.silver;
        let reserve = silver * u64::from(si.gold_reserve.min(100)) / 100;
        let budget = silver.saturating_sub(reserve);
        // Mushrooms the epics may take: none unless allowed, and never the
        // dragon's 25
        let mush_budget = if si.mushrooms {
            account.free_mushrooms(gs)
        } else {
            0
        };

        // ---- hourglasses: bought whenever the shop has one. They skip the
        // expedition waits, which for a bot that runs all day is worth more
        // than any single piece of gear - and they have no equipment slot,
        // so the gear search below never saw them at all
        if let Some((shop_pos, item)) = gs.shops.values().find_map(|shop| {
            shop.iter().find(|(_, item)| {
                matches!(
                    item.typ,
                    sf_api::gamestate::items::ItemType::QuickSandGlass
                ) && if item.mushroom_price > 0 {
                    item.mushroom_price <= mush_budget
                } else {
                    u64::from(item.price) <= budget
                }
            })
        }) && let Some(bag_pos) = gs.character.inventory.free_slot()
        {
            let what = if item.mushroom_price > 0 {
                format!(
                    "Bought an hourglass for {} mushrooms",
                    item.mushroom_price
                )
            } else {
                format!("Bought an hourglass for {} gold", item.price / 100)
            };
            let cmd = SFCommand::BuyShop {
                shop_pos,
                new_pos: bag_pos.into(),
                item_ident: item.command_ident(),
            };
            drop(status);
            account.shop_info.steps_this_round += 1;
            return self.send_simple(
                ident,
                "Shop",
                cmd,
                SimpleNote::Shop(what),
            );
        }

        let list = crate::shopping::candidates(
            gs,
            &si.protection(),
            f64::from(si.min_gain),
            budget,
            mush_budget,
            si.equip_companions,
        );
        // The thinking: what both shops hold for the four of them and
        // what wins, once per change of mind
        let on_offer: usize = gs.shops.values().map(|s| s.iter().count()).sum();
        let thought = match list.first() {
            Some(c) => format!(
                "shops - {on_offer} pieces on offer, {} worth buying within {} \
                 gold; best: {:?} for {} at +{:.0} % for {}",
                list.len(),
                human_money(budget as f64 / 100.0),
                c.slot,
                c.for_companion
                    .map_or("you", crate::companions::companion_name),
                c.gain_pct.min(999.0),
                shop_price(c)
            ),
            None => format!(
                "shops - {on_offer} pieces on offer, none at least {} % better \
                 than what you and Bert, Mark, Kunigunde wear within {} gold",
                si.min_gain,
                human_money(budget as f64 / 100.0)
            ),
        };
        if account.shop_info.last_thought.borrow().as_deref()
            != Some(thought.as_str())
        {
            let name = account.name.clone();
            account
                .shop_info
                .last_thought
                .replace(Some(thought.clone()));
            Self::push_think(&mut self.console, &name, thought);
            // ...and a word on the shop that held the find, or on the
            // empty pair
            let mood = match list.first().map(|c| c.shop_pos.typ) {
                Some(sf_api::command::ShopType::Weapon) => {
                    crate::goblin::Mood::WeaponShop
                }
                Some(sf_api::command::ShopType::Magic) => {
                    crate::goblin::Mood::MagicShop
                }
                None => crate::goblin::Mood::ShopEmpty,
            };
            Self::push_goblin(&mut self.console, &name, mood);
        }
        let Some(best) = list.into_iter().next() else {
            // Nothing within the budget. Is there something worth saving
            // for? Then the attributes wait for it (see `attr_step`) and
            // the shop is not rerolled - a reroll would throw it away
            let wants = crate::shopping::candidates(
                gs,
                &si.protection(),
                f64::from(si.min_gain),
                u64::MAX / 2,
                mush_budget,
                si.equip_companions,
            )
            .into_iter()
            .find(|c| c.mushrooms == 0)
            .map(|c| u64::from(c.price))
            // Only a piece within reach is worth saving for: three times
            // what is free above the reserve. Saving for *anything* better,
            // at any price, is why the shops were "almost never" rerolled -
            // there is nearly always some piece a little better somewhere
            .filter(|price| *price <= budget.saturating_mul(3));
            if let Some(price) = wants {
                drop(status);
                account.shop_info.gear_wants = Some(price);
                account.shop_info.status = Some(format!(
                    "Saving for a {} gold piece in the shop ({} gold free \
                     above the reserve) - attributes wait",
                    human_money(price as f64 / 100.0),
                    budget / 100
                ));
                // The slot being saved for is left alone by the
                // blacksmith; the others may still take a level
                return self.witch_step(ident);
            }
            // Nothing for any of the four. A shop full of things nobody can
            // use is worth nothing standing there, so one of the day's
            // rerolls gets spent - but only here, once both shops have been
            // looked at for everybody. That is why the allowance is a
            // ceiling and not a schedule: rerolling at fixed times would
            // throw away shops that had something in them
            let mut types = Vec::new();
            for (typ, _) in &gs.shops {
                types.push(typ);
            }
            // A reroll costs a mushroom - only out of what the mount does
            // not need
            let can_pay = account.free_mushrooms(gs) > 0;
            // The reroll allowance renews with the server's day, and what
            // was spent of it is on disk (see `spend_ledger`)
            let today = gs.server_time().current().date();
            drop(status);
            account.shop_info.gear_wants = None;
            let spent =
                key.as_deref().map(|k| self.spend.today(k, today).rerolls);
            if account.shop_info.roll_day != Some(today) {
                account.shop_info.roll_day = Some(today);
                account.shop_info.rolls_today = 0;
            }
            if let Some(spent) = spent {
                account.shop_info.rolls_today =
                    account.shop_info.rolls_today.max(spent);
            }
            if !types.is_empty() && account.shop_info.may_reroll() && can_pay {
                let idx =
                    usize::from(account.shop_info.rolls_today) % types.len();
                let shop = types[idx];
                account.shop_info.rolls_today += 1;
                if let Some(k) = key.as_deref() {
                    let on_disk = self
                        .spend
                        .bump(k, today, crate::spend_ledger::Spend::ShopReroll)
                        .rerolls;
                    account.shop_info.rolls_today =
                        account.shop_info.rolls_today.max(on_disk);
                }
                account.shop_info.steps_this_round += 1;
                let n = account.shop_info.rolls_today;
                let of = account.shop_info.rolls_per_day;
                return self.send_simple(
                    ident,
                    "Shop",
                    SFCommand::RefreshShop { shop },
                    SimpleNote::Shop(format!(
                        "Nothing for anybody - rerolled the {shop:?} shop \
                         ({n}/{of} today)"
                    )),
                );
            }
            account.shop_info.status = Some(format!(
                "Nothing in the shops for anybody within the budget ({} gold \
                 free), no reroll left today - {}",
                budget / 100,
                Local::now().format("%H:%M")
            ));
            // Nothing to buy: the witch's turn, then the blacksmith's
            return self.witch_step(ident);
        };
        let Some(bag_pos) = gs.character.inventory.free_slot() else {
            drop(status);
            account.shop_info.status =
                Some("No free slot in the bag - cannot buy".to_string());
            return Command::none();
        };
        // A piece bought for Bert, Mark or Kunigunde only has to reach the
        // bag. Putting it on is the companion step at the top of this
        // function - and it has to be, because the character's own
        // `PlayerItemMove` would put the thing on the character instead
        if let Some(who) = best.for_companion {
            drop(status);
            account.shop_info.gear_wants = None;
            account.shop_info.steps_this_round += 1;
            return self.send_simple(
                ident,
                "Shop",
                SFCommand::BuyShop {
                    shop_pos: best.shop_pos,
                    new_pos: bag_pos.into(),
                    item_ident: best.item.command_ident(),
                },
                SimpleNote::Shop(format!(
                    "Bought a {:?} for {} for {} (+{:.0} % on what they wear)",
                    best.slot,
                    crate::companions::companion_name(who),
                    shop_price(&best),
                    best.gain_pct.min(999.0)
                )),
            );
        }
        let worn = gs.character.equipment.0[best.slot].as_ref();
        let old_ident = worn.map(|i| i.command_ident());
        drop(status);
        account.shop_info.gear_wants = None;
        account.shop_info.pending = Some(crate::player::PendingBuy {
            bag_pos,
            slot: best.slot,
            new_ident: best.item.command_ident(),
            old_ident,
            equipped: false,
            started: Local::now(),
        });
        account.shop_info.steps_this_round += 1;
        self.send_simple(
            ident,
            "Shop",
            SFCommand::BuyShop {
                shop_pos: best.shop_pos,
                new_pos: bag_pos.into(),
                item_ident: best.item.command_ident(),
            },
            SimpleNote::Shop(format!(
                "Bought {:?} for {} (+{:.0} % over the worn one)",
                best.slot,
                shop_price(&best),
                best.gain_pct.min(999.0)
            )),
        )
    }
}

impl Helper {
    /// One enchantment at the witch, once the shops hold nothing to buy:
    /// a worn piece whose enchantment is unlocked, not enchanted yet, not
    /// about to be replaced, with the gold above the reserve covering the
    /// witch's price. Falls through to the blacksmith when there is none
    fn witch_step(&mut self, ident: AccountIdent) -> Command<Message> {
        use sf_api::command::Command as SFCommand;
        let Some(account) = self.servers.get_account_mut(&ident) else {
            return Command::none();
        };
        if !account.shop_info.witch_auto
            || !account.shop_info.auto
            || account.shop_info.steps_this_round >= 9
        {
            return self.smith_step(ident);
        }
        let arc = account.status.clone();
        let status = arc.lock_safe();
        let AccountStatus::Idle(_, gs) = &*status else {
            return Command::none();
        };
        let Some(witch) = gs.witch.as_ref() else {
            drop(status);
            account.shop_info.witch_status =
                Some("Witch: not unlocked yet".into());
            return self.smith_step(ident);
        };
        let silver = gs.character.silver;
        let reserve =
            silver * u64::from(account.shop_info.gold_reserve.min(100)) / 100;
        let budget = silver.saturating_sub(reserve);
        let min_gain = f64::from(account.shop_info.min_gain);
        // The first slot whose enchantment is unlocked and whose piece is
        // worth it - weapon first, the way the slots are ordered
        let mut pick = None;
        for (enchantment, ident_opt) in &witch.enchantments {
            let Some(ench_ident) = ident_opt else {
                continue;
            };
            let slot = enchantment.equipment_slot();
            let Some(item) = gs.character.equipment.0[slot].as_ref() else {
                continue;
            };
            if item.enchantment.is_some() {
                continue;
            }
            // The game has already said no to this exact piece. The
            // witch's list only says the *slot*'s enchantment is
            // unlocked; whether the thing worn in it can take one is
            // something only the game knows, and it told us once
            if account
                .shop_info
                .witch_refused
                .contains(&item.full_model_id)
            {
                continue;
            }
            if crate::smith::better_available(gs, None, slot, item, min_gain) {
                continue;
            }
            pick = Some((slot, *ench_ident, item.full_model_id));
            break;
        }
        let price = witch.enchantment_price;
        let Some((slot, ench_ident, model)) = pick else {
            drop(status);
            let refused = account.shop_info.witch_refused.len();
            account.shop_info.witch_status = Some(if refused > 0 {
                format!(
                    "Witch: every worn piece is enchanted, or its enchantment \
                     is not unlocked yet, or a better piece is in sight \
                     ({refused} the game will not enchant)"
                )
            } else {
                "Witch: every worn piece is enchanted, or its enchantment is \
                 not unlocked yet, or a better piece is in sight"
                    .to_string()
            });
            return self.smith_step(ident);
        };
        if price > budget {
            drop(status);
            account.shop_info.witch_status = Some(format!(
                "Witch: the {slot:?} waits for {} gold above the reserve ({} free)",
                human_money(price as f64 / 100.0),
                human_money(budget as f64 / 100.0)
            ));
            return self.smith_step(ident);
        }
        drop(status);
        account.shop_info.steps_this_round += 1;
        let what = format!(
            "Enchanted the {slot:?} at the witch ({} gold)",
            human_money(price as f64 / 100.0)
        );
        account.shop_info.witch_status = Some(what.clone());
        self.send_simple(
            ident,
            "Shop",
            SFCommand::WitchEnchant {
                enchantment: ench_ident,
            },
            SimpleNote::WitchEnchant { what, model },
        )
    }

    /// One blacksmith level, once the shops hold nothing to buy - see
    /// `crate::smith` for what gets raised and what is left alone. Part of
    /// the shop step: it runs where a purchase would, after every
    /// purchase, and shares its round guard, so a round never ends in an
    /// endless run of levels
    fn smith_step(&mut self, ident: AccountIdent) -> Command<Message> {
        use sf_api::command::{BlacksmithAction, Command as SFCommand};
        // How fast the character levels, from the guild diary: a character
        // gaining three or more levels a week outgrows its gear every few
        // days, so only pieces at or above its average item level are
        // worth a level; a slow one may raise a piece a little under it
        let levels_7d = self
            .servers
            .get_ident(&ident)
            .and_then(|(server, account)| {
                let lock = account.status.lock_safe();
                match &*lock {
                    AccountStatus::Idle(_, gs) | AccountStatus::Busy(gs, _) => {
                        gs.guild.as_ref().map(|g| {
                            (
                                crate::guild_history::GuildHistory::key(
                                    &server.ident.url, g,
                                ),
                                account.name.clone(),
                            )
                        })
                    }
                    _ => None,
                }
            })
            .and_then(|(key, name)| {
                self.guild_history
                    .progress(&key, &name, Local::now())
                    .levels_7d
            });
        let fast = levels_7d.is_some_and(|l| l >= 3);
        let tolerance = if fast {
            0.0
        } else {
            crate::smith::BELOW_AVERAGE_TOLERANCE
        };
        let Some(account) = self.servers.get_account_mut(&ident) else {
            return Command::none();
        };
        // The same reading recommends the shop's "% better" step
        account.shop_info.recommended_gain =
            crate::shopping::recommended_min_gain(levels_7d);
        if !account.shop_info.smith_auto || !account.shop_info.auto {
            return Command::none();
        }
        if account.shop_info.steps_this_round >= 9 {
            return Command::none();
        }
        let arc = account.status.clone();
        let status = arc.lock_safe();
        let AccountStatus::Idle(_, gs) = &*status else {
            return Command::none();
        };
        let min_gain = f64::from(account.shop_info.min_gain);
        let (plan, weighed) =
            crate::smith::next_level_explained(gs, min_gain, tolerance);
        // The thinking, once per change of mind: the average item level,
        // the floor under which a piece is not worth a level, the levels
        // this week, and what each piece was set aside for
        let avg = crate::smith::average_item_level(&gs.character.equipment);
        let thought = format!(
            "blacksmith - your average item level {}, {} this week, so pieces \
             under item level {} are not raised (they are the next to go). \
             {} pieces weighed: {} under that floor, {} with something better \
             in a shop or the bag, {} too dear for the stock, {} over the 40 % \
             share. {}",
            avg.map_or("?".to_string(), |a| format!("{a:.0}")),
            match levels_7d {
                Some(l) => format!("+{l} levels"),
                None => "no level count yet".to_string(),
            },
            avg.map_or("?".to_string(), |a| format!(
                "{:.0}",
                a * (1.0 - tolerance)
            )),
            weighed.considered,
            weighed.below_floor,
            weighed.better_in_sight,
            weighed.no_materials,
            weighed.over_share,
            match &plan {
                Some(p) => format!(
                    "Best value per material: {}",
                    p.what.trim_start_matches("Raised the ")
                ),
                None => "Nothing worth a level right now".to_string(),
            }
        );
        if account.shop_info.last_thought.borrow().as_deref()
            != Some(thought.as_str())
        {
            let name = account.name.clone();
            account
                .shop_info
                .last_thought
                .replace(Some(thought.clone()));
            Self::push_think(&mut self.console, &name, thought);
        }
        let Some(plan) = plan else {
            let note = match &gs.blacksmith {
                None => "Blacksmith: not unlocked yet".to_string(),
                Some(b) => format!(
                    "Blacksmith: nothing to raise right now ({} metal, {} \
                     arcane) - every piece is at +{}, under the item level \
                     floor, waits for a better one from the shop, or costs \
                     more than the stock allows",
                    b.metal,
                    b.arcane,
                    crate::smith::MAX_LEVEL
                ),
            };
            drop(status);
            account.shop_info.smith_status = Some(note);
            return Command::none();
        };
        let metal_before = gs.blacksmith.as_ref().map_or(0, |b| b.metal);
        let cmd = match plan.wearer {
            None => SFCommand::Blacksmith {
                item_pos: plan.slot.into(),
                action: BlacksmithAction::Upgrade,
                item_ident: plan.item_ident,
            },
            // The library has no place for a companion's equipment, so the
            // request is spelled out the way `EquipCompanion` spells its
            // target: place 101 + companion, then the slot, then the
            // blacksmith's action
            Some(c) => SFCommand::Custom {
                cmd_name: "PlayerItemMove".to_string(),
                arguments: vec![
                    (c as u8 + 101).to_string(),
                    (plan.slot as usize).to_string(),
                    (BlacksmithAction::Upgrade as usize).to_string(),
                    "-1".to_string(),
                    plan.item_ident.to_string(),
                ],
            },
        };
        drop(status);
        account.shop_info.steps_this_round += 1;
        account.shop_info.smith_status = Some(plan.what.clone());
        account.shop_info.pending_smith = Some(crate::player::PendingSmith {
            wearer: plan.wearer,
            slot: plan.slot,
            from: plan.from,
            metal_before,
            what: plan.what.clone(),
        });
        self.send_simple(ident, "Shop", cmd, SimpleNote::Shop(plan.what))
    }
}

/// "10 mushrooms" or "12.3 K gold" - whichever the piece is priced in
fn shop_price(c: &crate::shopping::Candidate) -> String {
    if c.mushrooms > 0 {
        format!("{} mushrooms", c.mushrooms)
    } else {
        format!("{} gold", human_money(f64::from(c.price) / 100.0))
    }
}

impl Helper {
    /// Buys one attribute if the gold above the reserve covers it. The price
    /// is learned from the last purchase; before the first one a
    /// conservative guess is used
    fn attr_step(&mut self, ident: AccountIdent) -> Command<Message> {
        use sf_api::command::Command as SFCommand;
        let Some(account) = self.servers.get_account_mut(&ident) else {
            return Command::none();
        };
        if account.shop_info.attr_bought_this_round >= 8 {
            account.shop_info.attr_status =
                Some("Enough for this round - continues next time".to_string());
            return Command::none();
        }
        let arc = account.status.clone();
        let status = arc.lock_safe();
        let AccountStatus::Idle(_, gs) = &*status else {
            return Command::none();
        };
        let si = &account.shop_info;
        let silver = gs.character.silver;
        let reserve = silver * u64::from(si.gold_reserve.min(100)) / 100;
        let budget = silver.saturating_sub(reserve);
        // Gear first: a piece the shop step is saving for beats a single
        // attribute point, so the gold is left to grow until it is bought
        if si.auto
            && let Some(price) = si.gear_wants
            && price > budget
        {
            drop(status);
            account.shop_info.attr_status = Some(format!(
                "Gear first: saving for a {} gold piece in the shop ({} \
                 gold free above the reserve)",
                human_money(price as f64 / 100.0),
                budget / 100
            ));
            return Command::none();
        }
        let guess = si
            .attr_last_cost
            .map(|c| c + c / 20)
            .unwrap_or(u64::from(gs.character.level) * 25 * 100);
        let Some(attr) = crate::shopping::next_attribute(
            gs,
            f64::from(si.attr_w_main),
            f64::from(si.attr_w_con),
            f64::from(si.attr_w_luck),
        ) else {
            drop(status);
            account.shop_info.attr_status =
                Some("All weights are 0 - nothing to buy".to_string());
            return Command::none();
        };
        if guess > budget {
            drop(status);
            account.shop_info.attr_status = Some(format!(
                "Waiting for gold: next {attr:?} ~{} gold, {} gold free above the reserve",
                guess / 100,
                budget / 100
            ));
            return Command::none();
        }
        let increase_to = crate::shopping::attribute_next_value(gs, attr);
        drop(status);
        account.shop_info.attr_bought_this_round += 1;
        self.send_simple(
            ident,
            "Attributes",
            SFCommand::IncreaseAttribute {
                attribute: attr,
                increase_to,
            },
            SimpleNote::Attr(format!("{attr:?} -> {increase_to}"), silver),
        )
    }

    /// One spin of the wheel, if the day's quota is not reached: the free
    /// spin first, then lucky coins, mushrooms only when allowed
    fn wheel_step(&mut self, ident: AccountIdent) -> Command<Message> {
        use sf_api::command::{Command as SFCommand, FortunePayment};
        let Some(account) = self.servers.get_account_mut(&ident) else {
            return Command::none();
        };
        let ti = &account.tavern_info;
        if ti.spins_this_round >= 40 {
            return Command::none();
        }
        let arc = account.status.clone();
        let status = arc.lock_safe();
        let AccountStatus::Idle(_, gs) = &*status else {
            return Command::none();
        };
        let wheel = &gs.specials.wheel;
        let now = Local::now();
        if wheel.spins_today
            >= ti.wheel_spins.min(crate::mechanics::wheel_spin_limit(gs))
        {
            drop(status);
            account.tavern_info.wheel_status =
                Some(format!("Done for today ({} spins)", now.format("%H:%M")));
            return Command::none();
        }
        let free = wheel.next_free_spin.is_none_or(|t| t <= now);
        let payment = if free {
            Some((FortunePayment::FreeTurn, "Free spin"))
        } else if wheel.lucky_coins >= crate::mechanics::WHEEL_COIN_PRICE {
            Some((FortunePayment::LuckyCoins, "Spin for lucky coins"))
        } else {
            None
        };
        let next_free = wheel.next_free_spin;
        drop(status);
        let Some((payment, what)) = payment else {
            account.tavern_info.wheel_status = Some(format!(
                "Waiting: need 10 lucky coins, next free spin {}",
                next_free
                    .map(|t| t.format("%d.%m. %H:%M").to_string())
                    .unwrap_or_else(|| "?".to_string())
            ));
            return Command::none();
        };
        account.tavern_info.spins_this_round += 1;
        self.send_simple(
            ident,
            "Wheel",
            SFCommand::SpinWheelOfFortune { payment },
            SimpleNote::Wheel(what.to_string()),
        )
    }

    /// One step of the dice game with the cup player. The game is three
    /// commands: throw all five, keep the symbol that came up most often and
    /// throw the rest again, then take what is on the table. Only free games
    /// are played - the rest costs mushrooms
    fn dice_step(&mut self, ident: AccountIdent) -> Command<Message> {
        use sf_api::command::{Command as SFCommand, DiceType, RollDicePrice};
        let Some(account) = self.servers.get_account_mut(&ident) else {
            return Command::none();
        };
        if !account.tavern_info.dice_auto {
            return Command::none();
        }
        // Three commands per game, and a handful of games a day
        if account.tavern_info.dice_this_round >= 60 {
            return Command::none();
        }
        let arc = account.status.clone();
        let status = arc.lock_safe();
        let AccountStatus::Idle(_, gs) = &*status else {
            return Command::none();
        };
        let game = &gs.tavern.dice_game;
        let now = Local::now();
        let remaining = game.remaining;
        let next_free = game.next_free;
        let on_table: Vec<DiceType> = game.current_dice.clone();
        let reward = game.reward.map(|r| (r.win_typ, r.amount));
        let glasses = gs.tavern.quicksand_glasses;
        drop(status);

        let stage = account.tavern_info.dice_stage;
        // The second throw decides the game - the winnings are paid out with
        // it and land in the state. A third "take what is on the table"
        // request was answered with "cannot roll for free yet" every time,
        // because for the server that was a new game. So the last stage
        // sends nothing: it reads the winnings and goes on to the next game
        if stage >= 2 {
            let ti = &mut account.tavern_info;
            ti.dice_stage = 0;
            // Silver is what the server counts in; the player sees gold.
            // The other prizes are plain counts
            let what = match reward {
                Some((DiceType::Silver, amount)) => format!(
                    "Won {} gold",
                    crate::ui::thousands(u64::from(amount) / 100)
                ),
                Some((typ, amount)) => format!(
                    "Won {}x {typ:?}",
                    crate::ui::thousands(u64::from(amount))
                ),
                None => "Game over - nothing won".to_string(),
            };
            ti.dice_status = Some(what.clone());
            ti.dice_log.push((now, what.clone(), reward.is_some()));
            let name = account.name.clone();
            if reward.is_some() {
                Self::push_success(
                    &mut self.successes,
                    &name,
                    format!("Dice: {what}"),
                );
            }
            self.console(
                Some(ident),
                format!("Dice: {what}"),
                Some(reward.is_some()),
            );
            self.goblin(
                Some(ident),
                if reward.is_some() {
                    crate::goblin::Mood::DiceWon
                } else {
                    crate::goblin::Mood::DiceLost
                },
            );
            return Command::perform(
                async {
                    crate::pace::sleep_between(1500, 3000).await;
                },
                move |()| Message::DiceTick { ident },
            );
        }
        // A finished game leaves the table set. Only the start of a new game
        // depends on the day's quota and the free-game timer
        let mut payment = RollDicePrice::Free;
        if stage == 0 {
            if remaining == 0 {
                account.tavern_info.dice_status = Some(format!(
                    "No games left today ({})",
                    now.format("%H:%M")
                ));
                return Command::none();
            }
            let force = account.tavern_info.dice_force_glass;
            if next_free.is_some_and(|t| t > now) || force {
                // A quicksand glass buys the wait away. The day's games are
                // capped anyway, so this only decides whether they are all
                // played today or trickle in over the next few hours
                if account.tavern_info.dice_skip && glasses > 0 {
                    payment = RollDicePrice::Hourglass;
                    account.tavern_info.dice_force_glass = false;
                } else {
                    account.tavern_info.dice_status = Some(format!(
                        "Waiting: next free game {}",
                        next_free
                            .map(|t| t.format("%d.%m. %H:%M").to_string())
                            .unwrap_or_else(|| "?".to_string())
                    ));
                    return Command::none();
                }
            }
        }

        // A new game starts with the last game's prize wiped from the
        // state: the server only sends a prize when one is won, so a lost
        // game left the previous one standing and it was reported won
        // again - "Won 452272x Silver" four times in three seconds
        if stage == 0 {
            let mut lock = arc.lock_safe();
            if let AccountStatus::Idle(_, gs) | AccountStatus::Busy(gs, _) =
                &mut *lock
            {
                gs.tavern.dice_game.reward = None;
            }
        }

        let (dices, what, next_stage) = match stage {
            // Nothing on the table yet - throw all five
            0 => (
                [DiceType::ReRoll; 5],
                if payment == RollDicePrice::Hourglass {
                    format!(
                        "Rolling the dice, wait skipped with a quicksand \
                         glass ({remaining} games left)"
                    )
                } else {
                    format!("Rolling the dice ({remaining} games left)")
                },
                1,
            ),
            // The five are lying there. Keep whatever came up most often and
            // throw the others again - that is the one re-roll we get
            1 => {
                let Some(keep) = most_common_dice(&on_table) else {
                    account.tavern_info.dice_stage = 0;
                    account.tavern_info.dice_status =
                        Some("The table is empty - starting again".into());
                    return Command::none();
                };
                let mut dices = [DiceType::ReRoll; 5];
                let mut kept = 0;
                for (slot, die) in dices.iter_mut().zip(on_table.iter()) {
                    if *die == keep {
                        *slot = *die;
                        kept += 1;
                    }
                }
                (
                    dices,
                    format!("Keeping {kept}x {keep:?}, re-rolling the rest"),
                    2,
                )
            }
            // Handled above - the second throw is the last request
            _ => return Command::none(),
        };

        account.tavern_info.dice_this_round += 1;
        self.send_simple(
            ident,
            "Dice",
            SFCommand::RollDice { payment, dices },
            SimpleNote::Dice {
                what,
                stage: next_stage,
            },
        )
    }

    /// Collects everything that is free and waiting: the daily login reward
    /// from the calendar, and the goblin gleeman's task chests as soon as
    /// enough task points are earned. One request per call, the result
    /// handler comes back for the next one
    fn dailies_step(&mut self, ident: AccountIdent) -> Command<Message> {
        use sf_api::command::Command as SFCommand;
        let Some(account) = self.servers.get_account_mut(&ident) else {
            return Command::none();
        };
        if !account.dailies_info.auto {
            return Command::none();
        }
        // Seven chests plus the calendar is the most there can ever be
        if account.dailies_info.steps_this_round >= 8 {
            return Command::none();
        }
        let arc = account.status.clone();
        let status = arc.lock_safe();
        let AccountStatus::Idle(_, gs) = &*status else {
            return Command::none();
        };
        let now = Local::now();
        let specials = &gs.specials;

        // 0) Keep a mount rented. A lapsed mount is a big, silent loss on
        //    every expedition, quest and guard shift. Renewed the day
        //    before it runs out, bought outright when there is none
        let mut action: Option<(SFCommand, String)> = None;
        // The ladder itself (dragon > tiger > horse > cow, best affordable,
        // never a downgrade onto a running mount) is `mechanics::mount_to_buy`
        if account.dailies_info.mount_auto {
            let ending_soon = gs
                .character
                .mount_end
                .is_some_and(|end| end <= now + chrono::Duration::days(1));
            if let Some(mount) = crate::mechanics::mount_to_buy(
                gs.character.mount, ending_soon, gs.character.silver,
                gs.character.mushrooms,
            ) {
                let cost = mount.cost();
                let price = if cost.mushrooms > 0 {
                    format!("{} mushrooms", cost.mushrooms)
                } else {
                    format!("{} gold", cost.silver / 100)
                };
                action = Some((
                    SFCommand::BuyMount { mount },
                    format!("Rented/renewed the {mount:?} ({price})"),
                ));
            }
        }

        // 0b) vouchers waiting in the inbox - a coupon, a delivery, a
        //     Twitch drop. The game nags "collect your voucher before it
        //     expires" on every login; one request takes it. Each is tried
        //     once, so a refusal (already claimed elsewhere, expired) does
        //     not come back every round
        if action.is_none() {
            let tried = &account.dailies_info.claim_tried;
            let open = gs.mail.claimables.iter().find(|c| {
                c.status != sf_api::gamestate::social::ClaimableStatus::Claimed
                    && c.claimable_until.is_none_or(|t| t > now)
                    && !tried.contains(&c.msg_id)
            });
            if let Some(c) = open {
                action = Some((
                    SFCommand::ClaimableClaim { msg_id: c.msg_id },
                    format!("Claimed the voucher from the inbox: {}", c.name),
                ));
            }
        }

        // 1) the login calendar - one door a day
        let calendar = &specials.calendar;
        let door_open = calendar.next_possible.is_none_or(|t| t <= now)
            && calendar.collected < calendar.rewards.len();
        if action.is_none() && door_open {
            let what = calendar
                .rewards
                .get(calendar.collected)
                .map(|r| {
                    format!(
                        "Collected the daily login reward: {} {}",
                        r.amount,
                        crate::expedition::calendar_reward_name(r.typ)
                    )
                })
                .unwrap_or_else(|| {
                    "Collected the daily login reward".to_string()
                });
            action = Some((SFCommand::CollectCalendar, what));
        }

        // 2) the gleeman's chests. A chest opens once the tasks have earned
        //    the points it asks for
        if action.is_none() {
            let daily = &specials.tasks.daily;
            let points = daily.earned_points();
            for (pos, chest) in daily.rewards.iter().enumerate() {
                // A chest that asks for 0 points is a phantom slot the game
                // sends when there is nothing real to collect - trying to open
                // it just earns an "invalid chest" refusal, so skip it
                if !chest.opened
                    && chest.required_points > 0
                    && points >= chest.required_points
                {
                    action = Some((
                        SFCommand::CollectDailyQuestReward { pos },
                        format!(
                            "Opened daily task chest {} ({} points)",
                            pos + 1,
                            chest.required_points
                        ),
                    ));
                    break;
                }
            }
        }
        if action.is_none() {
            let event = &specials.tasks.event;
            let running = event.end.is_none_or(|t| t > now);
            if running {
                let points = event.earned_points();
                for (pos, chest) in event.rewards.iter().enumerate() {
                    // Skip the phantom 0-point chest (see the daily loop
                    // above) - opening it only draws an "invalid chest" error
                    if !chest.opened
                        && chest.required_points > 0
                        && points >= chest.required_points
                    {
                        action = Some((
                            SFCommand::CollectEventTaskReward { pos },
                            format!(
                                "Opened event task chest {} ({} points)",
                                pos + 1,
                                chest.required_points
                            ),
                        ));
                        break;
                    }
                }
            }
        }

        // Nothing to collect - say when the next door opens, so the queue
        // line is not just "waiting"
        let next_door = calendar.next_possible;
        drop(status);
        let Some((cmd, what)) = action else {
            account.dailies_info.status = Some(match next_door {
                Some(t) if t > now => format!(
                    "Nothing to collect - next calendar door {}",
                    t.format("%d.%m. %H:%M")
                ),
                _ => "Nothing to collect right now".to_string(),
            });
            account.dailies_info.steps_this_round = 0;
            return Command::none();
        };
        account.dailies_info.steps_this_round += 1;
        if let SFCommand::ClaimableClaim { msg_id } = &cmd {
            account.dailies_info.claim_tried.push(*msg_id);
        }
        self.send_simple(ident, "Dailies", cmd, SimpleNote::Dailies(what))
    }

    /// FULL BOT: every farming automation of the character on or off, in
    /// the runtime state and in the config. Nothing is sent right away - the
    /// jobs go through the queue, one after another
    fn set_full_bot(
        &mut self,
        ident: AccountIdent,
        on: bool,
    ) -> Command<Message> {
        let Some(account) = self.servers.get_account_mut(&ident) else {
            return Command::none();
        };
        let name = account.name.clone();
        let now = Local::now();
        if let Some(si) = &mut account.scrapbook_info {
            si.auto_battle = on;
            si.losses_in_a_row = 0;
            si.auto_status = None;
        }
        if let Some(ui) = &mut account.underworld_info {
            ui.auto_lure = on;
            ui.auto_collect = on;
            ui.auto_build = on;
            ui.maint_steps = 0;
        }
        if let Some(fi) = &mut account.fortress_info {
            fi.auto_attack = on;
            fi.auto_since = on.then_some(now);
            fi.auto_collect = on;
            fi.auto_gems = on;
            fi.auto_build = on;
            fi.recruit_all = on;
        }
        account.dungeon_info.auto = on;
        account.expedition_info.auto = on;
        account.pet_info.auto_feed = on;
        account.pet_info.auto_arena = on;
        account.idle_info.auto_upgrade = on;
        account.idle_info.auto_sacrifice = on;
        account.guard_info.auto = on;
        // Beer is part of the day's thirst now, so ON drinks it too; the
        // guard flag above only arms the job - `guard_step` waits for the
        // tavern's "done for today" before any shift starts
        account.tavern_info.beer_auto = on;
        account.tavern_info.wheel_auto = on;
        account.tavern_info.dice_auto = on;
        // Gear shopping goes with ON too now ("the program should work out
        // and change the gear of the character and the companions") - the
        // protections stay as set, epics, gems, enchanted and upgraded
        // pieces are never replaced unless the player unticks that
        account.shop_info.auto = on;
        account.shop_info.attr_auto = on;
        account.social.auto_attack = on;
        account.social.auto_defense = on;
        account.social.confirm_off = None;
        if on {
            for job in Job::FULL_BOT_START {
                if !account.queue.iter().any(|(j, _)| *j == job) {
                    account.queue.push((job, now));
                }
            }
        } else {
            account.queue.clear();
        }
        if let Some(conf) =
            self.config.get_char_conf_mut(&name, ident.server_id)
        {
            conf.auto_battle = on;
            conf.auto_lure = on;
            conf.underworld_collect = on;
            conf.underworld_build = on;
            conf.auto_fortress = on;
            conf.fortress_collect = on;
            conf.fortress_gems = on;
            conf.fortress_build = on;
            conf.fortress_recruit_all = on;
            conf.dungeon_auto = on;
            conf.expedition_auto = on;
            conf.pets_feed = on;
            conf.pets_arena = on;
            conf.idle_upgrade = on;
            conf.idle_sacrifice = on;
            conf.guard_auto = on;
            conf.beer_auto = on;
            conf.wheel_auto = on;
            conf.dice_auto = on;
            conf.shop_auto = on;
            conf.attr_auto = on;
            conf.guild_attack = on;
            conf.guild_defense = on;
            _ = self.config.write();
        }
        Command::none()
    }

    /// Appends a console line. `who` is resolved to the character name
    pub fn console(
        &mut self,
        who: Option<AccountIdent>,
        text: String,
        ok: Option<bool>,
    ) {
        let name = who
            .and_then(|i| {
                self.servers.get_account_mut(&i).map(|a| a.name.clone())
            })
            .map(|n| titlecase::titlecase(&n))
            .unwrap_or_default();
        let dup = !name.is_empty()
            && crate::ConsoleLine::just_succeeded(&self.successes, &name);
        self.console.push_back(crate::ConsoleLine {
            at: Local::now(),
            who: name,
            text,
            ok,
            dup,
        });
        while self.console.len() > 3000 {
            self.console.pop_front();
        }
    }

    /// Appends one line of good news to the successes window: something was
    /// gained, won, bought or upgraded. Never called for refreshes, refusals
    /// or "nothing to do" - that is what keeps the window worth reading
    pub fn success(&mut self, who: Option<AccountIdent>, text: String) {
        let name = who
            .and_then(|i| {
                self.servers.get_account_mut(&i).map(|a| a.name.clone())
            })
            .map(|n| titlecase::titlecase(&n))
            .unwrap_or_default();
        self.successes.push_back(crate::ConsoleLine {
            at: Local::now(),
            who: name,
            text,
            ok: Some(true),
            dup: false,
        });
        while self.successes.len() > 1000 {
            self.successes.pop_front();
        }
    }

    /// Same as `success`, for the spots where an account is already borrowed
    /// out of `self.servers` and a `&mut self` call would not compile. Only
    /// touches `self.successes`, so it sits fine next to a live account borrow
    /// The goblin's word on what just happened - see `crate::goblin`. In
    /// the feed's green, the way the guild leader wants to watch him, and
    /// never folded into a success line as a duplicate
    fn goblin(&mut self, who: Option<AccountIdent>, mood: crate::goblin::Mood) {
        let name = who
            .and_then(|i| {
                self.servers.get_account_mut(&i).map(|a| a.name.clone())
            })
            .unwrap_or_default();
        Self::push_goblin(&mut self.console, &name, mood);
    }

    fn push_goblin(
        console: &mut std::collections::VecDeque<crate::ConsoleLine>,
        who: &str,
        mood: crate::goblin::Mood,
    ) {
        // One line at a time across the whole feed
        if !crate::goblin::may_speak() {
            return;
        }
        console.push_back(crate::ConsoleLine {
            at: Local::now(),
            who: titlecase::titlecase(who),
            text: crate::goblin::quip(mood).to_string(),
            ok: Some(true),
            dup: false,
        });
        while console.len() > 3000 {
            console.pop_front();
        }
    }

    /// A line of the bot's thinking for the live feed - what it weighed
    /// and what it decided, in words. Neither a success nor a refusal, so
    /// the feed paints it in its own colour. Written straight into the
    /// console so it can be called while an account is borrowed
    fn push_think(
        console: &mut std::collections::VecDeque<crate::ConsoleLine>,
        who: &str,
        text: String,
    ) {
        let who = titlecase::titlecase(who);
        let text = format!("{} {text}", crate::i18n::t("Thinking:"));
        // The same thought twice in ten minutes says nothing new - it only
        // pushes everything else out of the feed
        let now = Local::now();
        let repeat = console.iter().rev().take(50).any(|l| {
            l.ok.is_none()
                && l.who == who
                && l.text == text
                && now - l.at < chrono::Duration::minutes(10)
        });
        if repeat {
            return;
        }
        console.push_back(crate::ConsoleLine {
            at: now,
            who,
            text,
            ok: None,
            dup: false,
        });
        while console.len() > 3000 {
            console.pop_front();
        }
    }

    fn push_success(
        successes: &mut std::collections::VecDeque<crate::ConsoleLine>,
        who: &str,
        text: String,
    ) {
        successes.push_back(crate::ConsoleLine {
            at: Local::now(),
            who: titlecase::titlecase(who),
            text,
            ok: Some(true),
            dup: false,
        });
        while successes.len() > 1000 {
            successes.pop_front();
        }
    }

    /// Is this note a piece of good news worth the successes window? Only
    /// the notes whose success IS a gain: a collect, a purchase, an upgrade,
    /// a reward. Fights are handled where their result is known, and the
    /// housekeeping notes (refreshes, opens, look-ups, mail) never qualify
    fn is_success_note(note: &SimpleNote) -> bool {
        match note {
            // The tavern makes thirty-odd requests per expedition - every
            // crossroads, every skipped wait, every poll. None of that is a
            // gain. What is: the reward at the end, and the moment the 40
            // heroism (+35 % XP) is locked in
            SimpleNote::Expedition(what) => {
                what.starts_with("Boss reward")
                    || what.starts_with("Collected the reward")
                    || what.contains("40 heroism reached")
            }
            // A companion equip and a blacksmith level write their own line
            // once the slot is checked
            SimpleNote::Shop(what)
                if what.contains(" gets a ")
                    || what.starts_with("Raised the ") =>
            {
                false
            }
            // Rewards and gains
            SimpleNote::Dailies(_)
            | SimpleNote::Wheel(_)
            | SimpleNote::Attr(..)
            | SimpleNote::Shop(_)
            | SimpleNote::BackpackCleared(_)
            | SimpleNote::BackpackDismantled(_)
            | SimpleNote::PetFed { .. }
            | SimpleNote::IdleCollected(_) => true,
            // The Arena Manager buys in long runs of x1 - only the big
            // bundles are worth a line here; the rest is in the activity log
            SimpleNote::IdleBought(what) => {
                what.contains(" x25 ") || what.contains(" x100 ")
            }
            // The winning end of the dice writes its own line (see
            // `dice_step`), never the rolls
            SimpleNote::Dice { .. } => false,
            // The underworld collect writes its own line, with the amount
            SimpleNote::UnderworldMaint(what)
                if what.starts_with("Collected") =>
            {
                false
            }
            // Collects and upgrades - not "nothing to do" bookkeeping
            SimpleNote::UnderworldMaint(what)
            | SimpleNote::FortressMaint(what) => {
                what.starts_with("Collected")
                    || what.starts_with("Upgrad")
                    || what.starts_with("Finished")
                    || what.starts_with("Built")
                    || what.contains("gem")
                    || what.contains("Gem")
            }
            _ => false,
        }
    }

    /// Starts at most one queued job per account - and only when the account
    /// is idle and enough time passed since the last one. Runs on the UI
    /// tick, so nothing needs its own timer
    /// How much there is to do across every character, 0-100 (see
    /// `stealth::need_score`): what decides whether the next session is a
    /// quick return or a long stretch away, whether one is skipped, and
    /// whether an open one stays a little longer or closes early
    pub fn need_now(&self) -> u8 {
        use sf_api::gamestate::tavern::CurrentAction;
        let now = Local::now();
        let soon = chrono::Duration::hours(3);
        let mut need = 0u8;
        for server in self.servers.0.values() {
            for a in server.accounts.values() {
                let lock = a.status.lock_safe();
                let gs = match &*lock {
                    AccountStatus::Idle(_, gs) | AccountStatus::Busy(gs, _) => {
                        gs
                    }
                    _ => continue,
                };
                let free = matches!(
                    gs.tavern.current_action,
                    CurrentAction::Idle | CurrentAction::Expedition
                );
                let beer_cap = a.tavern_info.beer_max.min(gs.tavern.beer_max);
                let battle_soon = gs.guild.as_ref().is_some_and(|g| {
                    [g.attacking.as_ref(), g.defending.as_ref()]
                        .into_iter()
                        .flatten()
                        .any(|b| b.date > now && b.date - now < soon)
                }) && (a.social.auto_attack
                    || a.social.auto_defense);
                let facts = crate::stealth::NeedFacts {
                    thirst: a.expedition_info.auto
                        && free
                        && gs.tavern.thirst_for_adventure_sec > 0,
                    beer_left: a.expedition_info.auto
                        && a.tavern_info.beer_auto
                        && free
                        && gs.tavern.beer_drunk < beer_cap,
                    expedition_running: gs
                        .tavern
                        .expeditions
                        .active()
                        .is_some(),
                    arena_free: a
                        .scrapbook_info
                        .as_ref()
                        .is_some_and(|s| s.auto_battle)
                        && gs.arena.next_free_fight.is_none_or(|t| t <= now),
                    dungeon_free: a.dungeon_info.auto
                        && gs.dungeons.next_free_fight.is_none_or(|t| t <= now),
                    battle_soon,
                    queued: !a.queue.is_empty(),
                };
                need = need.max(crate::stealth::need_score(&facts));
            }
        }
        need
    }

    /// Burst mode: opens a session now and schedules the next one. Or does
    /// not open it - some sessions are skipped, the way a person does not
    /// come home some evenings - unless there is too much to do (see
    /// `stealth`)
    fn open_burst(&mut self) {
        let now = Local::now();
        // A session opened a moment ago is not opened again - two callers
        // (the sweep and the queue, or a click on "now") within a second
        // gave two "Session opened" lines and threw the first draw away
        if self
            .session_started
            .is_some_and(|t| now - t < chrono::Duration::seconds(60))
            && self.burst_until.is_some_and(|u| now < u)
        {
            return;
        }
        let need = self.need_now();
        // The gap to the next one: mostly a long stretch away, a quick
        // return when there is plenty to do - never on a clock
        let profile = self.profile();
        let gap = chrono::Duration::seconds(crate::stealth::session_gap_secs(
            self.config.gap_minutes(),
            need,
            now.hour(),
        ));
        self.burst_next = Some(now + gap);
        // A session is no longer skipped on purpose. Fifteen percent of
        // them simply never happened on top of gaps that are already
        // drawn from half to two and a half times the average - two knobs
        // doing the same job, one of them invisible, and the day's income
        // paying for both
        self.session_started = Some(now);
        self.idle_since = None;
        // How long this one lasts: three minutes to one or two hours, short
        // visits the common case, never alike the one before
        let secs = crate::stealth::session_length_secs(
            self.last_session_len,
            now.hour(),
            &profile,
        );
        self.last_session_len = Some(secs);
        let minutes = secs / 60;
        self.burst_until = Some(now + chrono::Duration::seconds(secs));
        // Every character gets its own delay into the session, so five of
        // them never log in within the same minute - all of them inside
        // the session's first quarter, so a short visit still gets its
        // work done
        let mut last_in = 0i64;
        for server in self.servers.0.values_mut() {
            for account in server.accounts.values_mut() {
                let delay = crate::stealth::stagger_secs(secs);
                last_in = last_in.max(delay);
                account.not_before =
                    Some(now + chrono::Duration::seconds(delay));
            }
        }
        // The sweep fills the queues the moment the session opens - its
        // ten-minute hold is for the middle of a session, not the start.
        // A session opened a few minutes after the last one closed used
        // to sit with empty queues until the checkers woke up
        self.last_sweep = None;
        self.console(
            None,
            format!(
                "Session opened for {minutes} min ({need} % to do) - \
                 characters come in one by one over the next {} min",
                (last_in / 60).max(1)
            ),
            None,
        );
        // "Thirst first" only when there is thirst: with nothing much to
        // do the goblin says so instead
        self.goblin(
            None,
            if need >= crate::stealth::NEED_BUSY {
                crate::goblin::Mood::SessionOpen
            } else {
                crate::goblin::Mood::SessionOpenQuiet
            },
        );
    }

    /// An open session, judged by what there is to do: one that ran out
    /// with plenty left stays a little longer (up to a cap); one with
    /// nothing to do for a while closes early - feet up. Returns whether
    /// the session is still open
    fn session_still_open(&mut self, now: chrono::DateTime<Local>) -> bool {
        let Some(until) = self.burst_until else {
            return false;
        };
        let need = self.need_now();
        let started = self.session_started.unwrap_or(now);
        let age = (now - started).num_minutes();
        // Nothing closes a session the player opened by hand until the
        // fifteen minutes are up
        if self.boosting() {
            return true;
        }
        if now < until {
            // Nothing to do at all for a while: log off early
            if need == crate::stealth::NEED_NONE {
                let since = *self.idle_since.get_or_insert(now);
                if (now - since).num_minutes() >= crate::stealth::IDLE_CLOSE_MIN
                    && age >= 3
                {
                    self.burst_until = None;
                    self.console(
                        None,
                        "Nothing left to do - session closed early".to_string(),
                        None,
                    );
                    self.goblin(None, crate::goblin::Mood::FeetUp);
                    return false;
                }
            } else {
                self.idle_since = None;
            }
            return true;
        }
        // Ran out: stay for one more thing while there is plenty to do
        if need >= crate::stealth::NEED_BUSY
            && age < crate::stealth::SESSION_CAP_MIN
        {
            let more = crate::stealth::extend_minutes();
            self.burst_until = Some(now + chrono::Duration::minutes(more));
            self.console(
                None,
                format!("Still busy ({need} % to do) - session stays {more} min longer"),
                None,
            );
            return true;
        }
        self.burst_until = None;
        false
    }

    /// Is "do it now" running?
    pub fn boosting(&self) -> bool {
        self.boost_until.is_some_and(|t| Local::now() < t)
    }

    /// Is anything still being worked on: a job in any queue, or a
    /// character in the middle of a request
    pub fn work_in_hand(&self) -> bool {
        self.servers.0.values().any(|s| {
            s.accounts.values().any(|a| {
                !a.queue.is_empty()
                    || matches!(&*a.status.lock_safe(), AccountStatus::Busy(..))
            })
        })
    }

    /// Jobs waiting in every queue together - what the button counts down
    pub fn jobs_waiting(&self) -> usize {
        self.servers
            .0
            .values()
            .flat_map(|s| s.accounts.values())
            .map(|a| a.queue.len())
            .sum()
    }

    /// Back to the slider's own schedule.
    ///
    /// The next session is drawn from *now*, not resumed from whatever was
    /// pending a quarter of an hour ago: everything due has just been
    /// done, so opening a session the same second would be the app
    /// clicking an empty round - and a player who has just finished
    /// theirs puts the phone down
    fn end_boost(&mut self) {
        self.boost_until = None;
        self.boost_idle_since = None;
        self.burst_until = None;
        if self.config.off() {
            return;
        }
        let now = Local::now();
        let need = self.need_now();
        self.burst_next = Some(
            now + chrono::Duration::seconds(crate::stealth::session_gap_secs(
                self.config.gap_minutes(),
                need,
                now.hour(),
            )),
        );
    }

    /// The safety slider's blend of lengths, gaps and ceilings
    pub fn profile(&self) -> crate::stealth::Profile {
        crate::stealth::Profile::from_safety(self.config.safety)
    }

    /// The quiet hours are their own tick in Settings now - the slider
    /// says how much the app plays, not whether it sleeps at night. It
    /// used to switch them off below 15 and widen them above 50, so a
    /// player who had ticked "quiet hours" could find them silently gone
    fn in_quiet_hours(&mut self, now: chrono::DateTime<Local>) -> bool {
        if !self.config.quiet_hours {
            return false;
        }
        let today = now.date_naive();
        let shift = match self.quiet_shift {
            Some((d, s)) if d == today => s,
            _ => {
                let s = crate::stealth::draw_shift();
                self.quiet_shift = Some((today, s));
                s
            }
        };
        crate::stealth::in_quiet(
            now.time(),
            self.config.quiet_from,
            self.config.quiet_to,
            shift,
            0,
        )
    }

    /// Sessions: true while a session is open; opens the next one when due.
    /// Always true with sessions off - except in the quiet hours, which
    /// hold in every mode
    /// Whether anything at all may go to the game right now.
    ///
    /// Two reasons it may not: the player pressed STOP, or the licence has
    /// run out. They are one question at every place that sends something,
    /// so that a new send path cannot be written that honours one and
    /// forgets the other
    pub fn halted(&self) -> bool {
        self.halt_reason().is_some()
    }

    /// Why nothing may go to the game, in words, or `None` if it may.
    ///
    /// The log used to say "not sent - STOP" whatever the reason, so a
    /// character sitting idle because of a licence read as one the player
    /// had stopped by hand. A reason that is always the same word is not a
    /// reason
    #[must_use]
    pub fn halt_reason(&self) -> Option<&'static str> {
        if self.paused {
            return Some("STOP");
        }
        if !self.licence.may_run() {
            return Some("the licence");
        }
        None
    }

    /// Read the stored code, judge it against the clock and this machine,
    /// and remember the answer. Also advances the remembered clock, which
    /// is what makes winding the machine's clock back pointless
    pub fn refresh_licence(&mut self) {
        use crate::licence;
        let (judge_by, mark) = licence::effective_now(
            licence::now_unix(),
            self.config.licence_clock_mark,
        );
        if mark != self.config.licence_clock_mark {
            self.config.licence_clock_mark = mark;
            _ = self.config.write();
        }
        self.licence = if self.config.licence.trim().is_empty() {
            licence::Status::Missing
        } else {
            match licence::verify(&self.config.licence, &licence::PUBLIC_KEY) {
                Ok(l) => licence::check(&l, judge_by, &licence::fingerprint()),
                Err(e) => licence::Status::Bad(e),
            }
        };
    }

    fn burst_active(&mut self) -> bool {
        let now = Local::now();
        // The order these are asked in is the whole decision, so it is
        // written down once in `pace::gate` with a test on it. It used to
        // be spelled out here, with the slider's left stop tested before
        // "do it now" - which is how pressing the button at OFF could put a
        // countdown and a green band on screen and send nothing
        let boosting = self.boosting();
        match crate::pace::gate(
            boosting,
            self.config.off(),
            self.in_quiet_hours(now),
        ) {
            // Asked for by hand, with the player sitting there watching. No
            // session runs underneath it; it ends on its own (see
            // `BOOST_MINUTES`), and the next session is drawn then
            crate::pace::Gate::GoingThroughIt => {
                self.burst_until = None;
                return true;
            }
            crate::pace::Gate::SliderOff => {
                // The quarter of an hour is up, or was never running
                if self.boost_until.is_some() {
                    self.end_boost();
                }
                return false;
            }
            crate::pace::Gate::Quiet | crate::pace::Gate::Schedule => {}
        }
        // The quarter of an hour is up: hand the wheel back to the slider
        if self.boost_until.is_some() {
            self.end_boost();
        }
        if self.in_quiet_hours(now) {
            // Said once a night, plainly - the feed should show why
            // nothing is happening at four in the morning
            if self.quiet_said != Some(now.date_naive()) {
                self.quiet_said = Some(now.date_naive());
                self.console(
                    None,
                    format!(
                        "Quiet hours ({}:00-{}:00, moved a little every day) -                          nothing goes out until they are over",
                        self.config.quiet_from, self.config.quiet_to
                    ),
                    None,
                );
                self.goblin(None, crate::goblin::Mood::QuietHours);
            }
            return false;
        }
        if self.config.sessions_level == crate::config::SessionsLevel::Off {
            return true;
        }
        if self.burst_until.is_some() {
            if self.session_still_open(now) {
                return true;
            }
            // Closed early: the next one comes when it was due anyway
            // (a quick return is drawn only by an open that ran its course)
            if self.burst_next.is_some_and(|t| now < t) {
                return false;
            }
        }
        match self.burst_next {
            Some(next) if now >= next => {
                self.open_burst();
                self.burst_until.is_some()
            }
            Some(_) => false,
            None => {
                self.open_burst();
                self.burst_until.is_some()
            }
        }
    }

    /// True when the queue holds something more urgent than `job`. Feeding a
    /// hundred pets or playing ten games of dice is a chain of requests that
    /// holds the character for minutes, and those chains bypass the queue
    /// entirely - so the arena's ten-minute fight quietly expired while they
    /// ran. A chain that sees this yields: it puts itself back in the queue
    /// and the urgent job goes first
    fn outranked_in_queue(&mut self, ident: AccountIdent, job: Job) -> bool {
        self.servers.get_account_mut(&ident).is_some_and(|a| {
            a.queue
                .iter()
                .any(|(waiting, _)| waiting.priority() < job.priority())
        })
    }

    /// Puts `job` back in the queue and stops the chain it was running
    fn yield_to_queue(
        &mut self,
        ident: AccountIdent,
        job: Job,
    ) -> Command<Message> {
        log::info!(
            "{ident}: {} yielding - something ahead of it is waiting",
            job.key()
        );
        Command::perform(async {}, move |()| Message::Enqueue { ident, job })
    }

    /// The daily Discord invite, from `admin::DISCORD_POSTER` and nobody
    /// else: it is that character's guild the invite is for. `None` when
    /// the character is not logged in or not free right now, so the caller
    /// tries again on the next tick instead of writing the day off
    fn post_discord_invite(&mut self) -> Option<Command<Message>> {
        if !self.config.admin_unlocked {
            return None;
        }
        let msg = crate::admin::DISCORD_INVITE.to_string();
        let poster = self
            .servers
            .0
            .values()
            .flat_map(|s| s.accounts.values())
            .find(|a| {
                a.name.trim().to_lowercase() == crate::admin::DISCORD_POSTER
                    && matches!(&*a.status.lock_safe(), AccountStatus::Idle(..))
            })
            .map(|a| a.ident);
        let Some(ident) = poster else {
            log::info!("discord invite waits - the poster is not free");
            return None;
        };
        self.console(
            Some(ident),
            format!("Guild chat, daily invite: {msg}"),
            Some(true),
        );
        Some(self.send_simple(
            ident,
            "Chat",
            sf_api::command::Command::GuildSendChat { message: msg },
            SimpleNote::GuildChat,
        ))
    }

    /// Posts one line into the guild chat, from an owner character. Only one
    /// of them ever posts, so two logged in owners do not say it twice
    fn post_to_guild_chat(&mut self, msg: String) -> Command<Message> {
        if !self.config.admin_unlocked || msg.trim().is_empty() {
            return Command::none();
        }
        let owner = self
            .servers
            .0
            .values()
            .flat_map(|s| s.accounts.values())
            .find(|a| {
                crate::admin::is_owner(&a.name)
                    && matches!(&*a.status.lock_safe(), AccountStatus::Idle(..))
            })
            .map(|a| a.ident);
        let Some(ident) = owner else {
            log::info!(
                "guild chat post skipped - no owner character is logged in \
                 and free right now"
            );
            return Command::none();
        };
        self.send_simple(
            ident,
            "Chat",
            sf_api::command::Command::GuildSendChat { message: msg },
            SimpleNote::GuildChat,
        )
    }

    /// Notices a level up. The game announces one in its own window, which
    /// the app never sees - so it is spotted by the number going up between
    /// two looks at the character
    fn watch_levels(&mut self) -> LevelWatch {
        let mut celebrate: Vec<(AccountIdent, String)> = Vec::new();
        let mut popups: Vec<(String, u16)> = Vec::new();
        let mut epics: Vec<(String, String)> = Vec::new();
        for server in self.servers.0.values_mut() {
            for account in server.accounts.values_mut() {
                let level = match &*account.status.lock_safe() {
                    AccountStatus::Idle(_, gs) | AccountStatus::Busy(gs, _) => {
                        Some(gs.character.level)
                    }
                    _ => None,
                };
                // A dungeon is ten enemies and the last one is the boss,
                // so a dungeon turning from open to finished is a boss kill
                // (cleared count, what the last fight - the boss - paid)
                let cleared = match &*account.status.lock_safe() {
                    AccountStatus::Idle(_, gs)
                    | AccountStatus::Busy(gs, _) => Some((
                        gs.dungeons
                            .light
                            .iter()
                            .map(|(_, p)| p)
                            .chain(gs.dungeons.shadow.iter().map(|(_, p)| p))
                            .filter(|p| {
                                matches!(
                                    p,
                                    sf_api::gamestate::dungeons::DungeonProgress::Finished
                                )
                            })
                            .count(),
                        gs.last_fight
                            .as_ref()
                            .map(|f| (f.xp_change, f.silver_change)),
                    )),
                    _ => None,
                };
                if let Some((now_cleared, gain)) = cleared {
                    match account.dungeons_cleared {
                        None => account.dungeons_cleared = Some(now_cleared),
                        Some(before) if now_cleared > before => {
                            account.dungeons_cleared = Some(now_cleared);
                            log::info!(
                                "{}: a dungeon boss went down ({before} -> \
                                 {now_cleared} cleared)",
                                account.ident
                            );
                            // The boss was the last fight, so its payout is
                            // the number this line is read for
                            let paid = match gain {
                                Some((xp, silver)) if xp > 0 || silver != 0 => {
                                    format!(
                                        " +{xp} XP, {:+} gold",
                                        silver / 100
                                    )
                                }
                                _ => String::new(),
                            };
                            celebrate.push((
                                account.ident,
                                format!(
                                    "Dungeon cleared - the boss is down!{paid}"
                                ),
                            ));
                        }
                        Some(_) => account.dungeons_cleared = Some(now_cleared),
                    }
                }

                // Epics are the one find worth interrupting whatever the
                // user is looking at. Counting them - worn plus the whole
                // bag - is enough: the number only goes up when one arrives,
                // and moving a piece from the bag onto the character keeps
                // the total the same, so equipping never raises a false one
                // Which epic is the new one is told by its ident, not by
                // its place: the last epic in the bag used to be shown,
                // which was whatever old epic happened to sit at the end -
                // a talisman nobody wanted, with minus gains all round
                let epic_now = match &*account.status.lock_safe() {
                    AccountStatus::Idle(_, gs) | AccountStatus::Busy(gs, _) => {
                        let mut n = 0usize;
                        let mut idents: Vec<String> = Vec::new();
                        let mut newest: Option<String> = None;
                        let worn = gs
                            .character
                            .equipment
                            .0
                            .iter()
                            .map(|(_, w)| w.as_ref());
                        let bag = gs.character.inventory.iter().map(|(_, i)| i);
                        for item in worn.chain(bag).flatten() {
                            if !item.is_epic() {
                                continue;
                            }
                            n += 1;
                            let id = item.command_ident().to_string();
                            if !account.epic_idents.contains(&id)
                                && newest.is_none()
                            {
                                // Which kind of epic, and what it would do
                                // for everybody who could wear it - "EPIC!
                                // Belt" alone left the question of whether
                                // to put it on
                                newest = Some(format!(
                                    "{:?} ({}): {}",
                                    item.typ,
                                    crate::shopping::item_kind(item),
                                    crate::shopping::gains_line(gs, item)
                                ));
                            }
                            idents.push(id);
                        }
                        Some((n, newest, idents))
                    }
                    _ => None,
                };
                if let Some((now_epics, newest, idents)) = epic_now {
                    let first_look = account.epics_seen.is_none();
                    account.epic_idents = idents.into_iter().collect();
                    // Where it came from: whatever the character was last
                    // sent to do - the dungeon, the tavern, the dice, the
                    // shop - so nobody has to work out where it dropped
                    let origin = account
                        .last_job_done
                        .map(|(label, at)| {
                            format!(
                                " - from {} at {}",
                                label,
                                at.format("%H:%M")
                            )
                        })
                        .unwrap_or_default();
                    match account.epics_seen {
                        // The first look only takes stock
                        None if first_look => {
                            account.epics_seen = Some(now_epics)
                        }
                        None => {}
                        Some(before) if now_epics > before => {
                            account.epics_seen = Some(now_epics);
                            let legendary = newest
                                .as_deref()
                                .is_some_and(|n| n.contains("egendar"));
                            let what = newest
                                .unwrap_or_else(|| "an epic item".to_string())
                                + &origin;
                            log::info!(
                                "{}: EPIC found - {what} ({before} -> \
                                 {now_epics})",
                                account.ident
                            );
                            celebrate.push((
                                account.ident,
                                if legendary {
                                    format!("LEGENDARY! {what}")
                                } else {
                                    format!("EPIC! {what}")
                                },
                            ));
                            epics.push((
                                titlecase::titlecase(&account.name),
                                what,
                            ));
                        }
                        // Sold, or the bag has not been read yet - follow it
                        // down quietly
                        Some(_) => account.epics_seen = Some(now_epics),
                    }
                }

                // 6.1: a potion drunk - one more active than last time
                let potions = match &*account.status.lock_safe() {
                    AccountStatus::Idle(_, gs) | AccountStatus::Busy(gs, _) => {
                        let active: Vec<String> = gs
                            .character
                            .active_potions
                            .iter()
                            .flatten()
                            .map(|p| format!("{:?} ({:?})", p.typ, p.size))
                            .collect();
                        Some(active)
                    }
                    _ => None,
                };
                if let Some(active) = potions {
                    let now = active.len();
                    if let Some(before) = account.potions_seen
                        && now > before
                    {
                        celebrate.push((
                            account.ident,
                            format!(
                                "POTION! {}",
                                active.last().cloned().unwrap_or_default()
                            ),
                        ));
                    }
                    account.potions_seen = Some(now);
                }

                let Some(level) = level else { continue };
                match account.last_level {
                    // First look - nothing to celebrate, just remember it
                    None => account.last_level = Some(level),
                    Some(before) if level > before => {
                        account.last_level = Some(level);
                        account.level_up = Some((level, Local::now()));
                        log::info!(
                            "{}: level {before} -> {level}", account.ident
                        );
                        celebrate.push((
                            account.ident,
                            format!("LEVEL {level}! Congratulations"),
                        ));
                        // One box per level, so somebody who was away all
                        // night clicks through exactly as many as they made
                        for lvl in (before + 1)..=level {
                            popups.push((
                                titlecase::titlecase(&account.name),
                                lvl,
                            ));
                        }
                    }
                    // A level can go down on a fresh login before the real
                    // character arrives - follow it, but say nothing
                    Some(_) => account.last_level = Some(level),
                }
            }
        }
        (celebrate, popups, epics)
    }

    fn run_queues(&mut self) -> Command<Message> {
        if self.halted() {
            return Command::none();
        }
        if !self.burst_active() {
            return Command::none();
        }
        let now = Local::now();
        if self
            .last_job
            .is_some_and(|t| (now - t).num_seconds() < self.global_gap)
        {
            return Command::none();
        }
        // Every character gets its turn as soon as its own pause is over -
        // the one that has waited longest goes first. It used to be one
        // character at a time, focus held for minutes, the rest queueing:
        // with five characters that looked like the bot was asleep, and the
        // game does not care whether two characters' requests interleave a
        // few seconds apart - still only one request goes out at a time
        // Only a character that is free for a request is a candidate. One
        // in the middle of a request used to be picked all the same - it
        // had waited longest - and since it could not run anything, nobody
        // ran: one slow answer held up every other character
        // ...and not one still in its own delay into the session, nor one
        // that has hit its ceiling of requests a minute (see `stealth`) -
        // a full queue must never turn into the burst only a script sends
        let cap = crate::stealth::request_cap(
            self.config.requests_per_min,
            self.config.pace,
            &self.profile(),
        );
        let candidates: Vec<_> = self
            .servers
            .0
            .values()
            .flat_map(|s| s.accounts.values())
            .filter(|a| !a.queue.is_empty())
            .filter(|a| a.not_before.is_none_or(|t| t <= now))
            .filter(|a| a.requests_last_minute(now) < cap)
            .map(|a| {
                let idle =
                    matches!(&*a.status.lock_safe(), AccountStatus::Idle(..));
                (a.last_dispatch, a.gap, idle, a.ident)
            })
            .collect();
        let Some(focus) = crate::player::next_turn(now, &candidates) else {
            return Command::none();
        };
        let mut todo: Option<(AccountIdent, Job)> = None;
        'outer: for server in self.servers.0.values_mut() {
            for account in server.accounts.values_mut() {
                if account.ident != focus {
                    continue;
                }
                if account.queue.is_empty() {
                    continue;
                }
                if account
                    .last_dispatch
                    .is_some_and(|t| (now - t).num_seconds() < account.gap)
                {
                    continue;
                }
                let idle = {
                    let arc = account.status.clone();
                    let lock = arc.lock_safe();
                    match &*lock {
                        AccountStatus::Idle(_, gs) => {
                            // Today's mushroom floor, before anything spends
                            account.roll_mushroom_day(gs);
                            // ...and what the purse says has gone out
                            // today, so the day's budget holds across
                            // restarts (see `spend_ledger`)
                            let key = crate::probe_history::ProbeHistory::key(
                                &server.ident.url, &account.name,
                            );
                            let day = gs.server_time().current().date();
                            account.tavern_info.mushroom_spent_today =
                                self.spend.note_mushrooms(
                                    &key, day, gs.character.mushrooms,
                                );
                            // ...and the game's own list of fights (the
                            // third tab of the post) comes with an
                            // ordinary poll, so the whole history costs
                            // nothing to read. Everything new in it goes
                            // into the record the arena's odds are
                            // calibrated from (see `fight_log`)
                            self.fights.absorb(&key, &gs.mail.combat_log);
                            true
                        }
                        _ => false,
                    }
                };
                if !idle {
                    continue;
                }
                // Jobs whose automation got switched off meanwhile are
                // dropped, not run
                let enabled: Vec<bool> = account
                    .queue
                    .iter()
                    .map(|(j, _)| account_job_enabled(account, *j))
                    .collect();
                let mut i = 0;
                account.queue.retain(|_| {
                    let keep = enabled[i];
                    i += 1;
                    keep
                });
                // The tavern first. While the last tavern step sent
                // something (`stalled` is false) nothing else of this
                // character goes out - the clearings of an expedition were
                // being interleaved with arena manager purchases and
                // fortress checks, which stretched a day's thirst over
                // hours. Once the tavern has nothing to send, the rest of
                // the queue gets its turn
                let tavern_first = account.expedition_info.auto
                    && !account.expedition_info.stalled;
                if tavern_first
                    && !account.queue.iter().any(|(j, _)| *j == Job::Expedition)
                {
                    account.queue.push((Job::Expedition, now));
                }
                // What to run is a judgement, not a running order: every
                // waiting job is scored by where the player put it, how
                // close it is to being lost, and how long it has waited
                // (see `Job::score`). Without the game state there is
                // nothing to judge by, so the configured order stands
                let scored = {
                    let arc = account.status.clone();
                    let lock = arc.lock_safe();
                    match &*lock {
                        AccountStatus::Idle(_, gs) => Some(
                            account
                                .queue
                                .iter()
                                .map(|(j, at)| {
                                    j.score(gs, (now - *at).num_seconds())
                                })
                                .collect::<Vec<_>>(),
                        ),
                        _ => None,
                    }
                };
                let candidates = account
                    .queue
                    .iter()
                    .enumerate()
                    // The tavern is walked through in one go: the daily
                    // reward on the way in, the dice with the cup player,
                    // the thirst, the wheel on the way out. It used to be
                    // `Job::Expedition` alone here, which meant the dice
                    // and the day's reward sat in the queue for as long as
                    // the expedition chain had anything to send - on a new
                    // day, with a full thirst, that was the whole visit
                    .filter(|(_, (j, _))| {
                        !tavern_first || Job::TAVERN.contains(j)
                    });
                let Some(pos) = (match &scored {
                    Some(scores) => candidates
                        .max_by(|(a, _), (b, _)| {
                            scores[*a]
                                .partial_cmp(&scores[*b])
                                .unwrap_or(std::cmp::Ordering::Equal)
                        })
                        .map(|(i, _)| i),
                    None => candidates
                        .min_by_key(|(_, (j, at))| (j.priority(), *at))
                        .map(|(i, _)| i),
                }) else {
                    continue;
                };
                let (job, queued_at) = account.queue.remove(pos);
                let waited = (now - queued_at).num_seconds();
                if waited > 120 {
                    log::info!(
                        "{}: {} waited {waited}s in the queue",
                        account.ident,
                        job.key()
                    );
                }
                account.last_dispatch = Some(now);
                account.last_job_done = Some((job.label(), now));
                // The next pause is drawn now, so a human-like schedule is
                // never a fixed rhythm. Between sessions it is minutes,
                // not seconds - a look-in, not a shift
                account.gap = crate::pace::account_gap();
                todo = Some((account.ident, job));
                break 'outer;
            }
        }
        let Some((ident, job)) = todo else {
            return Command::none();
        };
        self.last_job = Some(now);
        // While "do it now" runs the player is watching the feed, so the
        // jobs come a handful of seconds apart - close enough to read as
        // working, never a stretch long enough to read as stopped
        self.global_gap = if self.boosting() {
            crate::pace::boost_gap()
        } else {
            crate::pace::global_gap()
        };
        // Only the file log gets the dispatch line. The Live console used to
        // get one too, for every job picked off the queue - but most picks are
        // routine no-ops (a guard job while a shift is already running, an
        // arena check before the timer is up), so on Full Power that filled
        // the console with the same few labels over and over. What actually
        // happened - a fight won, souls collected, a build started - is logged
        // by each command's own result, which is the useful half
        log::info!("{ident}: running {}", job.label());
        // The window follows the job: the page it works on, on the
        // character it works for. Only while a character page is open -
        // Settings, the overview and the login stay where the player left
        // them - and the mouse is never touched, only what is shown
        if self.config.follow_bot
            && matches!(self.current_view, View::Account { .. })
        {
            self.current_view = View::Account {
                ident,
                page: job.page(),
            };
        }
        self.update(job.message(ident))
    }

    /// Feeds one pet: in every habitat only the highest level pet gets that
    /// habitat's fruit - 3 per pet and day is the game's limit, lower pets
    /// are never fed. The result handler calls this again until nothing is
    /// left to do. If the server says OK but nothing changes, we stop
    fn pet_feed(
        &mut self,
        ident: AccountIdent,
        _attempt: u8,
    ) -> Command<Message> {
        let Some(account) = self.servers.get_account_mut(&ident) else {
            return Command::none();
        };
        account.pet_info.roll_day();
        if account.pet_info.fed_this_round >= 120 {
            account.pet_info.status =
                Some("Feeding stopped - too many feeds in one round".into());
            return Command::none();
        }

        let arc = account.status.clone();
        let status = arc.lock_safe();
        let AccountStatus::Idle(_, gs) = &*status else {
            return Command::none();
        };
        let Some(pets) = gs.pets.as_ref() else {
            drop(status);
            account.pet_info.status =
                Some("This character has no pets yet".to_string());
            return Command::none();
        };

        // Did the last feed actually change anything? A `PetFeed` answer
        // carries no proof, so the proof comes from the state: total fruit in
        // every habitat plus what every pet has eaten today. One real feed
        // moves this by one. Three accepted feeds that move nothing means the
        // game is taking the request and dropping it, and carrying on would
        // only write fiction into the log
        let fingerprint = {
            let mut sum = 0u64;
            for (_, h) in &pets.habitats {
                sum = sum.saturating_add(u64::from(h.fruits) * 1000);
                for p in &h.pets {
                    sum = sum.saturating_add(u64::from(p.fruits_today));
                }
            }
            sum
        };
        if account.pet_info.last_was_refusal {
            // An honest "no" is not a phantom feed - the request never got
            // as far as changing anything
            account.pet_info.last_was_refusal = false;
            account.pet_info.feed_fingerprint = None;
        } else if account.pet_info.feed_fingerprint == Some(fingerprint) {
            account.pet_info.phantom_feeds += 1;
        } else {
            account.pet_info.phantom_feeds = 0;
        }
        if account.pet_info.phantom_feeds >= 3 {
            drop(status);
            let msg = "Stopped: the game accepts the feeds but nothing \
                       changes - no fruit leaves the habitat and no pet eats"
                .to_string();
            log::warn!("{ident}: pets - three feeds accepted with no effect");
            account
                .pet_info
                .log
                .push((Local::now(), msg.clone(), false));
            account.pet_info.status = Some(msg);
            account.pet_info.fed_this_round = 0;
            account.pet_info.feed_fingerprint = None;
            account.pet_info.phantom_feeds = 0;
            account.pet_info.last_feed = None;
            return Command::none();
        }

        let max_fruits_per_day = crate::mechanics::pet_feed_limit(gs);
        let max_pet_level = pets.max_pet_level.max(100);
        // What the wire's second number really is. sf-api 0.4.4 calls it
        // `fruit_idx`, and this code used to send the habitat number - which
        // is why only the Shadow habitat (number 0) ever got fed: the server
        // treats 0 as "no check" and refused every other habitat's 1..4.
        // The library's current source documents it as the TOTAL fruit count
        // across all habitats, a checksum so a request resent after bad
        // network does not feed a pet twice. Same wire format either way
        // (`PlayerPetFeed:{pet}/{n}`), so the fix is just the right number
        let total_fruit: u32 =
            pets.habitats.iter().map(|(_, h)| u32::from(h.fruits)).sum();
        // (wire pet id, habitat number for our own bookkeeping, the pet's
        // real id, log line)
        let mut todo: Option<(u32, u32, u32, String)> = None;
        let mut full = 0;
        let mut no_fruit = 0;
        // The habitats in the order of what they give this character
        // (5.4): its main attribute's element first, then Shadow
        // (constitution) and Fire (luck), the other two last. Water is
        // strength, Light dexterity, Earth intelligence
        let class_main = gs.character.class.main_attribute();
        let rank = |h: sf_api::gamestate::unlockables::HabitatType| {
            use sf_api::{
                command::AttributeType as A,
                gamestate::unlockables::HabitatType as H,
            };
            let main_elem = match class_main {
                A::Strength => H::Water,
                A::Dexterity => H::Light,
                _ => H::Earth,
            };
            if h == main_elem {
                0
            } else if h == H::Shadow {
                1
            } else if h == H::Fire {
                2
            } else {
                3
            }
        };
        let mut habitats: Vec<_> = pets.habitats.iter().collect();
        habitats.sort_by_key(|(h, _)| rank(*h));
        'habitats: for (habitat, info) in habitats {
            if info.fruits == 0 {
                no_fruit += 1;
                continue;
            }
            // Strongest pet first - fruit is scarce and he is the one on his
            // way to 100. A weaker pet only gets what is truly spare
            let mut order: Vec<&sf_api::gamestate::unlockables::Pet> = info
                .pets
                .iter()
                .filter(|p| p.level > 0 && p.level < max_pet_level)
                .collect();
            order.sort_by_key(|p| std::cmp::Reverse(p.level));
            let mut past_the_best = false;
            for pet in order {
                if past_the_best
                    && !other_pet_may_eat(
                        info.fruits, account.pet_info.spread_fruit,
                    )
                {
                    break;
                }
                past_the_best = true;
                if account.pet_info.done_today.contains(&pet.id) {
                    full += 1;
                    continue;
                }
                let eaten = pet.fruits_today.max(
                    account
                        .pet_info
                        .fed_today
                        .get(&pet.id)
                        .copied()
                        .unwrap_or(0),
                );
                if eaten >= max_fruits_per_day {
                    full += 1;
                    continue;
                }
                // `Pet.id` is the unique id across all habitats. The habitat
                // number is kept only for our own fruit bookkeeping - it is
                // NOT what goes on the wire (see `total_fruit` above)
                let habitat_idx = habitat as u32;
                todo = Some((
                    pet.id,
                    habitat_idx,
                    pet.id,
                    format!(
                        "{habitat:?} pet #{} (lvl {}, {}/{max_fruits_per_day} \
                         today, {} fruit left)",
                        pet.id,
                        pet.level,
                        eaten + 1,
                        info.fruits
                    ),
                ));
                break 'habitats;
            }
        }
        drop(status);

        let Some((sent_id, habitat_idx, pet_id, name)) = todo else {
            account.pet_info.status = Some(format!(
                "Nothing to feed: {full} pets have had enough, {no_fruit} \
                 habitats without fruit ({})",
                Local::now().format("%H:%M")
            ));
            account.pet_info.fed_this_round = 0;
            account.pet_info.last_feed = None;
            account.pet_info.feed_fingerprint = None;
            account.pet_info.phantom_feeds = 0;
            account.pet_info.opened_for = None;
            return self.pet_arena(ident);
        };

        // The official client opens a pet before it feeds it, the same way it
        // opens the dungeon screen before a fight. Open first, feed once the
        // answer comes back
        if account.pet_info.opened_for != Some(pet_id)
            && let Ok(view_id) = u16::try_from(pet_id)
        {
            account.pet_info.opened_for = Some(pet_id);
            account.pet_info.fed_this_round += 1;
            return self.send_simple(
                ident,
                "Pets",
                sf_api::command::Command::ViewPet { pet_id: view_id },
                SimpleNote::PetOpened(name),
            );
        }
        account.pet_info.fed_this_round += 1;
        account.pet_info.last_feed = Some((pet_id, habitat_idx as u8));
        account.pet_info.last_was_refusal = false;
        account.pet_info.feed_fingerprint = Some(fingerprint);
        self.send_simple(
            ident,
            "Pets",
            // The field is misnamed in sf-api 0.4.4: it carries the total
            // fruit count, not a fruit index (see `total_fruit`)
            sf_api::command::Command::PetFeed {
                pet_id: sent_id,
                fruit_idx: total_fruit,
            },
            SimpleNote::PetFed {
                pet: name,
                attempt: 0,
            },
        )
    }

    /// One step of a sacrifice the player confirmed: the gem out at the
    /// blacksmith if there is one (it goes into the bag and lives on), then
    /// the legendary taken apart for its materials. Only ever a legendary,
    /// and only the one at `pos` - if the bag changed, nothing happens
    fn sacrifice_step(
        &mut self,
        ident: AccountIdent,
        pos: usize,
        after_gem: bool,
    ) -> Command<Message> {
        use sf_api::gamestate::items::GemSlot;
        let Some(account) = self.servers.get_account_mut(&ident) else {
            return Command::none();
        };
        let arc = account.status.clone();
        let lock = arc.lock_safe();
        let AccountStatus::Idle(_, gs) = &*lock else {
            return Command::none();
        };
        let Some(item) = gs
            .character
            .inventory
            .backpack
            .get(pos)
            .and_then(|i| i.as_ref())
            .filter(|i| i.is_legendary())
        else {
            drop(lock);
            account.shop_info.status =
                Some("Sacrifice: the legendary is no longer there".into());
            return Command::none();
        };
        let item_ident = item.command_ident();
        let has_gem = matches!(item.gem_slot, Some(GemSlot::Filled(_)));
        let free = gs.character.inventory.count_free_slots();
        let name = item
            .typ
            .equipment_slot()
            .map_or_else(|| "piece".to_string(), |s| format!("{s:?}"));
        let Some(bag_pos) = gs
            .character
            .inventory
            .iter()
            .map(|(p, _)| p)
            .find(|p| p.backpack_pos() == pos)
        else {
            return Command::none();
        };
        drop(lock);
        if has_gem && !after_gem {
            if free == 0 {
                account.shop_info.status = Some(
                    "Sacrifice: the gem needs a free bag slot first".into(),
                );
                return Command::none();
            }
            return self.send_simple(
                ident,
                "Shop",
                sf_api::command::Command::Blacksmith {
                    item_pos: bag_pos.into(),
                    action: sf_api::command::BlacksmithAction::GemExtract,
                    item_ident,
                },
                SimpleNote::Sacrifice {
                    pos,
                    gem_out: true,
                    what: format!("took the gem out of the legendary {name}"),
                },
            );
        }
        self.send_simple(
            ident,
            "Shop",
            sf_api::command::Command::Blacksmith {
                item_pos: bag_pos.into(),
                action: sf_api::command::BlacksmithAction::Dismantle,
                item_ident,
            },
            SimpleNote::Sacrifice {
                pos,
                gem_out: false,
                what: format!("the legendary {name} was taken apart. F."),
            },
        )
    }

    /// Sells the one piece the backpack can best spare, if there is one.
    /// Used both by the fortress housekeeping (which keeps a slot free ahead
    /// of time) and by the dungeon step (which only notices once it is too
    /// late)
    fn backpack_sale(
        &mut self,
        ident: AccountIdent,
    ) -> Option<Command<Message>> {
        let account = self.servers.get_account_mut(&ident)?;
        let protection = account.shop_info.protection();
        let keep_free = usize::from(account.shop_info.keep_free.max(1));
        let dismantle_ok = account
            .shop_info
            .dismantle_blocked_until
            .is_none_or(|t| t <= Local::now());
        let arc = account.status.clone();
        let found = {
            let lock = arc.lock_safe();
            let AccountStatus::Idle(_, gs) = &*lock else {
                return None;
            };
            let can_dismantle = dismantle_ok && gs.blacksmith.is_some();
            let sale = crate::backpack::next_sale(
                gs,
                &protection,
                keep_free,
                can_dismantle,
            )?;
            let item_ident = gs
                .character
                .inventory
                .backpack
                .get(sale.pos.backpack_pos())
                .and_then(|i| i.as_ref())
                .map(sf_api::gamestate::items::Item::command_ident)?;
            let (used, total) = crate::backpack::space(gs);
            (sale, item_ident, used, total)
        };
        let (sale, item_ident, used, total) = found;
        Some(match sale.disposal {
            crate::backpack::Disposal::Sell => self.send_simple(
                ident,
                "Shop",
                sf_api::command::Command::SellShop {
                    item_pos: sale.pos.into(),
                    item_ident,
                },
                SimpleNote::BackpackCleared(format!(
                    "Backpack {used}/{total} - sold the {} to make room",
                    sale.what
                )),
            ),
            crate::backpack::Disposal::Dismantle => self.send_simple(
                ident,
                "Shop",
                sf_api::command::Command::Blacksmith {
                    item_pos: sale.pos.into(),
                    action: sf_api::command::BlacksmithAction::Dismantle,
                    item_ident,
                },
                SimpleNote::BackpackDismantled(format!(
                    "Backpack {used}/{total} - dismantled the {} at the \
                     blacksmith to make room",
                    sale.what
                )),
            ),
        })
    }

    /// One pet arena fight. The opponent is the same player for all five
    /// habitats, but each habitat may attack them once a day - so this sends
    /// the strongest habitat that has not gone yet, exactly the one the game
    /// itself pre-selects
    fn pet_arena(&mut self, ident: AccountIdent) -> Command<Message> {
        let Some(account) = self.servers.get_account_mut(&ident) else {
            return Command::none();
        };
        if !account.pet_info.auto_arena {
            return Command::none();
        }
        let arc = account.status.clone();
        let status = arc.lock_safe();
        let AccountStatus::Idle(_, gs) = &*status else {
            return Command::none();
        };
        let Some(pets) = gs.pets.as_ref() else {
            return Command::none();
        };
        let now = Local::now();
        let opponent_id = pets.opponent.id;
        let next_free = pets.opponent.next_free_battle;
        // Only the free fight. Anything else costs a mushroom
        let free = next_free.is_none_or(|t| t <= now);
        // Highest total pet level of the habitats that have not fought today
        let pick = pets
            .habitats
            .iter()
            .filter(|(_, info)| !info.battled_opponent)
            .map(|(habitat, info)| {
                let total: u32 =
                    info.pets.iter().map(|p| u32::from(p.level)).sum();
                (habitat, total)
            })
            .filter(|(_, total)| *total > 0)
            .max_by_key(|(_, total)| *total);
        let enemy_total = pets.opponent.level_total;
        drop(status);

        if opponent_id == 0 {
            account.pet_info.arena_status =
                Some("No pet arena opponent yet".into());
            return Command::none();
        }
        if !free {
            account.pet_info.arena_status = Some(format!(
                "Pet arena: next free fight {}",
                next_free
                    .map(|t| t.format("%d.%m. %H:%M").to_string())
                    .unwrap_or_else(|| "?".to_string())
            ));
            return Command::none();
        }
        let Some((habitat, total)) = pick else {
            account.pet_info.arena_status = Some(format!(
                "Pet arena: every habitat has already fought today ({})",
                now.format("%H:%M")
            ));
            return Command::none();
        };
        account.pet_info.arena_status = Some(format!(
            "Pet arena: {habitat:?} (our level total {total} vs {enemy_total})"
        ));
        self.send_simple(
            ident,
            "Pets",
            sf_api::command::Command::FightPetOpponent {
                habitat,
                opponent_id,
            },
            SimpleNote::PetArena {
                name: format!("Pet arena {habitat:?}"),
            },
        )
    }

    /// Signs the character up for the guild attack/defense, if it is not
    /// signed up yet. Both commands are toggles on the server side, which is
    /// why this only ever runs on freshly loaded guild data
    fn guild_auto_sign(&mut self, ident: AccountIdent) -> Command<Message> {
        use sf_api::gamestate::guild::BattlesJoined;

        // The attendance diary's key and the slider's targets, before the
        // account is borrowed
        let server_url = self
            .servers
            .0
            .get(&ident.server_id)
            .map(|s| s.ident.url.clone())
            .unwrap_or_default();
        let profile = self.profile();
        let Some(account) = self.servers.get_account_mut(&ident) else {
            return Command::none();
        };
        let att_key =
            crate::probe_history::ProbeHistory::key(&server_url, &account.name);
        let (want_attack, want_defense) =
            (account.social.auto_attack, account.social.auto_defense);
        // The portal has nothing to do with the battle sign-ups - it is one
        // swing a day for every member - so it may well be the only reason
        // this runs at all
        let want_portal = account.social.portal_due();
        let want_raid = account.social.raid_auto;
        if !want_attack && !want_defense && !want_portal && !want_raid {
            return Command::none();
        }
        if account.social.sign_attempts >= 3 {
            account.social.status = Some(
                "Signing up did not stick after 3 tries - giving up until \
                 the next check"
                    .to_string(),
            );
            return Command::none();
        }

        let own_name = account.name.to_lowercase();
        let arc = account.status.clone();
        let status = arc.lock_safe();
        let gs = match &*status {
            AccountStatus::Idle(_, gs) | AccountStatus::Busy(gs, _) => gs,
            _ => return Command::none(),
        };
        let Some(guild) = gs.guild.as_ref() else {
            return Command::none();
        };
        let me = guild
            .members
            .iter()
            .find(|m| m.name.to_lowercase() == own_name);
        let joined = me.and_then(|m| m.battles_joined);
        // The game keeps the time of our last portal swing on our own member
        // row. Our `portal_tried` flag lives only in memory, so after a
        // restart (every update is one) it was empty, the bot swung again and
        // the server answered "you can only fight once per day"
        let portal_done_today = me
            .and_then(|m| m.portal_fought)
            .is_some_and(|t| t.date_naive() == Local::now().date_naive());
        let in_attack =
            matches!(joined, Some(BattlesJoined::Attack | BattlesJoined::Both));
        let in_defense = matches!(
            joined,
            Some(BattlesJoined::Defense | BattlesJoined::Both)
        );
        let attack_at = guild.attacking.as_ref().map(|b| b.date);
        let defense_at = guild.defending.as_ref().map(|b| b.date);
        let portal_life = guild.portal.life_percentage;
        // A raid may start when none is running, the cooldown is over, we
        // hold a rank that may declare one, and the guild's 10 000 gold are
        // there (the game keeps gold as silver, times a hundred)
        let now = Local::now();
        let raid_possible = want_raid
            && guild.attacking.is_none()
            && guild.next_attack_possible.is_none_or(|t| t <= now)
            && me.is_some_and(|m| {
                matches!(
                    m.guild_rank,
                    sf_api::gamestate::guild::GuildRank::Leader
                        | sf_api::gamestate::guild::GuildRank::Officer
                )
            })
            && gs.character.silver >= 10_000 * 100;
        // The guild pet (5.4): its free fight against the hydra, and a
        // level when the purse covers it - the gold above the shop's
        // reserve, and mushrooms only out of the day's budget
        let hydra_ready = guild.pet_id != 0
            && guild.hydra.remaining_fights > 0
            && guild.hydra.next_battle.is_none_or(|t| t <= now);
        let pet_level = {
            use sf_api::gamestate::guild::GuildSkill;
            let price = &guild.upgrade_price[GuildSkill::Pet];
            let reserve = gs.character.silver
                * u64::from(account.shop_info.gold_reserve.min(100))
                / 100;
            let gold_ok = price.silver > 0
                && price.silver <= gs.character.silver.saturating_sub(reserve);
            let mush_ok = u32::from(price.mushrooms) <= account.free_mushrooms(gs);
            (guild.pet_id != 0
                && guild.own_pet_lvl < guild.pet_max_lvl
                && gold_ok
                && mush_ok
                && account
                    .social
                    .pet_blocked_until
                    .is_none_or(|t| t <= now))
            .then_some(guild.own_pet_lvl)
        };
        drop(status);
        // Picks (or keeps) a random sign-up moment within the next hour for
        // the given battle. Returns true once that moment has come
        let due = |slot: &mut Option<(DateTime<Local>, DateTime<Local>)>,
                   battle: DateTime<Local>|
         -> (bool, DateTime<Local>) {
            match slot {
                Some((b, at)) if *b == battle => (*at <= now, *at),
                _ => {
                    let until_battle = (battle - now).num_seconds().max(0);
                    // Anywhere from right away to six hours on (5.4 - it
                    // was within the hour), but never later than halfway
                    // to the battle itself: a battle is never missed
                    let window = (6 * 3600).min(until_battle / 2).max(1) as u64;
                    let at = now
                        + chrono::Duration::seconds(
                            fastrand::u64(0..=window) as i64
                        );
                    *slot = Some((battle, at));
                    (at <= now, at)
                }
            }
        };

        // Every duty goes through the attendance diary first: a miss now
        // and then keeps the record under the 100 % no person has. The
        // decision is made once per battle / portal day and remembered
        use crate::attendance::Duty;
        let mut masked: Vec<&'static str> = Vec::new();
        let mut attends =
            |att: &mut crate::attendance::Attendance, duty: Duty, when: i64| {
                let target = crate::stealth::attend_target(duty, &profile);
                let go = att.decide(&att_key, duty, when, target);
                if !go {
                    masked.push(duty.label());
                }
                go
            };

        let mut waiting: Vec<String> = Vec::new();
        let mut todo = None;
        if want_attack
            && !in_attack
            && let Some(battle) = attack_at
            && attends(&mut self.attendance, Duty::Attack, battle.timestamp())
        {
            let (ready, at) = due(&mut account.social.attack_sign_at, battle);
            if ready {
                todo = Some((
                    sf_api::command::Command::GuildJoinAttack,
                    "the attack",
                ));
            } else {
                waiting.push(format!("attack at {}", at.format("%H:%M")));
            }
        }
        if todo.is_none()
            && want_defense
            && !in_defense
            && let Some(battle) = defense_at
            && attends(&mut self.attendance, Duty::Defense, battle.timestamp())
        {
            let (ready, at) = due(&mut account.social.defense_sign_at, battle);
            if ready {
                todo = Some((
                    sf_api::command::Command::GuildJoinDefense,
                    "the defense",
                ));
            } else {
                waiting.push(format!("defense at {}", at.format("%H:%M")));
            }
        }

        // The portal goes last: the sign-ups have a deadline, the portal has
        // all day. A dead portal (0 % life) is one the guild already killed
        // today, so there is nothing to hit
        if todo.is_none() && want_portal && portal_done_today {
            // Already fought before this process started - remember it so the
            // question is not asked again every minute
            account.social.portal_tried = Some(now.date_naive());
        }
        if todo.is_none()
            && want_portal
            && !portal_done_today
            && portal_life > 0
        {
            account.social.portal_tried = Some(now.date_naive());
            if attends(&mut self.attendance, Duty::Portal, now.timestamp()) {
                todo = Some((
                    sf_api::command::Command::GuildPortalBattle,
                    "the portal",
                ));
            }
        }
        // A deliberate miss is said once, in the assistant's words - the
        // decision is remembered, so this line does not repeat
        for duty in masked {
            let name = account.name.clone();
            self.console.push_back(crate::ConsoleLine {
                at: now,
                who: titlecase::titlecase(&name),
                text: format!("Masking: sitting out the {duty} on purpose - attendance kept under 100 %"),
                ok: None,
                dup: false,
            });
        }

        // Nothing to sign up for and nothing running: declare the raid.
        // Once it is declared the sign-up branch above joins it on the
        // next pass
        if todo.is_none() && raid_possible {
            account.social.status = Some("Starting a raid".to_string());
            log::info!("{ident}: no raid running - starting one");
            return self.send_simple(
                ident,
                "Raid",
                sf_api::command::Command::GuildRaid,
                SimpleNote::GuildRaid,
            );
        }

        if todo.is_none() && want_attack && hydra_ready {
            return self.send_simple(
                ident,
                "Guild",
                sf_api::command::Command::GuildPetBattle {
                    use_mushroom: false,
                },
                SimpleNote::GuildPet("Fought the hydra with the guild pet".into()),
            );
        }
        if todo.is_none()
            && want_attack
            && let Some(current) = pet_level
        {
            return self.send_simple(
                ident,
                "Guild",
                sf_api::command::Command::GuildIncreaseSkill {
                    skill: sf_api::gamestate::guild::GuildSkill::Pet,
                    current,
                },
                SimpleNote::GuildPet(format!(
                    "Raised the guild pet to level {}",
                    current + 1
                )),
            );
        }

        let Some((cmd, what)) = todo else {
            if !waiting.is_empty() {
                account.social.status = Some(format!(
                    "Will join the {}",
                    waiting.join(" and the ")
                ));
            }
            return Command::none();
        };
        account.social.sign_attempts += 1;
        self.send_simple(
            ident,
            "Guild",
            cmd,
            SimpleNote::GuildSigned(what.to_string()),
        )
    }

    /// Second half of a smart attack: the opponent has been looked up, so
    /// the game's own soldier recommendation is known. Attack with that many
    /// (plus the safety margin), or wait when there are not enough soldiers
    /// One look at the raid cycle.
    ///
    /// The fortress used to be raided every "about x minutes" - a clock,
    /// and the plainest one in the log. This is the cycle a person has:
    /// the barracks fill (the game's training time sets that), they are
    /// noticed some drawn while later, the soldiers are spent one
    /// opponent at a time on whoever is worth them, and when they run
    /// out a new batch is queued and the character leaves. Nothing here
    /// runs outside a session - the job queue that calls this is gated
    /// like everything else
    fn fortress_raid_tick(&mut self, ident: AccountIdent) -> Command<Message> {
        use crate::{
            mechanics::{Barracks, RaidStep, raid_step},
            player::RaidPhase,
        };
        use sf_api::gamestate::fortress::{
            FortressResourceType, FortressUnitType,
        };

        let Some(account) = self.servers.get_account_mut(&ident) else {
            return Command::none();
        };
        let Some(fi) = account.fortress_info.as_ref() else {
            return Command::none();
        };
        if !fi.auto_attack {
            return Command::none();
        }
        let phase = fi.raid;
        let notice_max = fi.notice_max_min;

        // What the barracks and the stores say right now
        let arc = account.status.clone();
        let status = arc.lock_safe();
        let gs = match &*status {
            AccountStatus::Idle(_, gs) | AccountStatus::Busy(gs, _) => gs,
            _ => return Command::none(),
        };
        let Some(fortress) = gs.fortress.as_ref() else {
            return Command::none();
        };
        let unit = |t: FortressUnitType| {
            let u = &fortress.units[t];
            (
                Barracks {
                    soldiers: u32::from(u.count),
                    in_training: u32::from(u.in_training),
                    limit: u32::from(u.limit),
                },
                u.training.cost,
            )
        };
        let wood = fortress.resources[FortressResourceType::Wood].current;
        let stone = fortress.resources[FortressResourceType::Stone].current;
        let silver = gs.character.silver;
        let affordable = |cost: sf_api::gamestate::fortress::FortressCost| {
            [(cost.wood, wood), (cost.stone, stone), (cost.silver, silver)]
                .iter()
                .map(|(c, have)| if *c == 0 { u64::MAX } else { have / c })
                .min()
                .unwrap_or(0)
                .min(u64::from(u32::MAX)) as u32
        };
        let (soldiers, soldier_cost) = unit(FortressUnitType::Soldier);
        // The defence is topped up before the soldiers - "check the
        // archers and mages, then fill the soldiers back up". They cost
        // next to nothing and they are what stands when somebody raids us
        let refill: Option<(FortressUnitType, u32, String)> = [
            FortressUnitType::Archer,
            FortressUnitType::Magician,
            FortressUnitType::Soldier,
        ]
        .into_iter()
        .find_map(|t| {
            let (b, cost) = unit(t);
            match raid_step(b, affordable(cost)) {
                RaidStep::Recruit(n) => {
                    Some((t, n, format!("Recruiting {n} x {t:?}")))
                }
                _ => None,
            }
        });
        drop(status);

        let Some(fi) = &mut account.fortress_info else {
            return Command::none();
        };
        let now = Local::now();
        match phase {
            RaidPhase::Filling => {
                if let Some((unit, count, what)) = refill {
                    fi.status = Some(what.clone());
                    return self.send_simple(
                        ident,
                        "Fortress",
                        sf_api::command::Command::FortressBuildUnit {
                            unit,
                            count,
                        },
                        SimpleNote::FortressMaint(what),
                    );
                }
                match raid_step(soldiers, affordable(soldier_cost)) {
                    RaidStep::Full => {
                        let wait = crate::stealth::raid_notice_secs(notice_max);
                        fi.raid = RaidPhase::Noticed;
                        fi.next_due = Some(now + chrono::Duration::seconds(wait));
                        fi.status = Some(format!(
                            "Barracks full - raiding in about {} min",
                            wait / 60
                        ));
                    }
                    RaidStep::Recruit(_) => {}
                    RaidStep::Wait => {
                        fi.status = Some(if soldiers.in_training > 0 {
                            format!(
                                "Training {} soldiers, {} standing",
                                soldiers.in_training, soldiers.soldiers
                            )
                        } else {
                            "Waiting for wood and stone to train soldiers"
                                .to_string()
                        });
                    }
                }
                Command::none()
            }
            RaidPhase::Noticed => {
                let due = *fi
                    .next_due
                    .get_or_insert_with(|| now + chrono::Duration::minutes(3));
                if now < due {
                    return Command::none();
                }
                fi.raid = RaidPhase::Raiding {
                    attacks: 0,
                    paid_reroll: false,
                };
                self.fortress_command(ident, FortressActionKind::Attack, true)
            }
            RaidPhase::Raiding { attacks, .. } => {
                // Out of soldiers, or a run that has gone on for longer
                // than any run of raids has a right to: back to filling
                if soldiers.soldiers == 0 || attacks >= 40 {
                    fi.raid = RaidPhase::Filling;
                    fi.next_due = None;
                    return Command::perform(
                        async {
                            crate::pace::sleep_between(2500, 6000).await;
                        },
                        move |()| Message::FortressRaidStep { ident },
                    );
                }
                self.fortress_command(ident, FortressActionKind::Attack, true)
            }
        }
    }

    fn fortress_attack_scouted(
        &mut self,
        ident: AccountIdent,
        auto: bool,
    ) -> Command<Message> {
        let Some(account) = self.servers.get_account_mut(&ident) else {
            return Command::none();
        };
        let margin = account
            .fortress_info
            .as_ref()
            .map(|a| u32::from(a.soldier_margin))
            .unwrap_or(0);

        let arc = account.status.clone();
        let status = arc.lock_safe();
        let gs = match &*status {
            AccountStatus::Idle(_, gs) | AccountStatus::Busy(gs, _) => gs,
            _ => return Command::none(),
        };
        let Some(fortress) = gs.fortress.as_ref() else {
            return Command::none();
        };
        let soldier_unit = &fortress.units
            [sf_api::gamestate::fortress::FortressUnitType::Soldier];
        let available = u32::from(soldier_unit.count);
        let soldier_cost = soldier_unit.training.cost;
        let free_reroll = fortress
            .attack_free_reroll
            .is_some_and(|a| a <= Local::now());
        let reroll_price = fortress.opponent_reroll_price;
        let silver = gs.character.silver;
        let opponent = fortress
            .attack_target
            .and_then(|pid| gs.lookup.lookup_pid(pid));
        let advice = opponent.and_then(|p| {
            p.fortress.as_ref().map(|f| {
                (
                    p.name.clone(),
                    f.soldier_advice,
                    f.lootable_wood,
                    f.lootable_stone,
                )
            })
        });
        drop(status);

        let Some(fi) = &mut account.fortress_info else {
            return Command::none();
        };
        let raiding = matches!(fi.raid, crate::player::RaidPhase::Raiding { .. });

        // In a raid run an opponent that is not worth the soldiers is
        // passed over, the way he does it by hand: a free new one, a paid
        // one once if the price is small change, and then one soldier
        // thrown at them - "abys neplacal 5 vojaku na capka co ma 100
        // kamene"
        if auto && raiding
            && let Some((name, a, loot_wood, loot_stone)) = &advice
            && *a > 0
        {
            let wanted = u32::from(*a) + margin;
            let pays = crate::mechanics::raid_pays(
                *loot_wood,
                *loot_stone,
                wanted,
                soldier_cost.wood,
                soldier_cost.stone,
            );
            let too_strong = wanted > available;
            if !pays || too_strong {
                let paid_already = matches!(
                    fi.raid,
                    crate::player::RaidPhase::Raiding {
                        paid_reroll: true,
                        ..
                    }
                );
                let why = if too_strong {
                    format!("wants {wanted} soldiers, {available} ready")
                } else {
                    format!(
                        "{} wood + {} stone is not worth {wanted} soldiers",
                        loot_wood, loot_stone
                    )
                };
                let name = titlecase::titlecase(name);
                use crate::mechanics::Skip;
                return match crate::mechanics::skip_choice(
                    free_reroll,
                    reroll_price,
                    silver,
                    paid_already,
                    available,
                ) {
                    Skip::Free => {
                        let msg = format!("{name}: {why} - free new opponent");
                        fi.status = Some(msg.clone());
                        fi.log.push((Local::now(), msg, false));
                        self.fortress_command(
                            ident,
                            FortressActionKind::Reroll,
                            true,
                        )
                    }
                    Skip::Paid => {
                        if let crate::player::RaidPhase::Raiding {
                            paid_reroll,
                            ..
                        } = &mut fi.raid
                        {
                            *paid_reroll = true;
                        }
                        let msg = format!(
                            "{name}: {why} - new opponent for {} silver",
                            reroll_price / 100
                        );
                        fi.status = Some(msg.clone());
                        fi.log.push((Local::now(), msg, false));
                        self.fortress_command(
                            ident,
                            FortressActionKind::PaidReroll,
                            true,
                        )
                    }
                    Skip::OneSoldier => {
                        let msg =
                            format!("{name}: {why} - one soldier sent to move on");
                        fi.status = Some(msg.clone());
                        fi.log.push((Local::now(), msg, false));
                        fi.scouted_soldiers =
                            Some((1, format!("{name}: skipped with one soldier")));
                        self.fortress_command(
                            ident,
                            FortressActionKind::Attack,
                            true,
                        )
                    }
                    Skip::GiveUp => {
                        fi.raid = crate::player::RaidPhase::Filling;
                        fi.next_due = None;
                        fi.status = Some(format!("{name}: {why} - run over"));
                        Command::none()
                    }
                };
            }
        }

        let advice = advice
            .map(|(n, a, _, _)| (n, a))
            .filter(|(_, a)| *a > 0);
        let Some((name, advice)) = advice else {
            // No (usable) recommendation - attack the plain way. Marking the
            // scouting as done keeps `fortress_command` from asking again
            fi.status = Some(
                "The game gave no soldier recommendation - using the manual \
                 setting"
                    .to_string(),
            );
            let cap = if fi.attack_soldiers > 0 {
                u32::from(fi.attack_soldiers)
            } else {
                u32::MAX
            };
            fi.scouted_soldiers =
                Some((cap, "no recommendation - manual setting".to_string()));
            return self.fortress_command(
                ident,
                FortressActionKind::Attack,
                auto,
            );
        };

        let wanted = u32::from(advice) + margin;
        let name = titlecase::titlecase(&name);
        if wanted > available {
            // Not enough soldiers for this one. Sitting still means the
            // attack timer keeps running out on an opponent we are never
            // going to beat, so throw a single soldier at them instead: the
            // fight is lost, it costs one soldier, and the next attack gets
            // a fresh opponent that may well be worth it
            if fi.cheap_skip && available >= 1 {
                let msg = format!(
                    "{name} wants {wanted} soldiers, {available} ready - one \
                     soldier sent to move on"
                );
                fi.status = Some(msg.clone());
                fi.log.push((Local::now(), msg, false));
                fi.scouted_soldiers =
                    Some((1, format!("{name}: too strong - cheap skip")));
                return self.fortress_command(
                    ident,
                    FortressActionKind::Attack,
                    auto,
                );
            }
            let msg = format!(
                "Waiting: {name} needs {wanted} soldiers, {available} ready"
            );
            fi.status = Some(msg.clone());
            if !auto {
                fi.log.push((Local::now(), msg, false));
            }
            return Command::none();
        }

        fi.scouted_soldiers =
            Some((wanted, format!("{name}: game advises {advice}")));
        self.fortress_command(ident, FortressActionKind::Attack, auto)
    }

    /// Sends one fortress action (attack / new opponent / recruit) for the
    /// given character. `auto` marks it as coming from the timer, which means
    /// it silently gives up whenever the character is busy
    fn fortress_command(
        &mut self,
        ident: AccountIdent,
        action: FortressActionKind,
        auto: bool,
    ) -> Command<Message> {
        use sf_api::gamestate::fortress::FortressUnitType;

        let Some(account) = self.servers.get_account_mut(&ident) else {
            return Command::none();
        };

        let arc = account.status.clone();
        let mut status = arc.lock_safe();

        let AccountStatus::Idle(_, gs) = &*status else {
            drop(status);
            if let Some(fi) = &mut account.fortress_info {
                fi.status = Some("Character is busy".to_string());
            }
            return Command::none();
        };

        let Some(fortress) = gs.fortress.as_ref() else {
            drop(status);
            if let Some(fi) = &mut account.fortress_info {
                fi.status = Some("This character has no fortress".to_string());
            }
            return Command::none();
        };

        let unit = &fortress.units[FortressUnitType::Soldier];
        let all_soldiers = u32::from(unit.count);
        let in_training = unit.in_training;
        let barracks_full = unit.count + unit.in_training >= unit.limit;

        let (smart, cap, scouted, target) = account
            .fortress_info
            .as_ref()
            .map(|a| {
                (
                    a.smart_soldiers,
                    a.attack_soldiers,
                    a.scouted_soldiers.clone(),
                    fortress.attack_target,
                )
            })
            .unwrap_or((false, 0, None, None));

        // Only soldiers attack - archers and mages stay home as defense. How
        // many to send comes, in order: from the scouting step (the game's
        // own recommendation), the manual cap, or simply everybody
        let (soldiers, why) = match (&scouted, cap) {
            (Some((n, why)), _) => ((*n).min(all_soldiers), why.clone()),
            (None, 0) => (all_soldiers, "all soldiers".to_string()),
            (None, cap) => (
                all_soldiers.min(u32::from(cap)),
                format!("manual cap {cap}"),
            ),
        };
        // Scout first, when the smart mode is on and we have not done so.
        // A raid run always scouts: "you only ever send where the green
        // starts on the slider" - whatever the manual button is set to
        let raiding = account
            .fortress_info
            .as_ref()
            .is_some_and(|f| {
                matches!(f.raid, crate::player::RaidPhase::Raiding { .. })
            });
        let needs_scout = action == FortressActionKind::Attack
            && (smart || (auto && raiding))
            && scouted.is_none()
            && target.is_some();

        let free_reroll = fortress
            .attack_free_reroll
            .map(|a| a <= Local::now())
            .unwrap_or(false);

        // Reasons to not even send the command
        let blocked = match action {
            FortressActionKind::Attack if all_soldiers == 0 => Some(
                "No soldiers available - they need time to recover".to_string(),
            ),
            FortressActionKind::Train if in_training > 0 => {
                Some("A soldier is already being trained".to_string())
            }
            FortressActionKind::Train if barracks_full => {
                Some("The barracks are full".to_string())
            }
            _ => None,
        };

        if let Some(reason) = blocked {
            drop(status);
            if let Some(fi) = &mut account.fortress_info {
                fi.status = Some(reason);
            }
            return Command::none();
        }

        if needs_scout {
            drop(status);
            if let Some(fi) = &mut account.fortress_info {
                fi.status = Some("Looking at the opponent...".to_string());
            }
            let Some(target) = target else {
                return Command::none();
            };
            return self.send_simple(
                ident,
                "Scouting",
                sf_api::command::Command::ViewPlayer {
                    ident: target.to_string(),
                },
                SimpleNote::FortressScout { auto },
            );
        }
        // The scouting result is used exactly once
        if let Some(fi) = &mut account.fortress_info {
            fi.scouted_soldiers = None;
        }

        // A free new opponent before attacking. This used to be a checkbox,
        // which never made sense: it costs nothing, and a fresh opponent is
        // never worse than the one already standing there. `free_reroll` is
        // what keeps it from ever spending silver or mushrooms
        // Not in a raid run: there the opponent was just scouted and
        // judged, and a new one would be attacked unseen. The run takes
        // its new opponents through the skip ladder, on purpose
        let reroll_first = action == FortressActionKind::Attack
            && auto
            && free_reroll
            && !raiding;

        let Some(mut session) = status.take_session(action.busy_reason())
        else {
            drop(status);
            if let Some(fi) = &mut account.fortress_info {
                fi.status = Some("Character is busy".to_string());
            }
            return Command::none();
        };
        drop(status);

        if let Some(fi) = &mut account.fortress_info {
            fi.status = Some(match action {
                FortressActionKind::Attack => format!(
                    "Attacking with {soldiers} of {all_soldiers} soldiers ({why})"
                ),
                FortressActionKind::Reroll => {
                    "Looking for a new opponent".to_string()
                }
                FortressActionKind::PaidReroll => {
                    "Paying for a new opponent".to_string()
                }
                FortressActionKind::Train => "Recruiting a soldier".to_string(),
            });
        }

        Command::perform(
            async move {
                use sf_api::command::Command as SFCommand;
                if reroll_first {
                    let reroll = SFCommand::FortressNewEnemy {
                        use_mushroom: false,
                    };
                    // A refused re-roll is not worth aborting the attack for
                    _ = session.send_command(&reroll).await;
                    crate::pace::sleep_between(800, 2000).await;
                }
                let cmd = match action {
                    FortressActionKind::Attack => {
                        SFCommand::FortressAttack { soldiers }
                    }
                    FortressActionKind::Reroll
                    | FortressActionKind::PaidReroll => {
                        SFCommand::FortressNewEnemy {
                            use_mushroom: false,
                        }
                    }
                    FortressActionKind::Train => SFCommand::FortressBuildUnit {
                        unit: FortressUnitType::Soldier,
                        count: 1,
                    },
                };
                let resp = session.send_command(&cmd).await;
                (resp, session)
            },
            move |r| match r.0 {
                Ok(resp) => Message::FortressActionResult {
                    ident,
                    session: r.1,
                    resp: Box::new(resp),
                    action,
                },
                // The server rejecting a command (no resources, cooldown, ...)
                // leaves the session perfectly usable, so only real connection
                // problems trigger a relog
                Err(sf_api::error::SFError::ServerError(e)) => {
                    Message::FortressActionFailed {
                        ident,
                        session: r.1,
                        action,
                        error: e,
                    }
                }
                Err(_) => Message::PlayerCommandFailed {
                    ident,
                    session: r.1,
                    attempt: 0,
                },
            },
        )
    }
}

/// Enough of the last fight to tell one from another. `Fight` has no
/// equality of its own, and all that is needed here is "is this the same
/// fight the game was already showing before the command"
fn fight_fingerprint(
    gs: &sf_api::gamestate::GameState,
) -> Option<(bool, i64, u64, i64, u32, usize)> {
    let f = gs.last_fight.as_ref()?;
    Some((
        f.has_player_won,
        f.silver_change,
        f.xp_change,
        f.honor_change,
        f.rank_post_fight,
        f.fights.len(),
    ))
}

/// Shows the user, what auto battle is currently doing/waiting for
fn set_auto_status(account: &mut AccountInfo, msg: impl Into<String>) {
    if let Some(si) = &mut account.scrapbook_info {
        si.auto_status = Some(msg.into());
    }
}

/// 53_516_000 -> "53.5 M"
/// How many purchases the arena manager gets in one dispatch before it hands
/// the slot back. One is a standstill, ten holds the character for minutes
/// with human-like pauses in between - a short burst is the middle ground
const IDLE_BURST: u8 = 10;

/// Pick a real affordable bundle, including the cheap first levels after a reset.
fn idle_path_bundle(money: f64, need: u32, costs: [f64; 4]) -> Option<(sf_api::command::IdleUpgradeAmount, u32, f64)> {
    use sf_api::command::IdleUpgradeAmount as A;
    if !money.is_finite() || money <= 0.0 { return None; }
    for (amount, count, cost) in [(A::Hundred,100,costs[3]), (A::TwentyFive,25,costs[2]), (A::Ten,10,costs[1]), (A::One,1,costs[0])] {
        if cost.is_finite() && cost > 0.0 && cost <= money && (count <= need || cost * 50.0 <= money) {
            return Some((amount, count, cost));
        }
    }
    None
}

/// Yield through the existing scheduler without losing the rest of the buying run.
fn idle_requeue_after_burst(queue: &mut Vec<(Job, DateTime<Local>)>, bought: u8, enabled: bool, now: DateTime<Local>) -> bool {
    let should_yield = bought >= 25 || (bought >= IDLE_BURST && !queue.is_empty());
    if !should_yield { return false; }
    if enabled && !queue.iter().any(|(job, _)| *job == Job::ArenaManager) {
        queue.push((Job::ArenaManager, now));
    }
    true
}

/// The optimal Arena Manager build order, in the guild leader's own words
/// (the breakpoint table). Every building has breakpoints at 25, 50, 100,
/// 250, 500, 1000, 2500 (and on up); at each one the building's earnings
/// double and its cycle shortens, so *which* building to push next is what
/// decides how fast the money grows. This walks each building up to its next
/// listed breakpoint in exactly this order, then hands over to plain
/// return-on-investment for anything past the end of the table.
///
/// Returns the next building to upgrade and how many levels are still needed
/// to reach its current breakpoint, or `None` once the whole path is walked.
fn idle_next_on_path(
    idle: &sf_api::gamestate::idle::IdleGame,
) -> Option<(sf_api::gamestate::idle::IdleBuildingType, u32)> {
    use sf_api::gamestate::idle::IdleBuildingType as B;
    // Screen names -> library names: Seats=Seat, Popcorn=PopcornStand,
    // Parking=ParkingLot, Drink=Drinks, Deadly=DeadlyTrap, VIP=VIPSeat,
    // Snack=Snacks, Monster=StrayingMonsters
    const PATH: [(B, u32); 70] = [
        // column 1
        (B::Seat, 25),
        (B::Seat, 50),
        (B::PopcornStand, 25),
        (B::Seat, 100),
        (B::PopcornStand, 50),
        (B::PopcornStand, 100),
        (B::ParkingLot, 25),
        (B::ParkingLot, 50),
        (B::Seat, 250),
        (B::ParkingLot, 100),
        (B::Trap, 25),
        (B::PopcornStand, 250),
        (B::Trap, 50),
        (B::Trap, 100),
        (B::Drinks, 25),
        (B::ParkingLot, 250),
        (B::Drinks, 50),
        (B::Drinks, 100),
        (B::DeadlyTrap, 25),
        (B::Seat, 500),
        (B::Trap, 250),
        (B::DeadlyTrap, 50),
        (B::PopcornStand, 500),
        (B::DeadlyTrap, 100),
        // column 2
        (B::VIPSeat, 25),
        (B::Drinks, 250),
        (B::VIPSeat, 50),
        (B::ParkingLot, 500),
        (B::VIPSeat, 100),
        (B::Snacks, 25),
        (B::DeadlyTrap, 250),
        (B::Snacks, 50),
        (B::Trap, 500),
        (B::Snacks, 100),
        (B::StrayingMonsters, 25),
        (B::VIPSeat, 250),
        (B::StrayingMonsters, 50),
        (B::Drinks, 500),
        (B::StrayingMonsters, 100),
        (B::Toilet, 25),
        (B::Snacks, 250),
        (B::Toilet, 50),
        (B::DeadlyTrap, 500),
        (B::Toilet, 100),
        (B::Seat, 1000),
        (B::StrayingMonsters, 250),
        (B::PopcornStand, 1000),
        (B::VIPSeat, 500),
        // column 3
        (B::Toilet, 250),
        (B::ParkingLot, 1000),
        (B::Snacks, 500),
        (B::Trap, 1000),
        (B::StrayingMonsters, 500),
        (B::Drinks, 1000),
        (B::Toilet, 500),
        (B::DeadlyTrap, 1000),
        (B::VIPSeat, 1000),
        (B::Snacks, 1000),
        (B::StrayingMonsters, 1000),
        (B::Toilet, 1000),
        (B::Seat, 2500),
        (B::PopcornStand, 2500),
        (B::ParkingLot, 2500),
        (B::Trap, 2500),
        (B::Drinks, 2500),
        (B::DeadlyTrap, 2500),
        (B::VIPSeat, 2500),
        (B::Snacks, 2500),
        (B::StrayingMonsters, 2500),
        (B::Toilet, 2500),
    ];
    for &(typ, breakpoint) in PATH.iter() {
        let level = idle.buildings[typ].level;
        if level < breakpoint {
            return Some((typ, breakpoint - level));
        }
    }
    None
}

/// Which player to lure: the best target that was crawled today. `ui.best`
/// is already ordered best-first and already limited to players at or below
/// the lure level, so the first fresh one is the pick. `None` means nothing
/// is fresh yet - the caller re-crawls rather than luring stale data.
///
/// This replaced a gate that waited for 90% of the ~100 targets to be fresh,
/// which on a live server never happens: the crawler cycles the whole Hall
/// of Fame, so the first targets go stale again before the last are reached.
/// The job spun 200 times a session without ever attacking. One fresh
/// beatable target is all a lure ever needed.
#[must_use]
/// Which crawled character to drag into the underworld.
///
/// **The game says what level is worth luring.** `Underworld::lure_level`
/// is "the level of characters you need to lure to get the full reward" -
/// it rises with the Keeper and with what has been built down there, and
/// anybody at or above it pays out the same full pile of souls. Below it
/// the souls are docked.
///
/// So the pick is the *lowest* level that still clears the bar: the full
/// reward, off the weakest opponent that pays it, which is the one most
/// likely to go down. Taking the strongest fresh name on the list instead
/// buys nothing - the souls are capped either way - and loses more
/// fights. With nobody at the bar yet, the highest level crawled is the
/// closest to a full payout there is.
///
/// Only characters looked at today count: an old crawl is a level and a
/// set of gear from days ago.
fn pick_lure_target(
    best: &[crate::CharacterInfo],
    wanted_level: u16,
) -> Option<crate::CharacterInfo> {
    pick_lure_target_capped(best, wanted_level, None)
}

/// `pick_lure_target` with a ceiling the opponent should stay under: the
/// Keeper's level less a margin (5.4). A lure that is lost pays nothing,
/// and a day ends with lures unused when every pick is a coin toss. So
/// when the best-paying target is above the ceiling, the strongest one
/// under it is taken instead - a smaller reward that is actually won
fn pick_lure_target_capped(
    best: &[crate::CharacterInfo],
    wanted_level: u16,
    ceiling: Option<u16>,
) -> Option<crate::CharacterInfo> {
    let fresh = || best.iter().filter(|a| !a.is_old());
    let pick = fresh()
        .filter(|a| a.level >= wanted_level)
        .min_by_key(|a| a.level)
        .or_else(|| fresh().max_by_key(|a| a.level));
    match (pick, ceiling) {
        (Some(p), Some(cap)) if p.level > cap => fresh()
            .filter(|a| a.level <= cap)
            .max_by_key(|a| a.level)
            .or(Some(p))
            .cloned(),
        (p, _) => p.cloned(),
    }
}

/// The strongest pet of a habitat gets that habitat's fruit first, because
/// he is the one on his way to level 100. A weaker pet may only have what is
/// truly spare: a pet eats at most three fruits a day, so anything past the
/// next day's three for the strongest is fruit that would otherwise rot in
/// the habitat unused. `spread` overrides this for someone who would rather
/// raise the whole habitat than one pet, and then every fruit is fair game.
#[must_use]
fn other_pet_may_eat(fruit_left: u16, spread: bool) -> bool {
    /// A pet's daily limit, held back for the strongest before the rest is
    /// shared out
    const KEEP_FOR_BEST: u16 = 3;
    spread || fruit_left > KEEP_FOR_BEST
}

#[cfg(test)]
mod reroll_tests {
    use super::MAX_FREE_REROLLS;

    /// The rule the arena re-roll follows, on its own: throw the trio back
    /// while there are tries left and the game is actually changing it.
    /// The re-roll is free, so the only two things that may stop it are
    /// the try count and the game handing back the same three
    fn should_reroll(used: u8, ids: &[u32], seen: &[u32]) -> bool {
        used < MAX_FREE_REROLLS && ids != seen && !ids.is_empty()
    }

    #[test]
    fn a_trio_nobody_can_beat_is_thrown_back_but_not_for_ever() {
        let a = [1u32, 2, 3];
        let b = [4u32, 5, 6];
        // Fresh three, no tries used: throw them back
        assert!(should_reroll(0, &a, &[]));
        // The game changed them, one try used: again
        assert!(should_reroll(1, &b, &a));
        // Out of tries
        assert!(!should_reroll(MAX_FREE_REROLLS, &b, &a));
        // The game handed back the same three - that is its answer
        assert!(!should_reroll(1, &a, &a));
        // Nothing fetched yet is not something to re-roll
        assert!(!should_reroll(0, &[], &[]));
    }
}

#[cfg(test)]
mod lure_tests {
    use super::pick_lure_target;
    use crate::CharacterInfo;

    /// The regression this guards: an empty crawl gives no target, and the
    /// caller must fall through to a re-crawl instead of attacking nothing
    #[test]
    fn nothing_crawled_yet_means_no_target() {
        assert!(pick_lure_target(&[], 0).is_none());
    }

    /// One fresh target is enough - the old code needed 90% of the whole
    /// crawled list to have been looked at today, which on a few thousand
    /// names never happens, so nothing was ever lured. A single fresh
    /// entry must produce a pick, and a lone stale one must not
    #[test]
    fn one_fresh_target_is_enough_to_lure() {
        assert!(
            pick_lure_target(&[CharacterInfo::test_stub(true)], 0).is_some()
        );
        assert!(
            pick_lure_target(&[CharacterInfo::test_stub(false)], 0).is_none()
        );
    }

    /// The souls are capped at the game's own `lure_level`, so the pick is
    /// the weakest opponent that still clears it - not the strongest name
    /// on the list, which pays exactly the same and wins more often
    #[test]
    fn the_weakest_full_reward_target_wins() {
        let at = |l| CharacterInfo::test_stub_at(true, l);
        let list = vec![at(120), at(260), at(215), at(190), at(400)];
        assert_eq!(pick_lure_target(&list, 213).unwrap().level, 215);
        // Exactly on the bar is already the full reward
        assert_eq!(pick_lure_target(&list, 215).unwrap().level, 215);
        // Nobody reaches it: the closest to a full payout is the highest
        assert_eq!(pick_lure_target(&list, 900).unwrap().level, 400);
        // A stale name is never picked, however well it would pay
        let mut stale = vec![CharacterInfo::test_stub_at(false, 250)];
        assert!(pick_lure_target(&stale, 213).is_none());
        stale.push(at(300));
        assert_eq!(pick_lure_target(&stale, 213).unwrap().level, 300);
    }

    /// 5.4: above the Keeper's ceiling the strongest target under it wins;
    /// with nobody under it the old pick stands
    #[test]
    fn the_keeper_ceiling_trades_reward_for_a_win() {
        use super::pick_lure_target_capped;
        let at = |l| CharacterInfo::test_stub_at(true, l);
        let list = vec![at(150), at(180), at(215), at(400)];
        assert_eq!(
            pick_lure_target_capped(&list, 213, Some(190)).unwrap().level,
            180
        );
        assert_eq!(
            pick_lure_target_capped(&list, 213, Some(300)).unwrap().level,
            215
        );
        assert_eq!(
            pick_lure_target_capped(&[at(400)], 213, Some(100))
                .unwrap()
                .level,
            400
        );
    }
}

#[cfg(test)]
mod fruit_tests {
    use super::other_pet_may_eat;

    /// The strongest pet is never gated by this - he is `order[0]` and the
    /// loop only consults the rule once it is `past_the_best`. So the rule is
    /// purely "may a *weaker* pet eat now", and the answer is no until the
    /// strongest's three-a-day is safe
    #[test]
    fn a_weaker_pet_only_eats_the_real_surplus() {
        // Three or fewer left: that is the strongest's ration, hands off
        assert!(!other_pet_may_eat(3, false));
        assert!(!other_pet_may_eat(1, false));
        assert!(!other_pet_may_eat(0, false));
        // Four or more: one day's ration is safe, the rest is shared
        assert!(other_pet_may_eat(4, false));
        assert!(other_pet_may_eat(9, false));
    }

    /// Spread mode is the whole point of the flag: level the habitat, not one
    /// pet, so even the last fruit may go to a weaker one
    #[test]
    fn spread_mode_shares_down_to_the_last_fruit() {
        assert!(other_pet_may_eat(1, true));
        assert!(other_pet_may_eat(0, true));
    }
}

fn human_money(v: f64) -> String {
    const UNITS: [(&str, f64); 5] =
        [("Q", 1e15), ("T", 1e12), ("B", 1e9), ("M", 1e6), ("K", 1e3)];
    for (name, size) in UNITS {
        if v >= size {
            return format!("{:.1} {name}", v / size);
        }
    }
    format!("{v:.0}")
}

/// Whether the automation behind a queued job is (still) switched on
pub fn account_job_enabled(a: &AccountInfo, job: Job) -> bool {
    match job {
        Job::FortressCollect => {
            a.fortress_info.as_ref().is_some_and(|f| f.maintains())
        }
        Job::FortressAttack => {
            a.fortress_info.as_ref().is_some_and(|f| f.auto_attack)
        }
        Job::Expedition => a.expedition_info.auto,
        Job::Arena => a.scrapbook_info.as_ref().is_some_and(|s| s.auto_battle),
        Job::Dungeon => a.dungeon_info.auto,
        Job::GuildSign => {
            a.social.auto_attack || a.social.auto_defense || a.social.raid_auto
        }
        Job::Pets => a.pet_info.auto_feed || a.pet_info.auto_arena,
        Job::ArenaManager => {
            a.idle_info.auto_upgrade
                || a.idle_info.auto_collect
                || a.idle_info.auto_sacrifice
        }
        Job::Lure => a.underworld_info.as_ref().is_some_and(|u| u.auto_lure),
        Job::Underworld => a
            .underworld_info
            .as_ref()
            .is_some_and(|u| u.auto_build || u.auto_collect),
        // A shift that is running has to be finished whatever the switch
        // says - the wage of a shift started by hand, with the automation
        // off, sat uncollected because the job was dropped right here
        Job::Guard => {
            a.guard_info.auto
                || matches!(
                    &*a.status.lock_safe(),
                    AccountStatus::Idle(_, gs)
                        if matches!(
                            gs.tavern.current_action,
                            sf_api::gamestate::tavern::CurrentAction::CityGuard { .. }
                        )
                )
        }
        Job::Wheel => a.tavern_info.wheel_auto,
        // Gear buying is off by default, but dressing the companions and
        // keeping the bag clear are not - the job has to run for those too
        Job::Shopping => {
            a.shop_info.auto
                || a.shop_info.equip_companions
                || a.shop_info.clear_backpack
        }
        Job::Attributes => a.shop_info.attr_auto,
        Job::Dailies => a.dailies_info.auto,
        Job::Dice => a.tavern_info.dice_auto,
    }
}

/// Wood and stone lying in the fortress right now
fn fortress_stock(gs: &sf_api::gamestate::GameState) -> (u64, u64) {
    use sf_api::gamestate::fortress::FortressResourceType;
    gs.fortress
        .as_ref()
        .map(|f| {
            (
                f.resources[FortressResourceType::Wood].current,
                f.resources[FortressResourceType::Stone].current,
            )
        })
        .unwrap_or((0, 0))
}

/// What to keep off the table: the symbol with the most dice on it. Three of
/// a kind wins the game, so the count decides - and when two symbols are
/// level, the one worth more does. An hourglass skips a wait, arcane and
/// souls are the slow currencies you cannot farm at will, while silver,
/// wood and stone come in from the fortress anyway
fn most_common_dice(
    dice: &[sf_api::command::DiceType],
) -> Option<sf_api::command::DiceType> {
    use sf_api::command::DiceType;
    // The event's EPIC symbol beats everything, whatever the counts: one
    // epic on the table is kept and the rest re-rolled for more of them -
    // an epic item is worth more than any pile of stone
    if dice.contains(&DiceType::Epic) {
        return Some(DiceType::Epic);
    }
    let worth = |d: DiceType| match d {
        DiceType::Epic => 10,
        DiceType::Hourglass => 5,
        DiceType::Arcane => 4,
        DiceType::Souls => 3,
        DiceType::Stone => 2,
        DiceType::Wood => 1,
        DiceType::Silver | DiceType::ReRoll => 0,
    };
    let mut best: Option<(DiceType, usize)> = None;
    for d in dice {
        if *d == DiceType::ReRoll {
            continue;
        }
        let n = dice.iter().filter(|o| *o == d).count();
        let better = match best {
            None => true,
            Some((bd, bn)) => n > bn || (n == bn && worth(*d) > worth(bd)),
        };
        if better {
            best = Some((*d, n));
        }
    }
    best.map(|(d, _)| d)
}

#[cfg(test)]
mod idle_tests {
    use super::*;
    use sf_api::command::IdleUpgradeAmount as A;

    #[test]
    fn cheap_post_reset_levels_do_not_wait_for_ten_thousand() {
        assert_eq!(idle_path_bundle(26.0, 1, [26.0, 300.0, 900.0, 9000.0]), Some((A::One, 1, 26.0)));
        assert_eq!(idle_path_bundle(100.0, 25, [5.0, 55.0, 180.0, 9000.0]), Some((A::Ten, 10, 55.0)));
    }
    #[test]
    fn path_respects_breakpoints_but_bundles_trivial_catch_up_costs() {
        assert_eq!(idle_path_bundle(1000.0, 1, [100.0, 500.0, 800.0, 900.0]), Some((A::One, 1, 100.0)));
        assert_eq!(idle_path_bundle(1000000.0, 1, [5.0, 55.0, 180.0, 9000.0]), Some((A::Hundred, 100, 9000.0)));
    }
    #[test]
    fn missing_prices_and_invalid_balances_never_buy() {
        for money in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert!(idle_path_bundle(money, 100, [1.0;4]).is_none());
        }
        assert!(idle_path_bundle(100.0, 100, [0.0, f64::INFINITY, f64::NAN, 101.0]).is_none());
    }
    #[test]
    fn yielding_preserves_other_jobs_and_queues_only_one_continuation() {
        let now = Local::now();
        let mut queue = vec![(Job::Arena, now), (Job::Dungeon, now)];
        assert!(!idle_requeue_after_burst(&mut queue, 9, true, now));
        assert_eq!(queue.len(), 2);
        assert!(idle_requeue_after_burst(&mut queue, 10, true, now));
        assert_eq!(queue.iter().map(|(j,_)| *j).collect::<Vec<_>>(), vec![Job::Arena, Job::Dungeon, Job::ArenaManager]);
        assert!(idle_requeue_after_burst(&mut queue, 10, true, now));
        assert_eq!(queue.len(), 3);
    }
    #[test]
    fn hard_limit_resumes_but_disabled_automation_does_not() {
        let now = Local::now();
        let mut queue = Vec::new();
        assert!(idle_requeue_after_burst(&mut queue, 25, false, now));
        assert!(queue.is_empty());
        assert!(idle_requeue_after_burst(&mut queue, 25, true, now));
        assert_eq!(queue[0].0, Job::ArenaManager);
    }
}

#[cfg(test)]
mod dice_tests {
    use sf_api::command::DiceType::{self, *};

    #[test]
    fn the_symbol_we_keep_is_the_one_with_the_most_dice() {
        assert_eq!(
            super::most_common_dice(&[Silver, Stone, Silver, Wood, Silver]),
            Some(Silver)
        );
        assert_eq!(super::most_common_dice(&[Souls]), Some(Souls));
    }

    #[test]
    fn a_tie_goes_to_the_more_valuable_symbol() {
        // Two of each: wood is fortress filler, arcane is not
        assert_eq!(
            super::most_common_dice(&[Wood, Arcane, Wood, Arcane, Silver]),
            Some(Arcane)
        );
        // ...but the count still comes first
        assert_eq!(
            super::most_common_dice(&[Wood, Wood, Wood, Arcane, Arcane]),
            Some(Wood)
        );
    }

    /// One EPIC die is kept even against three of something else
    #[test]
    fn an_epic_die_is_always_the_one_to_keep() {
        assert_eq!(
            super::most_common_dice(&[Stone, Stone, Stone, Epic, Wood]),
            Some(Epic)
        );
        assert_eq!(
            super::most_common_dice(&[Epic, Epic, Silver, Silver, Silver]),
            Some(Epic)
        );
    }

    #[test]
    fn an_empty_table_keeps_nothing() {
        assert_eq!(super::most_common_dice(&[]), None);
        let empty: [DiceType; 5] = [ReRoll; 5];
        assert_eq!(super::most_common_dice(&empty), None);
    }
}
