//! Read-only fixtures for visual review. This mode creates no game session,
//! schedules no background work and never passes actions to the live handler.
use crate::{
    AccountID, AccountIdent, AccountPage, Helper, View,
    login::PlayerAuth,
    message::Message,
    player::{AccountInfo, AccountStatus, FortressInfo, UnderworldInfo},
    server::ServerIdent,
};
use chrono::Local;
use iced::Command;
use sf_api::{
    gamestate::{
        GameState, character::Class, fortress::Fortress,
        underworld::Underworld, unlockables::Pets,
    },
    session::ServerConnection,
};

pub fn populate(helper: &mut Helper) {
    helper.config.lang = crate::i18n::Lang::Cs;
    crate::i18n::set_lang(helper.config.lang);
    let server = ServerIdent::new("https://preview.invalid/");
    let ident = AccountIdent {
        server_id: server.id,
        account: AccountID(1),
    };
    if std::env::var("SFC_PREVIEW_PAGE").as_deref() == Ok("options") {
        helper.config.accounts.push(crate::config::AccountConfig::Regular {
        name: "Commander".into(),
        pw_hash: sf_api::session::PWHash::new("preview-only"),
        server: "https://preview.invalid/".into(),
        config: Default::default(),
        });
    }
    let mut gs = GameState::default();
    gs.character.name = "Commander".into();
    gs.character.class = Class::Warrior;
    gs.character.level = 213;
    // Nine digits of gold, which is what did not fit on the purse card
    gs.character.silver = 9_845_954_000;
    gs.character.mushrooms = 147;
    gs.character.experience = 1_720_000;
    gs.character.next_level_xp = 2_450_000;
    gs.character.rank = 842;
    gs.character.honor = 34_270;
    for (_, value) in gs.character.attribute_basis.iter_mut() {
        *value = 1800;
    }
    gs.tavern.thirst_for_adventure_sec = 6840;
    gs.tavern.beer_max = 10;
    gs.tavern.beer_drunk = 3;
    gs.tavern.quicksand_glasses = 86;
    // Busy on the city guard, so the "currently doing" plate has the one
    // state that did not fit on it to show
    gs.tavern.current_action =
        sf_api::gamestate::tavern::CurrentAction::CityGuard {
            busy_until: Local::now() + chrono::Duration::hours(7),
            hours: 10,
        };
    gs.tavern.expeditions.start =
        Some(Local::now() - chrono::Duration::days(1));
    gs.tavern.expeditions.end = Some(Local::now() + chrono::Duration::days(1));
    let mut pets = Pets {
        max_pet_level: 200,
        rank: 1264,
        ..Default::default()
    };
    for (index, (element, habitat)) in pets.habitats.iter_mut().enumerate() {
        let owned = [14, 11, 16, 13, 17][index];
        pets.total_collected += owned as u16;
        habitat.fruits = [8, 12, 4, 19, 7][index];
        for (i, pet) in habitat.pets.iter_mut().enumerate() {
            pet.id = (index * 20 + i + 1) as u32;
            pet.element = element;
            // Rising with the number, the way a collection grows - and
            // ending near the next boss's level, so the simulator has
            // close fights to show and not only sure ones
            pet.level = if i < owned { (6 + i * 4) as u16 } else { 0 };
        }
        // The next boss is the one after the last pet won - so the
        // simulator has something to fight
        habitat.exploration =
            sf_api::gamestate::unlockables::HabitatExploration::Exploring {
                fights_won: owned as u32,
                next_fight_lvl: crate::pet_sim::BOSS_LEVEL[owned],
            };
    }
    gs.pets = Some(pets);
    let mut fortress = Fortress {
        building_max_lvl: 20,
        ..Default::default()
    };
    for (_, building) in fortress.buildings.iter_mut() {
        building.level = 12;
    }
    for (_, unit) in fortress.units.iter_mut() {
        unit.level = 85;
        unit.count = 24;
        unit.limit = 30;
    }
    // Three stores at three different speeds, so the harvest panel has
    // something to pick a winner out of: stone fills in four hours, wood in
    // six, experience in fifteen
    for (i, (_, resource)) in fortress.resources.iter_mut().enumerate() {
        resource.current = 145_000;
        resource.limit = 220_000;
        resource.production.per_hour = [3_000, 4_500, 1_200][i % 3];
        resource.production.limit = [18_000, 18_000, 18_000][i % 3];
        resource.production.last_collectable = [9_400, 16_100, 2_300][i % 3];
    }
    fortress.last_collectable_updated = Some(Local::now());
    gs.fortress = Some(fortress);
    let mut underworld = Underworld {
        souls_current: 64_250,
        souls_limit: 125_000,
        lure_level: 213,
        lured_today: 2,
        ..Default::default()
    };
    for (_, building) in underworld.buildings.iter_mut() {
        building.level = 8;
    }
    {
        use sf_api::gamestate::underworld::UnderworldResourceType as UR;
        let souls = &mut underworld.production[UR::Souls];
        souls.per_hour = 640;
        souls.limit = 9_600;
        souls.last_collectable = 5_100;
        let gold = &mut underworld.production[UR::Silver];
        gold.per_hour = 120_000;
        gold.limit = 1_440_000;
        gold.last_collectable = 400_000;
        underworld.last_collectable_update = Some(Local::now());
    }
    gs.underworld = Some(underworld);
    // Real offers on the shelves, so the shop rows can be looked at: a long
    // item line in a narrow lane is what wrapped each one into a paragraph,
    // and an empty shop hides that entirely
    {
        use sf_api::{
            command::{AttributeType, ShopType},
            gamestate::items::ItemType,
        };
        gs.character.equipment.0
            [sf_api::gamestate::items::EquipmentSlot::Hat] = Some({
            let mut worn = gs.shops[ShopType::Weapon].items[0].clone();
            worn.typ = ItemType::Hat;
            worn.class = Some(Class::Warrior);
            worn.item_quality = 248;
            worn.upgrade_count = 4;
            worn.attributes[AttributeType::Strength] = 590;
            worn.attributes[AttributeType::Constitution] = 480;
            worn.attributes[AttributeType::Luck] = 310;
            worn
        });
        for (shop, kinds) in [
            (
                ShopType::Weapon,
                [
                    ItemType::Hat,
                    ItemType::BreastPlate,
                    ItemType::Gloves,
                    ItemType::FootWear,
                    ItemType::Amulet,
                    ItemType::Belt,
                ],
            ),
            (
                ShopType::Magic,
                [
                    ItemType::Ring,
                    ItemType::Talisman,
                    ItemType::Hat,
                    ItemType::Gloves,
                    ItemType::Belt,
                    ItemType::FootWear,
                ],
            ),
        ] {
            // The default shop calls itself the magic shop whichever it
            // is - the real one is told by the server. Two shelves both
            // headed "Magic shop" is the fixture, not the page
            gs.shops[shop].typ = shop;
            for (i, kind) in kinds.into_iter().enumerate() {
                let it = &mut gs.shops[shop].items[i];
                it.typ = kind;
                it.class = Some(Class::Warrior);
                it.item_quality = 250 + i as u32 * 3;
                it.price = (74_000 + i as u32 * 9_100) * 100;
                it.mushroom_price = 0;
                it.attributes[AttributeType::Strength] =
                    (600 + i as u32 * 21) as u32;
                it.attributes[AttributeType::Constitution] =
                    (470 + i as u32 * 17) as u32;
                it.attributes[AttributeType::Luck] =
                    (300 + i as u32 * 9) as u32;
            }
        }
    }
    // The three companions, dressed, so the shop's four columns all have
    // figures in them - with no companions the three are dots and the
    // page cannot be judged
    {
        use sf_api::{
            command::AttributeType, gamestate::dungeons::CompanionClass,
        };
        // The map type is not re-exported, so it is reached through a
        // value the state already holds
        let mut companions = gs.dungeons.companions.take().unwrap_or_default();
        let _ = CompanionClass::Warrior;
        for (i, (_, c)) in companions.iter_mut().enumerate() {
            c.level = 213;
            for (_, v) in c.attributes.iter_mut() {
                *v = 1500 + i as u32 * 120;
            }
            for slot in [
                sf_api::gamestate::items::EquipmentSlot::Hat,
                sf_api::gamestate::items::EquipmentSlot::Gloves,
                sf_api::gamestate::items::EquipmentSlot::FootWear,
                sf_api::gamestate::items::EquipmentSlot::Belt,
                sf_api::gamestate::items::EquipmentSlot::Amulet,
                sf_api::gamestate::items::EquipmentSlot::Ring,
            ] {
                let mut worn = gs.shops[sf_api::command::ShopType::Weapon]
                    .items[0]
                    .clone();
                worn.typ = match slot {
                    sf_api::gamestate::items::EquipmentSlot::Hat => {
                        sf_api::gamestate::items::ItemType::Hat
                    }
                    sf_api::gamestate::items::EquipmentSlot::Gloves => {
                        sf_api::gamestate::items::ItemType::Gloves
                    }
                    sf_api::gamestate::items::EquipmentSlot::FootWear => {
                        sf_api::gamestate::items::ItemType::FootWear
                    }
                    sf_api::gamestate::items::EquipmentSlot::Belt => {
                        sf_api::gamestate::items::ItemType::Belt
                    }
                    sf_api::gamestate::items::EquipmentSlot::Amulet => {
                        sf_api::gamestate::items::ItemType::Amulet
                    }
                    _ => sf_api::gamestate::items::ItemType::Ring,
                };
                worn.class = None;
                worn.item_quality = 240 + i as u32 * 5;
                worn.attributes[AttributeType::Strength] = 560 + i as u32 * 40;
                worn.attributes[AttributeType::Constitution] = 500;
                worn.attributes[AttributeType::Luck] = 300;
                c.equipment.0[slot] = Some(worn);
            }
        }
        gs.dungeons.companions = Some(companions);
    }
    gs.idle_game = Some(idle_game());
    gs.guild = Some(guild());
    let mut account = AccountInfo::new("Commander", PlayerAuth::SSO, ident);
    account.fortress_info = FortressInfo::new(&gs, None);
    account.underworld_info = UnderworldInfo::new(&gs, None);
    account.pet_info.auto_feed = true;
    account.expedition_info.auto = true;
    *account.status.lock().unwrap() =
        AccountStatus::Busy(Box::new(gs), "Offline preview".into());
    // Constructing a client does not send a request. No Session exists here.
    let connection = ServerConnection::new("https://preview.invalid/")
        .expect("static preview URL");
    helper
        .servers
        .get_or_insert_default(server, connection, None)
        .accounts
        .insert(ident.account, account);
    // A feed with lines in it. The console is the one part of the screen
    // that is deliberately not set in the artwork - it keeps the system
    // face at its heaviest, in the colours the lines mean - and an empty
    // panel is no way to check that
    for (mins, who, text, ok) in [
        (
            23, "Commander",
            "Thinking: tavern first - 114 thirst and a free beer", None,
        ),
        (
            22,
            "Commander",
            "Expedition started: Toilet paper (3 clearings)",
            Some(true),
        ),
        (
            19,
            "Ravenna",
            "Arena: beat Kunigunde, +2140 honour",
            Some(true),
        ),
        (
            17, "Commander", "Shop reroll: nothing worth the mushroom", None,
        ),
        (
            14,
            "Commander",
            "Fortress: wood store full, collected 9400",
            Some(true),
        ),
        (
            11,
            "Tycho",
            "Dungeon 14: lost to the Gatekeeper",
            Some(false),
        ),
        (
            9, "Commander",
            "Thinking: underworld lure is cheaper than the arena run", None,
        ),
        (6, "Commander", "Gem mine: digging until 21:40", Some(true)),
        (
            3,
            "Belinka",
            "Guild raid: signed up for the attack",
            Some(true),
        ),
        (
            1,
            "Commander",
            "Pets: fed 5 habitats, fruit ran out on Shadow",
            Some(false),
        ),
    ] {
        helper.console.push_back(crate::ConsoleLine {
            at: chrono::Local::now() - chrono::Duration::minutes(mins),
            who: who.into(),
            text: text.into(),
            ok,
            dup: false,
        });
    }
    for (mins, who, text) in [
        (
            22, "Commander", "Expedition started: Toilet paper (3 clearings)",
        ),
        (19, "Ravenna", "Arena: beat Kunigunde, +2140 honour"),
        (14, "Commander", "Fortress: wood store full, collected 9400"),
        (6, "Commander", "Gem mine: digging until 21:40"),
    ] {
        helper.successes.push_back(crate::ConsoleLine {
            at: chrono::Local::now() - chrono::Duration::minutes(mins),
            who: who.into(),
            text: text.into(),
            ok: Some(true),
            dup: false,
        });
    }

    // The drawn lettering is on unless the harness says otherwise. Every
    // width on a page is measured differently with it off, so a
    // screenshot of one says nothing about the other
    if std::env::var("SFC_PREVIEW_FONT").as_deref() == Ok("off") {
        helper.config.drawn_font = false;
        crate::ui::glyphs::set_artwork(false);
    }

    // A diary with something in it. Without one every member's time in the
    // guild is a question mark, which is exactly the state the column was
    // in before it existed - a screenshot of that says nothing about
    // whether the column works
    seed_guild_diary(helper);
    // The chat tab, with the archive switched on and a path to show
    if std::env::var("SFC_PREVIEW_PAGE").as_deref() == Ok("guild-chat")
        && let Some(server) = helper.servers.0.values_mut().next()
        && let Some(account) = server.accounts.values_mut().next()
    {
        account.social.show_chat = true;
        helper.config.chat_archive = true;
        helper.chat_archive_note = Some(
            r"C:\Users\jirim\Desktop\SF Commander - chat Imperium.txt".into(),
        );
        // Older lines, as they come back out of the file - level ups among
        // the talk, the way a real day of Imperium reads
        account.social.show_archive = true;
        account.social.archive_lines = [
            "[2026-09-17 21:04] Zaiyfu: raid zitra v 19?",
            "[2026-09-17 21:06] Petr Macinka: jo, prihlaste se vsichni",
            "[2026-09-17] Hrac KAMSON87 dosahl urovne 206.",
            "[2026-09-17 22:15] Nyx: HoK 10 mam",
            "[2026-09-18] Hrac Bohdan dosahl urovne 186.",
            "[2026-09-18] Hrac Mentol dosahl urovne 231.",
            "[2026-09-18 08:40] Zaiyfu: kdo ma jeste volnou ruku na pet?",
            "[2026-09-18] Hrac Lilith dosahl urovne 198.",
            "[2026-09-18 19:02] Petr Macinka: raid jde, 43 %",
            "[2026-09-18] Hrac Zaiyfu dosahl urovne 249.",
        ]
        .into_iter()
        .map(String::from)
        .collect();
    }
    if std::env::var("SFC_PREVIEW_PAGE").as_deref() == Ok("guild-open")
        && let Some(server) = helper.servers.0.values_mut().next()
        && let Some(account) = server.accounts.values_mut().next()
    {
        account.social.drawer_open = true;
    }

    // No UI tick runs in the preview, so the pet fights are worked out
    // here once
    helper.refresh_pet_sim(ident);

    helper.current_view = match std::env::var("SFC_PREVIEW_PAGE")
        .unwrap_or_default()
        .as_str()
    {
        "overview" => View::Overview {
            selected: Default::default(),
            action: None,
        },
        // The door, with a saved character to show the list and an error
        // line to show where one lands. "login-empty" is the first start:
        // nothing saved, nothing to go back to
        s if s.starts_with("login") => {
            if s != "login-empty" {
                helper
                    .config
                    .accounts
                    .push(crate::config::AccountConfig::SF {
                        name: "PetrMacinka".into(),
                        pw_hash: sf_api::session::PWHash::new("x"),
                        characters: Vec::new(),
                    });
                helper.config.accounts.push(
                    crate::config::AccountConfig::Regular {
                        name: "Gussolean".into(),
                        pw_hash: sf_api::session::PWHash::new("x"),
                        server: "https://s30.sfgame.eu".into(),
                        config: Default::default(),
                    },
                );
                helper.login_state.name = "PetrMacinka".into();
                helper.login_state.password = "hunter2hunter".into();
                helper.login_state.error =
                    Some("Could not communicate with the server".into());
            }
            helper.login_state.login_typ = match s {
                "login-saved" => crate::login::LoginType::Saved,
                "login-server" => crate::login::LoginType::Regular,
                "login-steam" => crate::login::LoginType::Steam,
                _ => crate::login::LoginType::SFAccount,
            };
            View::Login
        }
        // "settings" opens the first tab; "settings-play" and the rest
        // open one particular one, so a screenshot can be taken of any
        // page of it without clicking
        s if s.starts_with("settings") => {
            helper.settings_tab = match s.trim_start_matches("settings") {
                "-play" => crate::SettingsTab::Play,
                "-queue" => crate::SettingsTab::Queue,
                "-stealth" => crate::SettingsTab::Stealth,
                // "-owner" is what everybody else sees, "-owner-open" is
                // what he sees once the password is in. They are two
                // different pages and both have to be looked at
                "-owner" => crate::SettingsTab::Owner,
                "-owner-open" => {
                    helper.config.admin_unlocked = true;
                    crate::SettingsTab::Owner
                }
                _ => crate::SettingsTab::Look,
            };
            View::Settings
        }
        other => View::Account {
            ident,
            page: preview_page(other),
        },
    };
}

