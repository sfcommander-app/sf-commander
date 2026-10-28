//! The two sounds the app makes.
//!
//! Both are baked into the exe, so there is nothing extra to hand out and
//! nothing to go missing on somebody else's machine. They are CC0 from
//! freesound.org - public domain, no attribution needed, redistribution in a
//! compiled program explicitly allowed. That last part is why they are not
//! the game's own sounds: those belong to the people who made the game.
//!
//! Nothing here is allowed to matter. The bot runs unattended for hours on
//! machines with no sound card, over remote desktop, with the audio device
//! taken by something else - so every failure is swallowed and the caller
//! never waits for any of it.

use std::io::Cursor;

/// The message went out - short and quiet
static MESSAGE: &[u8] = include_bytes!("../assets/sounds/msg.wav");
/// Something worth looking up for: a level, a cleared dungeon, a day's
/// tavern finished
static CELEBRATE: &[u8] = include_bytes!("../assets/sounds/celebrate.wav");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sound {
    Message,
    Celebrate,
}

impl Sound {
    const fn bytes(self) -> &'static [u8] {
        match self {
            Sound::Message => MESSAGE,
            Sound::Celebrate => CELEBRATE,
        }
    }
}

/// Plays one sound and returns immediately. A missing or busy audio device
/// is not an error worth telling anybody about - it just stays quiet
pub fn play(sound: Sound) {
    std::thread::Builder::new()
        .name("sound".into())
        .spawn(move || {
            // The stream has to outlive the playback, which is why this
            // whole thing sits on its own thread instead of being fired and
            // forgotten from the UI one
            let Ok(stream) = rodio::OutputStreamBuilder::open_default_stream()
            else {
                log::debug!("no audio device - staying quiet");
                return;
            };
            let sink = rodio::Sink::connect_new(stream.mixer());
            // A third of full volume (5.4) - "much too loud" next to
            // everything else on the machine
            sink.set_volume(0.35);
            match rodio::Decoder::new(Cursor::new(sound.bytes())) {
                Ok(decoded) => {
                    sink.append(decoded);
                    sink.sleep_until_end();
                }
                Err(e) => log::debug!("could not decode {sound:?}: {e}"),
            }
        })
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sound that will not decode is a sound nobody ever hears, and the
    /// failure would only show up on somebody else's machine at three in the
    /// morning. Both files are checked here instead
    #[test]
    fn both_sounds_are_wav_files_that_decode() {
        for sound in [Sound::Message, Sound::Celebrate] {
            let bytes = sound.bytes();
            assert!(
                bytes.starts_with(b"RIFF") && bytes[8..12] == *b"WAVE",
                "{sound:?} is not a wav file"
            );
            assert!(
                rodio::Decoder::new(Cursor::new(bytes)).is_ok(),
                "{sound:?} does not decode"
            );
        }
    }
}
