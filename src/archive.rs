//! The guild chat, written to a text file as it happens.
//!
//! The game keeps a day or two of chat and then it is gone. This keeps
//! all of it: one file per guild, on the desktop, a line appended every
//! time a new one turns up. Switched on in the chat tab; nothing is
//! written until it is.
//!
//! The game's chat carries a time of day and no date, and hands over the
//! same last few dozen lines on every refresh. So two things are done
//! here: the date is worked out (a time later than now is yesterday's),
//! and a line is written only once - the last few hundred written lines
//! are remembered and anything already among them is skipped.

use std::{
    collections::VecDeque,
    io::Write,
    path::{Path, PathBuf},
};

use chrono::{DateTime, Local, NaiveDate, NaiveTime};
use sf_api::gamestate::guild::ChatMessage;

/// How many written lines are remembered per guild for the "already
/// written" check. The game hands over fewer than this per refresh, so a
/// line that has scrolled out of the game's own window is never seen
/// again anyway
pub const REMEMBERED: usize = 400;

/// One line as it goes into the file: date, time, who, what. The game's
/// own lines (a level up, somebody joining) have no time and no author;
/// they get the date and the sentence, written the way the chat tab
/// writes it
#[must_use]
pub fn format_line(msg: &ChatMessage, date: NaiveDate) -> String {
    let body = crate::chat::decode(&msg.message)
        .unwrap_or_else(|| msg.message.clone());
    match msg.time {
        Some(t) if !msg.user.is_empty() => {
            format!("[{} {}] {}: {body}", date, t.format("%H:%M"), msg.user)
        }
        Some(t) => format!("[{} {}] {body}", date, t.format("%H:%M")),
        None => format!("[{date}] {body}"),
    }
}

/// The date a chat line belongs to. The chat only says the time of day;
/// a time later than now cannot be today's, so it was yesterday's
#[must_use]
pub fn date_of(time: Option<NaiveTime>, now: DateTime<Local>) -> NaiveDate {
    let today = now.date_naive();
    match time {
        Some(t) if t > now.time() => today.pred_opt().unwrap_or(today),
        _ => today,
    }
}

/// The lines in `chat` (newest first, as the game sends it) that have not
/// been written yet, oldest first, and the memory brought up to date
#[must_use]
pub fn new_lines(
    seen: &mut VecDeque<String>,
    chat: &[ChatMessage],
    now: DateTime<Local>,
) -> Vec<String> {
    let mut out = Vec::new();
    for msg in chat.iter().rev() {
        let line = format_line(msg, date_of(msg.time, now));
        if seen.contains(&line) {
            continue;
        }
        seen.push_back(line.clone());
        while seen.len() > REMEMBERED {
            seen.pop_front();
        }
        out.push(line);
    }
    out
}

/// Where the file goes: the desktop, or the folder the app runs from when
/// there is no desktop to be found
#[must_use]
pub fn chat_file(guild: &str) -> PathBuf {
    let safe: String = guild
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' { c } else { '_' })
        .collect();
    desktop_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(format!("SF Commander - chat {}.txt", safe.trim()))
}

/// Append the lines, one per row. A failure is returned, not swallowed:
/// the chat tab says when the file cannot be written
pub fn append(path: &Path, lines: &[String]) -> std::io::Result<()> {
    if lines.is_empty() {
        return Ok(());
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    for l in lines {
        writeln!(f, "{l}")?;
    }
    Ok(())
}

/// The last `n` lines of the file, oldest first - nothing when there is no
/// file yet
#[must_use]
pub fn read_tail(path: &Path, n: usize) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let lines: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
    let skip = lines.len().saturating_sub(n);
    lines[skip..].iter().map(|l| (*l).to_string()).collect()
}

/// Whether an archived line is one of the game's own - a level up, a hit
/// on the portal, somebody joining - rather than somebody talking. A
/// person's line carries a time and a name: "[2026-09-19 14:16] Zaiyfu:
/// ...". The game's carries the date alone: "[2026-09-19] Hrac ..."
#[must_use]
pub fn is_games_own(line: &str) -> bool {
    match line.find(']') {
        Some(end) => !line[..end].contains(':'),
        None => true,
    }
}

/// The desktop folder. On Windows it is asked of the registry, because a
/// desktop moved into OneDrive is not `%USERPROFILE%\Desktop` any more;
/// that is the fallback. Elsewhere `~/Desktop`
#[must_use]
pub fn desktop_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        if let Some(p) = windows_desktop() {
            return Some(p);
        }
        return std::env::var_os("USERPROFILE")
            .map(|h| PathBuf::from(h).join("Desktop"));
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Desktop"))
    }
}