/// An Arena Manager mid-run: money in the purse, ten buildings at different
/// levels, and a sacrifice offer sitting just under a threshold
fn idle_game() -> sf_api::gamestate::idle::IdleGame {
    use num_bigint::BigInt;
    use sf_api::gamestate::idle::IdleGame;
    // The private fields keep a literal out of reach, so the defaults are
    // filled in first and the interesting ones written over
    let mut idle = IdleGame::default();
    idle.current_money = BigInt::from(48_600_000_000u64);
    idle.current_runes = BigInt::from(87_711u32);
    idle.resets = 37;
    idle.sacrifice_runes = BigInt::from(4_180u32);
    for (i, (_, b)) in idle.buildings.iter_mut().enumerate() {
        b.level = (240 - i as u32 * 19).max(0);
        b.upgrade_cost =
            BigInt::from(3_400_000_000u64 * (i as u64 + 1) * (i as u64 + 1));
    }
    idle
}

/// A guild with a full bench, so the member table can be looked at without a
/// live login
/// Fill the app's own diary for the fixture guild, so the time-in-guild
/// column has something to say. Half the roster was here before the diary
/// started (a floor - "120+ d"), half joined while it was watching (an
/// exact figure), and one has never been seen at all ("?"), because all
/// three of those have to be looked at
fn seed_guild_diary(helper: &mut crate::Helper) {
    use chrono::Duration;
    let Some(server) = helper.servers.0.values().next() else {
        return;
    };
    let url = server.ident.url.to_string();
    let Some(account) = server.accounts.values().next() else {
        return;
    };
    let guard = account.status.lock().unwrap();
    let gs = match &*guard {
        crate::AccountStatus::Idle(_, gs)
        | crate::AccountStatus::Busy(gs, _) => gs,
        _ => return,
    };
    let Some(guild) = gs.guild.as_ref() else {
        return;
    };
    let key = crate::guild_history::GuildHistory::key(&url, guild);
    let names: Vec<String> =
        guild.members.iter().map(|m| m.name.clone()).collect();
    drop(guard);

    // One raid on the book, lost, so the "last raid" line has something
    // to say
    helper
        .guild_history
        .raids
        .entry(key.clone())
        .or_default()
        .extend([
            crate::guild_history::RaidRecord {
                number: 52,
                power: 262_000_000.0,
                won: true,
                when: chrono::Local::now() - Duration::days(4),
                chance: Some(0.71),
                signed_chance: Some(0.66),
                signed_up: Some(44),
            },
            crate::guild_history::RaidRecord {
                number: 53,
                power: 278_000_000.0,
                won: false,
                when: chrono::Local::now() - Duration::hours(3),
                chance: Some(0.58),
                signed_chance: Some(0.41),
                signed_up: Some(37),
            },
        ]);
    let today = chrono::Local::now().date_naive();
    let diary = helper.guild_history.guilds.entry(key).or_default();
    for (i, name) in names.iter().enumerate() {
        // Never seen: the last one in the list
        if i + 1 == names.len() {
            continue;
        }
        let entry = diary.entry(name.to_lowercase()).or_default();
        let age = 400 - (i as i64 * 7);
        entry.levels.insert(today - Duration::days(age.max(1)), 100);
        entry.levels.insert(today, 200);
        // The second half turned up while the diary was running, so their
        // figure is exact rather than a floor
        if i * 2 >= names.len() {
            entry.joined_at =
                Some(chrono::Local::now() - Duration::days(age.max(1)));
        }
    }
}

