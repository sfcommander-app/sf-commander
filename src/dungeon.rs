use sf_api::{
    gamestate::{
        GameState,
        dungeons::{Dungeon, DungeonProgress, LightDungeon},
    },
    simulate::{Fighter, PlayerFighterSquad, simulate_battle},
};

use crate::player::DungeonChance;

/// How many fights get simulated per dungeon. 1000 is precise enough to show
/// whole percent and still takes only a few milliseconds per dungeon
pub const SIM_ITERATIONS: u32 = 1000;

/// Simulates every dungeon the character can currently enter and returns how
/// likely a win against the next enemy is. Dungeons, that are locked or
/// already cleared, are included as well (with no enemy), so that the list
/// stays complete
#[must_use]
pub fn compute_chances(gs: &GameState, iterations: u32) -> Vec<DungeonChance> {
    let squad = PlayerFighterSquad::new(gs);
    let player = Fighter::from(&squad.character);
    let own_level = i32::from(gs.character.level);

    let companions: Vec<Fighter> = squad
        .companions
        .as_ref()
        .map(|c| c.iter().map(|(_, f)| Fighter::from(f)).collect())
        .unwrap_or_default();

    let mut res = Vec::new();

    let mut push = |dungeon: Dungeon,
                    raw_name: &str,
                    progress: DungeonProgress,
                    shadow: bool| {
        let open = matches!(progress, DungeonProgress::Open { .. });
        let (stage, enemy, enemy_level, win, no_data) = match progress {
            DungeonProgress::Locked => {
                (0, "locked".to_string(), 0, f64::NAN, false)
            }
            DungeonProgress::Finished => {
                (0, "cleared".to_string(), 0, f64::NAN, false)
            }
            DungeonProgress::Open { finished } => {
                match gs.dungeons.current_enemy(dungeon) {
                    // An enemy the tables have no damage figures for. The
                    // library fills those in as zero, so the simulation has
                    // it hitting for nothing and hands back a confident
                    // 100 % - which is how the twister ended up looking like
                    // the safest fight in the game and swallowing the free
                    // fight every hour. We can not simulate it - but we can
                    // learn its odds by fighting it once a day and remembering
                    // how it went, which the caller fills back in here
                    Some(monster)
                        if monster.min_dmg == 0 && monster.max_dmg == 0 =>
                    {
                        (
                            finished,
                            format!("{} (no data)", monster.name),
                            monster.level,
                            f64::NAN,
                            true,
                        )
                    }
                    Some(monster) => {
                        // The tower and the shadow dungeons are fought with
                        // the companions; the light dungeons and the twister
                        // are not. `sf-api` knows which is which, so asking
                        // it beats hardcoding the tower
                        let mut left = vec![player.clone()];
                        if dungeon.is_with_companions() {
                            left.extend(companions.iter().cloned());
                        }
                        let right = [Fighter::from(monster)];
                        let result = simulate_battle(
                            &left,
                            &right,
                            iterations,
                            false,
                        );
                        (
                            finished,
                            monster.name.to_string(),
                            monster.level,
                            result.win_ratio,
                            false,
                        )
                    }
                    // No monster data for this stage - the fight is still
                    // possible, we just can not predict it
                    None => {
                        (finished, "unknown enemy".to_string(), 0, f64::NAN, true)
                    }
                }
            }
        };

        res.push(DungeonChance {
            dungeon,
            name: crate::util::split_camel(raw_name),
            enemy,
            enemy_level,
            stage,
            win,
            shadow,
            open,
            no_data,
            level_gap: i16::try_from(i32::from(enemy_level) - own_level)
                .unwrap_or(0),
        });
    };

    for (dungeon, progress) in &gs.dungeons.light {
        push(
            Dungeon::Light(dungeon),
            &format!("{dungeon:?}"),
            *progress,
            false,
        );
    }
    for (dungeon, progress) in &gs.dungeons.shadow {
        push(
            Dungeon::Shadow(dungeon),
            &format!("{dungeon:?}"),
            *progress,
            true,
        );
    }

    // Enterable dungeons first, those by best odds; unknown odds after the
    // known ones, then everything locked or cleared
    res.sort_by(|a, b| {
        b.open.cmp(&a.open).then_with(|| {
            match (a.win.is_nan(), b.win.is_nan()) {
                (true, true) => std::cmp::Ordering::Equal,
                (true, false) => std::cmp::Ordering::Greater,
                (false, true) => std::cmp::Ordering::Less,
                (false, false) => b
                    .win
                    .partial_cmp(&a.win)
                    .unwrap_or(std::cmp::Ordering::Equal),
            }
        })
    });
    res
}

/// The level of the tower we are currently on, needed for `FightTower`
#[must_use]
pub fn tower_level(gs: &GameState) -> Option<u8> {
    match gs.dungeons.light[LightDungeon::Tower] {
        DungeonProgress::Open { finished } => u8::try_from(finished).ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    /// Five dungeons in the library's tables have monsters with no damage
    /// figures at all - the whole twister and portal among them. The
    /// generated code fills those in as zero, so a simulation against them
    /// says the enemy never lands a hit and the app reported 100 %. That is
    /// the number that sent the free dungeon fight into the twister every
    /// hour, and lost it every time
    #[test]
    fn an_enemy_with_no_damage_figures_is_a_data_gap_not_a_harmless_enemy() {
        use sf_api::gamestate::dungeons::{
            Dungeon, DungeonProgress, ShadowDungeon, get_dungeon_monster,
        };
        let first = get_dungeon_monster(
            Dungeon::Shadow(ShadowDungeon::Twister),
            DungeonProgress::Open { finished: 0 },
        )
        .expect("the twister has enemies");
        assert_eq!(
            (first.min_dmg, first.max_dmg),
            (0, 0),
            "if this ever stops being zero the library gained the real \
             numbers and the guard in compute_chances can go"
        );
        assert!(first.hp > 0, "it is not a harmless enemy - it has health");
    }
}
