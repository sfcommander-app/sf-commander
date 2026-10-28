use iced::{
    Alignment, Element, Length,
    widget::{column, scrollable},
};
use super::glyphs::ticked as checkbox;
// His lettering stands in for the toolkit's `text`, so every line on
// this screen is set in the face the purse card is
use super::glyphs::lettered as text;

use crate::i18n::t;
use crate::util::LockExt;
use crate::{
    config::Config,
    message::Message,
    player::{AccountInfo, AccountStatus},
    server::ServerInfo,
};

pub fn view_options<'a>(
    player: &'a AccountInfo,
    og_server: &'a ServerInfo,
    config: &'a Config,
) -> Element<'a, Message> {
    let char_conf = config.get_char_conf(&player.name, og_server.ident.id);

    let mut all = column!().spacing(14).width(Length::Fill);

    // ------------------------------------------------------------- sounds
    // App-wide, so it lives here now instead of in every character's header
    all = all.push(text(t("Sounds")).size(15));
    all = all.push(
        checkbox(t("Play a sound on level-ups and epic finds"), config.sounds)
            .style(crate::ui::style::check())
            .on_toggle(Message::SetSounds)
            .text_size(13),
    );

    // ------------------------------------------------------ saved login
    all = all.push(text(t("On startup")).size(15));

    let Some(conf) = char_conf else {
        all = all.push(
            text(t(
                "This character was not logged in with 'Remember me', so \
                 there is nothing to start automatically. Everything else in \
                 the app still works for this session.",
            ))
            .size(12),
        );
        return scrollable(
            column!(all)
                .padding(20)
                .width(Length::Fill)
                .align_items(Alignment::Center),
        )
        .height(Length::Fill)
        .into();
    };

    let name = player.name.clone();
    let server = og_server.ident.id;

    all = all.push(
        checkbox(t("Log in automatically on startup"), conf.login)
            .style(crate::ui::style::check())
            .on_toggle({
                let name = name.clone();
                move |nv| Message::ConfigSetAutoLogin {
                    name: name.clone(),
                    server,
                    nv,
                }
            })
            .text_size(13),
    );
    all = all.push(
        checkbox(t("Start arena auto-battle after login"), conf.auto_battle)
            .style(crate::ui::style::check())
            .on_toggle({
                let name = name.clone();
                move |nv| Message::ConfigSetAutoBattle {
                    name: name.clone(),
                    server,
                    nv,
                }
            })
            .text_size(13),
    );
    all = all.push(
        checkbox(t("Start underworld auto-lure after login"), conf.auto_lure)
            .style(crate::ui::style::check())
            .on_toggle({
                let name = name.clone();
                move |nv| Message::ConfigSetAutoLure {
                    name: name.clone(),
                    server,
                    nv,
                }
            })
            .text_size(13),
    );
    all = all.push(
        checkbox(
            t("Start automatic fortress attacks after login"),
            conf.auto_fortress,
        )
        .style(crate::ui::style::check())
        .on_toggle({
            let name = name.clone();
            move |nv| Message::ConfigSetAutoFortress {
                name: name.clone(),
                server,
                nv,
            }
        })
        .text_size(13),
    );
    all = all.push(
        text(t(
            "Everything else (dungeons, guild sign-up, fortress housekeeping, \
             pets, Arena Manager) is remembered automatically when you switch \
             it on.",
        ))
        .size(11)
        .style(iced::theme::Text::Color(crate::ui::style::MUTED)),
    );

    // ------------------------------------------------- character maxxing
    // Gear, companions, backpack and attributes in one place - the same
    // controls as on the Shops page, so nothing has to be hunted for
    all = all.push(text(t("Character maxxing")).size(15));
    all = all.push(
        text(t(
            "Gear first, then attributes: the shop step buys the best piece \
             the gold covers (and saves for one it does not), the attributes \
             take what is left above the reserve. Bert, Mark and Kunigunde \
             are dressed from the backpack.",
        ))
        .size(11)
        .style(iced::theme::Text::Color(crate::ui::style::MUTED)),
    );
    {
        let lock = player.status.lock_safe();
        if let AccountStatus::Idle(_, gs) | AccountStatus::Busy(gs, _) = &*lock
        {
            all = all.push(super::shops::maxxing_settings(player, gs));
        }
    }

    crate::ui::style::page(all)
}