fn guild() -> sf_api::gamestate::guild::Guild {
    use sf_api::gamestate::guild::{
        BattlesJoined, Guild, GuildMemberData, GuildRank,
    };
    // Fifty: the size of the guild this is built for. The member table has
    // to fit all of them on one 1080p screen without a scrollbar, so the
    // fixture is the real thing rather than a comfortable half of it
    let names = [
        "Commander", "Ravenna", "Tycho", "Bělinka", "Morgrim", "Astra",
        "Kvido", "Selene", "Dorian", "Nyx", "Radomír", "Pepa z depa", "Thalia",
        "Brumla", "Oktavián", "Livie", "Zephyr", "Marek", "Isolde", "Vlkodlak",
        "Cassia", "Hubert", "Věra", "Otakar", "Sylva", "Bohdan", "Elaine",
        "Krtek", "Adéla", "Lucius", "Ondřej", "Miriam", "Tobiáš", "Kamila",
        "Šimon", "Anežka", "Vojtěch", "Ludmila", "Kryštof", "Barbora",
        "Matyáš", "Rozálie", "Štěpán", "Johanka", "Vilém", "Dobroslav",
        "Květa", "Bohumil", "Zdislava", "Prokop",
    ];
    let mut members = Vec::new();
    for (i, name) in names.iter().enumerate() {
        members.push(GuildMemberData {
            name: (*name).to_string(),
            level: 260u16.saturating_sub(i as u16 * 3).max(120),
            last_online: Some(
                Local::now() - chrono::Duration::hours(i as i64 * 7),
            ),
            treasure_skill: 45u16.saturating_sub(i as u16 / 2),
            instructor_skill: 45u16.saturating_sub(i as u16 / 2),
            guild_pet_lvl: 200u16.saturating_sub(i as u16 * 3).max(50),
            guild_rank: match i {
                // SFC_PREVIEW_MEMBER: the page as an ordinary member sees it
                0 if std::env::var("SFC_PREVIEW_MEMBER").is_ok() => {
                    GuildRank::Member
                }
                0 => GuildRank::Leader,
                1..=8 => GuildRank::Officer,
                49 => GuildRank::Invited,
                _ => GuildRank::Member,
            },
            portal_fought: (i % 3 != 0)
                .then(|| Local::now() - chrono::Duration::hours(2)),
            battles_joined: if i < 39 {
                Some(BattlesJoined::Attack)
            } else {
                None
            },
            knights: 20u8.saturating_sub(i as u8),
            ..Default::default()
        });
    }
    let mut guild = Guild::default();
    // A raid declared, thirty-nine of fifty signed up - so the odds line
    // has both of its figures to show
    guild.attacking = Some(sf_api::gamestate::guild::PlanedBattle {
        // The library's own marker for "a raid, not another guild"
        other: 1_000_000,
        date: Local::now() + chrono::Duration::hours(5),
    });
    // A few lines of chat, both kinds: people talking and the game's own
    guild.chat = vec![
        sf_api::gamestate::guild::ChatMessage {
            user: String::new(), time: None,
            message: "#du#KaličůvPalec#90#96".into(),
        },
        sf_api::gamestate::guild::ChatMessage {
            user: String::new(), time: None,
            message: "#bd#02:04 PoseidonVltavy#0".into(),
        },
        sf_api::gamestate::guild::ChatMessage {
            user: "Zaiyfu".into(),
            time: Some(chrono::NaiveTime::from_hms_opt(14, 16, 0).unwrap()),
            message: "from what i found the final boss takes only up to 56-57"
                .into(),
        },
        sf_api::gamestate::guild::ChatMessage {
            user: String::new(),
            time: None,
            message: "Hrac Bohdan dosahl urovne 186.".into(),
        },
        sf_api::gamestate::guild::ChatMessage {
            user: "Petr Macinka".into(),
            time: Some(chrono::NaiveTime::from_hms_opt(13, 58, 0).unwrap()),
            message: "raid v 18:00, prihlaste se".into(),
        },
    ];
    guild.name = "Imperium".into();
    guild.description = "Nejlepší cech na serveru.".into();
    guild.honor = 184_920;
    guild.rank = 7;
    guild.joined = Some(Local::now() - chrono::Duration::days(430));
    guild.own_treasure_skill = 45;
    guild.total_treasure_skill = 1_180;
    guild.own_instructor_skill = 45;
    guild.total_instructor_skill = 1_140;
    guild.finished_raids = 52;
    guild.own_pet_lvl = 200;
    guild.pet_max_lvl = 200;
    guild.members = members;
    guild
}