#[cfg(windows)]
fn windows_desktop() -> Option<PathBuf> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new("reg")
        .args([
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders",
            "/v",
            "Desktop",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    // "    Desktop    REG_EXPAND_SZ    %USERPROFILE%\Desktop" - the value
    // is everything after the type, and may carry %VARS%
    let line = text.lines().find(|l| l.trim_start().starts_with("Desktop"))?;
    let mut parts = line.split_whitespace();
    parts.next()?; // Desktop
    parts.next()?; // REG_EXPAND_SZ
    let raw = parts.collect::<Vec<_>>().join(" ");
    if raw.is_empty() {
        return None;
    }
    let expanded = expand_env(&raw);
    let p = PathBuf::from(expanded);
    p.is_dir().then_some(p)
}

/// `%USERPROFILE%\Desktop` with the variables filled in
#[cfg(windows)]
fn expand_env(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) => {
                let var = &after[..end];
                out.push_str(&std::env::var(var).unwrap_or_default());
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(user: &str, hhmm: Option<&str>, text: &str) -> ChatMessage {
        ChatMessage {
            user: user.into(),
            time: hhmm.map(|s| NaiveTime::parse_from_str(s, "%H:%M").unwrap()),
            message: text.into(),
        }
    }

    fn at(hhmm: &str) -> DateTime<Local> {
        Local::now()
            .date_naive()
            .and_time(NaiveTime::parse_from_str(hhmm, "%H:%M").unwrap())
            .and_local_timezone(Local)
            .unwrap()
    }

    /// The game hands over the same lines on every refresh; each goes in
    /// the file once
    #[test]
    fn a_line_is_written_once_whatever_the_game_repeats() {
        let mut seen = VecDeque::new();
        let chat = vec![
            msg("Petr", Some("14:16"), "ahoj"),
            msg("Zaiyfu", Some("14:10"), "hi"),
        ];
        let first = new_lines(&mut seen, &chat, at("14:20"));
        assert_eq!(first.len(), 2);
        // Oldest first, the way a file reads
        assert!(first[0].contains("Zaiyfu"));
        let again = new_lines(&mut seen, &chat, at("14:35"));
        assert!(again.is_empty(), "written twice: {again:?}");
        // A new line on top: only that one
        let chat = vec![msg("Nyx", Some("14:30"), "raid?")]
            .into_iter()
            .chain(chat)
            .collect::<Vec<_>>();
        let third = new_lines(&mut seen, &chat, at("14:35"));
        assert_eq!(third.len(), 1);
        assert!(third[0].contains("Nyx"));
    }

    /// The chat says 23:58; it is 00:03. That line is yesterday's
    #[test]
    fn a_time_later_than_now_is_yesterdays() {
        let now = at("00:03");
        let yesterday = now.date_naive().pred_opt().unwrap();
        assert_eq!(
            date_of(Some(NaiveTime::parse_from_str("23:58", "%H:%M").unwrap()), now),
            yesterday
        );
        assert_eq!(
            date_of(Some(NaiveTime::parse_from_str("00:01", "%H:%M").unwrap()), now),
            now.date_naive()
        );
        assert_eq!(date_of(None, now), now.date_naive());
    }

    /// The game's own lines have no author and no time and still go in
    #[test]
    fn the_games_own_lines_go_in_with_the_date_only() {
        let now = at("10:00");
        let line = format_line(
            &msg("", None, "Hrac KAMSON87 dosahl urovne 206."),
            now.date_naive(),
        );
        assert!(line.starts_with(&format!("[{}] ", now.date_naive())));
        assert!(line.ends_with("206."));
        let said = format_line(&msg("Petr", Some("09:59"), "ahoj"), now.date_naive());
        assert!(said.ends_with("09:59] Petr: ahoj"));
    }

    /// "kopirovat stary veci z chatu, treba ten spam level upu" - the old
    /// lines come back out of the file, and the game's own are told apart
    /// so they can be hidden
    #[test]
    fn old_lines_come_back_and_the_games_own_are_told_apart() {
        let dir = std::env::temp_dir().join(format!("sfc-archive-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("chat.txt");
        let now = at("14:20");
        let lines = vec![
            format_line(&msg("Petr", Some("14:16"), "ahoj"), now.date_naive()),
            format_line(&msg("", None, "Hrac Bohdan dosahl urovne 186."), now.date_naive()),
            format_line(&msg("Nyx", Some("14:18"), "raid?"), now.date_naive()),
        ];
        append(&path, &lines).unwrap();
        append(&path, &lines[..1]).unwrap();
        // Four in the file; the last three are Bohdan, Nyx, Petr
        let back = read_tail(&path, 3);
        assert_eq!(back.len(), 3);
        assert_eq!(back, vec![lines[1].clone(), lines[2].clone(), lines[0].clone()]);
        assert!(is_games_own(&back[0]));
        assert!(!is_games_own(&back[1]));
        assert!(!is_games_own(&back[2]));
        // Nothing yet: nothing, not a panic
        assert!(read_tail(&dir.join("none.txt"), 10).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_file_name_is_safe_for_any_guild_name() {
        let p = chat_file("Imperium / s30: \"best\"");
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        assert!(name.starts_with("SF Commander - chat Imperium"));
        assert!(!name.contains('/') && !name.contains(':') && !name.contains('"'));
        assert!(name.ends_with(".txt"));
    }
}
