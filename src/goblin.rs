//! The voice in the live feed: SF Commander speaking to its user the way
//! KITT spoke to Michael and JARVIS to Stark - the assistant that runs the
//! account, reports what just happened, what it means and what it is
//! doing about it next. Calm, competent, on the user's side. Pure flavour;
//! nothing here changes what the bot does. Every line is an i18n key, so
//! it speaks Czech when the app does.
//!
//! The module keeps its old name; the greedy goblin it was written for
//! read as a fool and got replaced.

use std::sync::atomic::{AtomicI64, Ordering};

use crate::i18n::t;

/// When the feed last carried one of these lines, as a unix time - so it
/// carries one at a time, never a pile-up when three characters win
/// three fights in the same minute
static LAST_QUIP: AtomicI64 = AtomicI64::new(0);

/// The least time between two lines
pub const QUIP_GAP_SECS: i64 = 90;

/// May a line go out now? Says yes once, then holds for `QUIP_GAP_SECS`
#[must_use]
pub fn may_speak() -> bool {
    let now = chrono::Local::now().timestamp();
    let last = LAST_QUIP.load(Ordering::Relaxed);
    if now - last < QUIP_GAP_SECS {
        return false;
    }
    LAST_QUIP.store(now, Ordering::Relaxed);
    true
}

/// What just happened, for the feed to have a word about
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mood {
    DiceLost,
    DiceWon,
    DungeonLost,
    DungeonWon,
    EpicFound,
    LevelUp,
    SessionOpen,
    /// A session with little to do - the thirst is spent, the fights used
    SessionOpenQuiet,
    FeetUp,
    ShopEmpty,
    /// The weapon shop held the day's find
    WeaponShop,
    /// The magic shop held the day's find
    MagicShop,
    Blacksmith,
    QuietHours,
    ArenaWon,
    ArenaLost,
    /// 6.1: a potion went down
    PotionDrunk,
    /// 6.1: a legendary dropped
    LegendaryFound,
}