/// Only the offline preview reads this: it lets a screenshot run walk every
/// page without a mouse.
fn preview_page(name: &str) -> AccountPage {
    match name {
        "tavern" => AccountPage::Tavern,
        "scrapbook" => AccountPage::Scrapbook,
        "guard" => AccountPage::Guard,
        "shops" => AccountPage::Shops,
        "arena" => AccountPage::ArenaManager,
        "pets" => AccountPage::Pets,
        "dungeons" => AccountPage::Dungeons,
        "underworld" => AccountPage::Underworld,
        // "guild-open" is the same page with the summary drawer down, so
        // the officers line and the raid odds can be looked at too
        "guild" | "guild-open" | "guild-chat" => AccountPage::Guild,
        "mail" => AccountPage::Mail,
        "log" => AccountPage::Log,
        "options" => AccountPage::Options,
        _ => AccountPage::Fortress,
    }
}

pub fn update(helper: &mut Helper, message: Message) -> Command<Message> {
    match message {
        Message::TavernTab(tab) => helper.tavern_tab = tab,
        Message::WindowResized { width, height } => {
            helper.window_w = width as f32;
            helper.window_h = height as f32;
        }
        Message::ViewSubPage { player, page }
            if helper.servers.get_ident(&player).is_some() =>
        {
            helper.current_view = View::Account {
                ident: player,
                page,
            };
        }
        Message::ViewSettings => helper.current_view = View::Settings,
        Message::ViewOverview => {
            helper.current_view = View::Overview {
                selected: Default::default(),
                action: None,
            }
        }
        Message::GuildMemberDetail { ident, name } => {
            if let Some(account) = helper.servers.get_account_mut(&ident) { account.social.selected_member = name; }
        }
        Message::GuildMemberSort { ident, sort } => {
            if let Some(account) = helper.servers.get_account_mut(&ident) { account.social.member_sort = sort; }
        }
        Message::GuildShowMembers { ident, show } => {
            if let Some(account) = helper.servers.get_account_mut(&ident) { account.social.show_chat = !show; }
        }
        Message::GuildDrawer { ident } => {
            if let Some(account) = helper.servers.get_account_mut(&ident) { account.social.drawer_open = !account.social.drawer_open; }
        }
        Message::GuildTwoColumns(on) => helper.config.guild_two_columns = on,
        Message::GuildCompact { ident, state } => {
            if let Some(account) = helper.servers.get_account_mut(&ident) { account.social.compact_members = state; }
        }
        Message::ShowSettingsTab(tab) => helper.settings_tab = tab,
        Message::ToggleHelp => {
            helper.config.show_help = !helper.config.show_help;
            crate::ui::glyphs::set_help(helper.config.show_help);
        }
        Message::ToggleDrawnFont => {
            helper.config.drawn_font = !helper.config.drawn_font;
            crate::ui::glyphs::set_artwork(helper.config.drawn_font);
        }
        Message::ChangeTheme(theme) => helper.config.theme = theme,
        Message::SetLanguage(lang) => {
            helper.config.lang = lang;
            crate::i18n::set_lang(lang);
        }
        Message::ToggleConsole => helper.console_open = !helper.console_open,
        // The pet simulator is arithmetic on the fixture, not a request:
        // it runs here so the page can be tried, not only looked at
        Message::PetSimHabitat { ident, .. }
        | Message::PetSimTheirPet { ident, .. }
        | Message::PetSimField { ident, .. } => {
            helper.pet_sim_input(message);
            helper.refresh_pet_sim(ident);
        }
        _ => {}
    }
    Command::none()
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::Application;

    #[test]
    fn preview_never_dispatches_live_actions() {
        let (mut helper, _) = Helper::new(crate::Args {
            sub: None,
            replaced: None,
            handover: None,
            preview: true,
        });
        assert_eq!(helper.subscription().into_recipes().len(), 1); // Window resize only; no game timers.
        assert!(
            helper
                .update(Message::WindowResized {
                    width: 1200,
                    height: 720
                })
                .actions()
                .is_empty()
        );
        assert_eq!((helper.window_w, helper.window_h), (1200.0, 720.0));
        for tab in [
            crate::ui::TavernTab::Rewards,
            crate::ui::TavernTab::Settings,
            crate::ui::TavernTab::Expeditions,
        ] {
            assert!(
                helper.update(Message::TavernTab(tab)).actions().is_empty()
            );
            assert_eq!(helper.tavern_tab, tab);
        }
        assert!(helper.update(Message::UpdateCheck).actions().is_empty());
        assert!(helper.update(Message::ViewSettings).actions().is_empty());
        assert_eq!(helper.current_view, View::Settings);
        let ident = helper
            .servers
            .0
            .values()
            .next()
            .unwrap()
            .accounts
            .values()
            .next()
            .unwrap()
            .ident;
        assert!(
            helper
                .update(Message::PetFeedTimer { ident })
                .actions()
                .is_empty()
        );
        assert!(
            helper
                .update(Message::ShopTimer { ident })
                .actions()
                .is_empty()
        );
        assert!(helper.config.accounts.is_empty());
        assert!(matches!(
            &*helper
                .servers
                .get_ident(&ident)
                .unwrap()
                .1
                .status
                .lock()
                .unwrap(),
            AccountStatus::Busy(..)
        ));
    }

    #[test]
    fn quest_fallback_respects_master_switch_and_sacrifice_can_run_alone() {
        let server = ServerIdent::new("https://preview.invalid/");
        let mut account = AccountInfo::new(
            "test",
            PlayerAuth::SSO,
            AccountIdent {
                server_id: server.id,
                account: AccountID(1),
            },
        );
        account.expedition_info.auto = false;
        account.expedition_info.quest_auto = true;
        assert!(!crate::message::account_job_enabled(
            &account,
            crate::player::Job::Expedition
        ));
        account.expedition_info.quest_auto = false;
        assert!(!crate::message::account_job_enabled(
            &account,
            crate::player::Job::Expedition
        ));
        account.idle_info.auto_upgrade = false;
        account.idle_info.auto_collect = false;
        account.idle_info.auto_sacrifice = true;
        assert!(crate::message::account_job_enabled(
            &account,
            crate::player::Job::ArenaManager
        ));
    }
}
