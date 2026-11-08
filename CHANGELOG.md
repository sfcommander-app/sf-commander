# Changelog

## 6.2.14 — Arena Manager large-number fix
- All Arena Manager values now share one arbitrary-precision named formatter: money, owned/offered/target runes, remaining runes and every building upgrade price
- The formatter covers named scales through vigintillion instead of stopping at quintillion and leaving another twelve digits in front of the unit
- Exact unrounded values remain available on hover; affordability calculations are unchanged

## 6.2.13 — Activity and Arena Manager clarity
- Activity now starts with a Companion-style overview of the named automation run, current task, queued work, last completed task and today's results
- Human-readable live/module history is separated from the durable verified write history; internal timestamp action IDs stay out of the UI
- The durable write journal is loaded from a bounded tail and cached, so opening Activity does not repeatedly read a growing file
- Arena Manager shows large rune totals by magnitude (for example `12.4 trillion`) with the exact value available on hover
- New Activity and rune labels have Czech translations

## 6.2.12 — Guild Black Gem chance
- Guild summary shows the Black Gem chance from eligible members
- Each member contributes exactly 1% only when their own Hall of Knights is level 15 or higher; low member levels are never pooled together

## 6.2.11 — kalic-updates
- Master Kill Switch starts active and gates every command that can change game state
- Central write executor records started, blocked, verified failure, inconclusive and verified success outcomes
- Ambiguous write/connection failures activate the Kill Switch instead of being replayed automatically
- Remembered password hashes migrate out of `helper.toml` into Windows Credential Manager
- Durable, redacted action journal with CSV export from the Activity panel
- Windows CI now tests, lints, builds, packages, installs and smoke-tests the application
- Release tags must match the Cargo package version before artifacts can be produced
- Enabled guild duties are no longer deliberately skipped; authoritative attendance from system mail remains unknown until confirmed fixtures are available

## 6.2.10
- Guild overview: rows aligned
- Guild legend: treasury / instructor / knights

## 6.2.9
- Sidebar tabs fill the full height
- Arena Manager buys only with the money the game reports
- Arena: no endless re-rolling of the same three opponents
- Live feed doesn't repeat the same line

## 6.2.8
- Arena Manager: sacrifice threshold works with huge rune counts
- Arena Manager: fewer refused purchases after a reset

## 6.2.7
- Arena Manager: buying no longer gets stuck

## 6.2.6
- Guild chat: Tower floor clears readable
- Arena Manager: sacrifice threshold no longer shifts randomly

## 6.2.5
- Replaced gear: gem taken out first, then dismantled or sold, legendaries kept
- Removed the "recruit a soldier after every fortress attack" switch
- Guild chat: catapult message readable

## 6.2 – 6.2.4
- UI redesign: new colours, icon sidebar with countdowns, character overview, tavern tabs
- Guild overview of all members in two columns
- New font with Czech characters
- Beer no longer retried every hour when mushrooms are short

## 6.1
- Guild member table readable at full size
- Guild: attackable guilds with estimated chance
- New live feed lines for potions and legendary drops

## 6.0
- Plays continuously at a human pace: OFF / STEALTH / BALANCED / NO SHAME, night rest
- Puts on better gear from the backpack
- Blacksmith upgrades gear, shops get rerolled, spare epics dismantled
- Fortress checked a few times a day with a sensible build order
- Underworld lures pick targets around the Keeper's level
- Pets: all fed as far as the fruit goes
- Guild: battle sign-up within the next 6 h, hydra, guild pets
- Quests skipped with hourglasses, Arena Manager buys in bulk
- UI redesign

## 5.1.6
- Full backpack no longer stops dungeons, gem mine and daily rewards
- Settings kept after a restart
- Crawler backs off when its session dies
- helper.log limited to 20 MB

## 4.8
- Member list fits on one screen, officers highlighted
- Arena Manager: sacrifice threshold up to 99 000, money no longer left unspent
- Twister odds learned from the level gap
- Underworld builds Heart of Darkness, Soul Extractor and Gladiator Trainer
- Underworld lures fixed
- Sidebar tabs show countdowns and state
- Background polling removed
- Two characters never send a request in the same second
- Arena re-rolls a hopeless set of opponents
- Guild chat: game messages readable
- Guild founding date

## 4.7
- Arena Manager: sacrifice uses the right rune count
- Witch skips items that can't be enchanted

## 4.6
- "Do it now" and sessions no longer overlap
- Guild chat: promotions and demotions readable

## 4.5
- One safety slider 0–10
- "Do it now" button
- Game messages in the guild chat
- New members' first 24 h not held against them
- Raid win chance shown and copyable
- Steam / Google login survives updates
- Guild contribution ladder and officer seat advice

## 4.2
- Sessions come when there is work to do
- Arena learns from real fight results
- Daily mushroom budget
- Gear scoring fix

## 2.1
- Portal attendance in the guild table
- Fortress attacks at random times
- Gems socketed automatically, gem colours can be protected
- Gear recycling: gem moved to the new piece, old piece dismantled or sold
- Witch enchants, blacksmith limits
- Safety profiles NO SHAME / BALANCED / STEALTH
- Settings per character

## 2.0
- Play sessions of varying length depending on time of day and work left

## 1.98
- Random gaps between sessions, quiet hours, request limit, characters never start together

## 1.97
- Daily mushroom limit
- Blacksmith upgrades for the character and companions
- Follow mode
- Guild table colours, kick advice only with a hard reason
- Underworld build orders

## 1.80 – 1.81
- Renamed to S&F Commander
- Better gear bought before attributes
- Arena Manager sacrifice follows the game's own counter

## 1.78
- Safe saving of helper.toml with backup and recovery
- Daily mushroom limits survive a restart

## 1.76 – 1.77
- Guild finder for characters without a guild
- Beer on by default, city guard only after the tavern is done

## 1.38
- Level-up popup
- Companions (Bert, Mark, Kunigunde) get gear from the backpack
- Dungeons with unknown odds no longer show 100 %
- Sounds
- Guild chat always visible

## 1.31 – 1.37
- Pet feeding fixed, pet arena
- Dice game
- Backpack keeps free slots
- Expeditions aim for 40 heroism
- Updates arrive within minutes
- Live feed
- City guard never starts just before the daily reset

## 1.21 – 1.30
- Expedition item chains, XP / gold switch
- Daily reward and task chests
- Gems socketed and sold
- STOP stops everything, RESUME logs back in
- Automatic updates
- Guild bonus bars
- Arena Manager buys newly unlocked buildings
- No free fight left unused

## 1.17 – 1.20
- Several accounts in tabs
- FULL BOT switch and job queue
- City guard, beer and wheel of fortune automation
- Arena Manager automatic sacrifice
- Guild battle odds
- Human pace / full power
- Shops: automatic gear buying with protections, attributes
- Console, activity log, sessions
- New look

## 1.15 and older
- Czech language
- Member grades and kick advice
- Automatic expeditions
- Arena Manager upgrades
- Pet feeding
- Dungeons with simulated win chance
- Fortress: attacks, recruiting, collecting, gem mine, upgrades
- Guild chat and battle sign-up
- Mail
- Scrapbook target selection