/// One line for the mood, drawn at random so the same event does not read
/// the same twice in a row. Six a mood, every one true to the game: the
/// arena drops no items, dungeons come once an hour, the shops restock
/// at midnight, a reroll costs a mushroom. "I" is SF Commander, "you" is
/// the user
#[must_use]
pub fn quip(mood: Mood) -> &'static str {
    let lines: &[&'static str] = match mood {
        Mood::DiceLost => &[
            "The dice went against us. I only ever stake spare silver there, so nothing in your plan moved.",
            "A losing throw. I am after the hourglasses, not the pot - I will keep playing while the budget holds.",
            "Nothing on the dice this round. Variance, not a trend; the next throw knows nothing about this one.",
            "Lost at the dice. Two matching dice and I push, three and I take the pot - the odds have not changed.",
            "The cup player took this one. Your mushrooms were not touched; the dice never get those.",
            "A miss at the dice. I stop the moment the silver reserve says so - you will not see a losing streak eat the purse.",
        ],
        Mood::DiceWon => &[
            "Dice won. Silver back and an hourglass on top - I will spend it on thirst before the day is out.",
            "Three of a kind. The winnings go where everything else goes: gear first, attributes with what is left.",
            "A winning throw. Small stakes, steady hand - the cup player pays out to the patient.",
            "Won at the dice. Every hourglass here is thirst you do not have to wait for tomorrow.",
            "The dice paid. Banked, and I am back to the tavern - the expedition is where your level comes from.",
            "A good throw. I take the winnings and leave the table; the pot was never the point.",
        ],
        Mood::DungeonLost => &[
            "The dungeon boss held. He outscales you for now - a few levels and a better main weapon and I will have that door open.",
            "Lost in the dungeon. That is data, not a setback: the next free attempt is in an hour and the gear step runs before it.",
            "The boss won this one. Dungeons come down to main attribute and constitution, so that is where I am sending the next gold.",
            "Beaten in the dungeon. No soldiers, no gold spent, only the free attempt - the cheapest lesson the game sells.",
            "The boss is one step ahead. I will be back in an hour with the same key and a little more strength.",
            "Knocked out of the dungeon. Bert, Mark and Kunigunde get the next finds; a stronger party takes this floor.",
        ],
        Mood::DungeonWon => &[
            "Dungeon boss down. That is a day's thirst worth of XP in one fight - I have the next attempt lined up for the top of the hour.",
            "Floor cleared. Each boss is worth more than the last; the free fight timer is already running for the next.",
            "Dungeon won. I judged the loot against what you wear - if it was better, it is on already.",
            "Another boss done. This is where your levels come cheap: one fight an hour, no gold, no mushrooms.",
            "The boss fell. Straight on to the next door - the XP curve rewards whoever keeps knocking.",
            "Dungeon cleared. Any gear from it goes to whoever it helps most: you first, then the companions.",
        ],
        Mood::EpicFound => &[
            "An epic. Every attribute on one piece - this is the tier I plan the whole build around.",
            "Epic found. I checked it against the worn piece; if it wins, it is on and the old one is dismantled or sold, not wasted.",
            "Purple. An epic lifts every fight at once: arena, dungeon, guild - one drop, three fronts.",
            "An epic in the bag. This is what the reserve is for: a gem in the socket, a few blacksmith levels, and it stays for a long time.",
            "Epic. If it is the wrong class for you, it may still fit a companion - nobody in the party goes without a look.",
            "An epic is yours. Protected from now on: no shop offer replaces it unless it is clearly the better piece.",
        ],
        Mood::LevelUp => &[
            "Level up. The shops restock to your new level, so my next look at them is worth more than the last.",
            "New level. Attributes get pricier from here - gear first, then the cheapest attribute that still pays.",
            "Level gained. Nothing changes in the routine, everything changes in the numbers.",
            "Up a level. I read the average item level for the blacksmith plan - pieces that fall behind get raised or replaced.",
            "Level up. A few more and the next dungeon door stops being a wall.",
            "Another level. The scrapbook fills faster now: stronger arena opponents wear things you have not collected yet.",
        ],
        Mood::SessionOpen => &[
            "Session open. Order of business: thirst in the tavern, then the free fights, then the shops and the fortress.",
            "I am in, and there is work. Thirst does not carry over, so the expedition goes first - everything else keeps.",
            "Session started. The plan is XP per minute: expedition, dungeon, arena, in that order; the gold decides the rest.",
            "Logged in. Beer counts as thirst today, the dice give the hourglasses, the guild battle is signed.",
            "Open. A short list in the right order and off again - that is how a strong account looks from the outside.",
            "Session open. Thirst first, always; the arena and dungeon timers run whether I am here or not.",
        ],
        Mood::SessionOpenQuiet => &[
            "A short session. Your thirst is spent, so it is the free fights, the fortress and a look at the shops.",
            "In for the timers. Arena and dungeon if they are up, collect the fortress, out again.",
            "Little to do today. A player with nothing to do leaves - so I keep this brief.",
            "Quick visit. I am checking the gold against the reserve; if the shop has a real upgrade, I buy it now.",
            "Session with a short list. The guild battle signature and the fortress collect are all that is due.",
            "Nothing pressing. The value of this visit is the timers - one arena fight, one dungeon, done.",
        ],
        // One visit is skipped, not the day - the console line above this
        // one says when the next comes
        Mood::FeetUp => &[
            "Everything is done: thirst spent, free fights used, shops checked. I am logging off early - a full account has no reason to stay.",
            "Nothing left that pays. Staying logged in would be traffic without progress, so I am closing the session.",
            "Done ahead of time. The next thing worth clicking is an hour away on the timers.",
            "All timers used, all gold placed. Closing - your progress for the day is in.",
            "Done for now. The expedition runs on its own; there is nothing to gain by watching it.",
            "Nothing pending. Logging off keeps the account looking like a person with a life - which is the point.",
        ],
        Mood::ShopEmpty => &[
            "Both shops checked, nothing beats what you wear. I reroll only if the mushroom budget allows - a bad reroll is a wasted mushroom.",
            "No upgrade in the shops today. The gold stays in the reserve; the next restock is at midnight.",
            "Shops empty of anything useful. Normal past a certain item level - your real upgrades come from dungeons and epics now.",
            "Nothing in the shop worth the gold. Attributes take what is above the reserve instead - a level is never wasted.",
            "Shop pass. Neither the weapon smith nor the magic shop has a piece over the threshold - the threshold is there to stop the churn.",
            "Looked at the shops, kept the gold. The companions did not need anything from there either.",
        ],
        Mood::WeaponShop => &[
            "Weapon shop: every blade, bow and plate weighed against what you and the companions wear - and one of them wins. Buying.",
            "Went through the weapon smith's rack. One piece beats a worn one by the margin you set; the gold is there, it is yours.",
            "Weapon shop checked. Most of it is filler at your level, but one piece scores higher than its slot - I am taking it.",
            "The armourer had something today. Scored on attributes, damage and armor, not on looks - it goes on the moment it is in the bag.",
            "Weapon shop: a real upgrade among the usual six. The old piece is recycled, gem first, then dismantled or sold.",
            "Found it in the weapon shop. The companions were in the comparison too - it goes to whoever gains most.",
        ],
        Mood::MagicShop => &[
            "Magic shop: rings, amulets, belts and talismans weighed against yours - one of them is a step up. Buying.",
            "Went through the magic shop's shelf. A piece there beats a worn one by your margin, with the gold to spare.",
            "Magic shop checked. Trinkets are where the attributes hide; one of today's carries more than what you wear.",
            "The magic shop had the find today. Scored like everything else - attributes first, the rune counted - and it wins its slot.",
            "Magic shop: a real upgrade among the usual six. The piece it replaces is recycled, gem first, then dismantled or sold.",
            "Found it in the magic shop. Bert, Mark and Kunigunde were in the comparison too - it goes to whoever gains most.",
        ],
        Mood::Blacksmith => &[
            "Blacksmith upgrade done. Metal and arcane splinters into a piece you will wear for a long while - the only kind worth raising.",
            "Raised a piece at the blacksmith. Each level is a few percent on every fight, paid in resources the shop cannot use anyway.",
            "Upgrade forged. I keep the stock above forty percent - the next epic will want splinters too.",
            "Blacksmith step: the weakest worn piece got the level; nothing under your average gets raised at all.",
            "Another blacksmith level. Cheaper than any shop piece, and it stays when the shop changes its mind.",
            "Raised at the forge. A gemmed, raised epic is a piece for twenty levels - that is my plan for each slot.",
        ],
        Mood::QuietHours => &[
            "Quiet hours. Nothing goes out until morning - an account that plays at four every night is not a person.",
            "Night. The timers keep running, the thirst resets at midnight, and the first session after the edge picks everything up.",
            "Quiet hours hold. I moved the edges a little today - the same bedtime every night is a pattern too.",
            "Lights out. Progress lost overnight: none. The expedition already running finishes by itself.",
            "Night rest. Full power would override this - it is off, so the account sleeps like its owner.",
            "Quiet hours. The one thing worth waking for tonight would be a guild battle, and that is signed already.",
        ],
        Mood::ArenaWon => &[
            "Arena won. Honor up, and if the opponent wore anything new, your scrapbook has one more page - that is the real prize.",
            "Won in the arena. The free fight comes every ten minutes; the gold is small, the scrapbook bonus is not.",
            "Arena win. I picked the opponent for pages, not for honor - a fuller scrapbook is XP on every quest.",
            "Beat them in the arena. Next free fight in ten minutes, next target already chosen by what they wear.",
            "Arena won without a mushroom spent. Free fights only - the paid ones are the worst deal in the game.",
            "Won. Honor is a side effect; the hall of fame is climbed with levels, the arena is for the album.",
        ],
        Mood::PotionDrunk => &[
            "Cheers! A potion down - the next hours hit harder.",
            "Glug glug. The stats just went up; the timer is running, so I keep the fights coming.",
            "Potion in. Same character, bigger numbers - let us use it.",
        ],
        Mood::LegendaryFound => &[
            "A LEGENDARY. Hold on - this one does not come twice.",
            "Legendary drop! It goes on whoever it fits best, and it is never sold.",
            "Orange glow in the bag. Today is a good day.",
        ],
        Mood::ArenaLost => &[
            "Lost in the arena. It cost you nothing but the ten-minute timer; I pick the next target weaker, the page is still out there.",
            "Arena loss. They had the stats; nothing is lost but honor, and honor buys nothing.",
            "Beaten. The scrapbook page stays open - a level or two and the same opponent goes down.",
            "Lost this fight. Free attempt, free lesson: you need constitution before you need luck.",
            "Arena lost. Noted - my next pick favours a weaker player with pages still missing from your album.",
            "A loss in the arena. Ten minutes and the free fight is back; nothing else in the plan moves.",
        ],
    };
    t(lines[fastrand::usize(..lines.len())])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mood_has_a_line() {
        for mood in [
            Mood::DiceLost,
            Mood::DiceWon,
            Mood::DungeonLost,
            Mood::DungeonWon,
            Mood::EpicFound,
            Mood::LevelUp,
            Mood::SessionOpen,
            Mood::SessionOpenQuiet,
            Mood::FeetUp,
            Mood::ShopEmpty,
            Mood::WeaponShop,
            Mood::MagicShop,
            Mood::Blacksmith,
            Mood::QuietHours,
            Mood::ArenaWon,
            Mood::ArenaLost,
        ] {
            assert!(!quip(mood).is_empty());
        }
    }

    /// One line, then silence for the gap - three fights in a minute are
    /// one remark, not three
    #[test]
    fn the_voice_does_not_pile_up() {
        LAST_QUIP.store(0, Ordering::Relaxed);
        assert!(may_speak());
        assert!(!may_speak());
    }
}
