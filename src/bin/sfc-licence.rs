//! The author's own licence desk. **This is not shipped.**
//!
//! It is a second binary in the same crate so that it shares
//! `src/licence.rs` with the app - one copy of the canonical format, so the
//! thing that signs and the thing that checks can never drift apart. It is
//! built with `cargo build --bin sfc-licence` and stays on the author's
//! machine; `cargo build --bin sf-commander` does not produce it and the
//! release that goes to the guild does not contain it.
//!
//! The private key is never in this file, never in the repository, and
//! never inside any binary. It is a file on disk that this tool reads.
//!
//! ```text
//!   sfc-licence keygen  key.pk8       # once, ever. Back this file up.
//!   sfc-licence issue   key.pk8 --machine K7QM-3H2P-9XRF-2B4 --to "Petr"
//!   sfc-licence inspect <code>
//! ```

use std::{fs, path::Path, process::ExitCode};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD as B64};
use ring::signature::KeyPair as _;

#[path = "../licence.rs"]
mod licence;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        ["keygen", path] => keygen(Path::new(path)),
        ["issue", key, rest @ ..] => issue(Path::new(key), rest),
        ["inspect", token] => inspect(token, None),
        ["inspect", token, key] => inspect(token, Some(Path::new(key))),
        _ => {
            eprintln!("{}", usage());
            return ExitCode::FAILURE;
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn usage() -> String {
    "\
sfc-licence - the author's licence desk (never ship this binary)

  sfc-licence keygen <key.pk8>
      Make the one and only signing key. Refuses to overwrite an existing
      file: losing this key means every licence ever issued becomes
      unrenewable, so it is a file to back up and to keep off any machine
      that other people can reach.

  sfc-licence issue <key.pk8> --machine <CODE> --to <NAME> [--days 365]
                              [--id <SERIAL>]
      Print one licence code. The machine code is the one the customer
      reads off the app's Settings page.

  sfc-licence inspect <code> [key.pk8]
      Say what a code contains. Given the key file it also checks the
      signature, which is how to check a code before the public half has
      been pasted into the app and rebuilt."
        .to_string()
}

/// Make the signing key. Once, and then never again - every licence ever
/// issued is tied to it
fn keygen(path: &Path) -> Result<(), String> {
    if path.exists() {
        return Err(format!(
            "{} already exists. Refusing to overwrite a signing key - if \
             this really is meant to be a new one, move the old file away \
             yourself, knowing every licence it signed stops being \
             renewable",
            path.display()
        ));
    }
    let rng = ring::rand::SystemRandom::new();
    let pkcs8 = ring::signature::Ed25519KeyPair::generate_pkcs8(&rng)
        .map_err(|_| "the system would not give us randomness".to_string())?;
    let pair = ring::signature::Ed25519KeyPair::from_pkcs8(pkcs8.as_ref())
        .map_err(|_| "generated a key we cannot read back".to_string())?;
    write_private(path, pkcs8.as_ref())?;

    let public = pair.public_key().as_ref();
    println!("Signing key written to {}", path.display());
    println!(
        "\nBack that file up somewhere safe and offline. It is the only \
         thing that can issue or renew a licence, and there is no way to \
         recover it.\n"
    );
    println!("Now paste this into src/licence.rs, replacing PUBLIC_KEY:\n");
    println!("pub const PUBLIC_KEY: [u8; 32] = [");
    for row in public.chunks(8) {
        let cells: Vec<String> =
            row.iter().map(|b| format!("0x{b:02x},")).collect();
        println!("    {}", cells.join(" "));
    }
    println!("];");
    Ok(())
}

/// Write the key with no more permission than the owner needs
fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    fs::write(path, bytes).map_err(|e| format!("cannot write the key: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn issue(key_path: &Path, rest: &[&str]) -> Result<(), String> {
    let mut machine = None;
    let mut holder = None;
    let mut days = licence::TERM_DAYS;
    let mut id = None;
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        let mut value = || {
            it.next()
                .copied()
                .ok_or_else(|| format!("{flag} needs a value"))
        };
        match *flag {
            "--machine" => machine = Some(value()?.to_string()),
            "--to" => holder = Some(value()?.to_string()),
            "--id" => id = Some(value()?.to_string()),
            "--days" => {
                days = value()?
                    .parse()
                    .map_err(|_| "--days wants a number".to_string())?;
            }
            other => return Err(format!("unknown option {other}")),
        }
    }
    let machine = machine.ok_or("--machine is required")?;
    let holder = holder.ok_or("--to is required")?;
    // The separator is the one character the format cannot carry, because
    // it is what holds the fields apart
    if [&machine, &holder].iter().any(|s| s.contains('|')) {
        return Err("a name or machine code cannot contain '|'".into());
    }

    let pkcs8 = fs::read(key_path)
        .map_err(|e| format!("cannot read {}: {e}", key_path.display()))?;
    let pair = ring::signature::Ed25519KeyPair::from_pkcs8(&pkcs8)
        .map_err(|_| format!("{} is not a signing key", key_path.display()))?;

    let now = licence::now_unix();
    let l = licence::Licence {
        id: id.unwrap_or_else(|| serial(now, &machine)),
        machine,
        holder,
        issued: now,
        expires: now + days * 86_400,
    };
    let sig = pair.sign(l.payload().as_bytes());
    let token = l.token(sig.as_ref());

    // Read it back with the public half, exactly as the app will. An
    // unverifiable code must never leave this desk
    let mut public = [0u8; 32];
    public.copy_from_slice(pair.public_key().as_ref());
    licence::verify(&token, &public)
        .map_err(|e| format!("refusing to hand out a code that fails: {e}"))?;

    eprintln!(
        "licence {} for {} on {}, {} days, runs out {}",
        l.id,
        l.holder,
        l.machine,
        days,
        stamp(l.expires)
    );
    // The code alone on stdout, so it can be piped or copied cleanly
    println!("{token}");
    Ok(())
}

fn inspect(token: &str, key_path: Option<&Path>) -> Result<(), String> {
    let cleaned: String =
        token.chars().filter(|c| !c.is_whitespace()).collect();
    let payload_b64 = cleaned.split('.').next().unwrap_or_default();
    let bytes = B64
        .decode(payload_b64)
        .map_err(|_| "that is not a licence code".to_string())?;
    let text =
        String::from_utf8(bytes).map_err(|_| "not readable".to_string())?;
    println!("contents: {text}");
    // Against the key file if one was given, otherwise against whatever
    // this build carries - which before the first keygen is nothing
    let mut public = licence::PUBLIC_KEY;
    if let Some(path) = key_path {
        let pkcs8 = fs::read(path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let pair = ring::signature::Ed25519KeyPair::from_pkcs8(&pkcs8)
            .map_err(|_| format!("{} is not a signing key", path.display()))?;
        public.copy_from_slice(pair.public_key().as_ref());
    }
    match licence::verify(&cleaned, &public) {
        Ok(l) => {
            println!(
                "signature: good\nholder:    {}\nmachine:   {}\nissued:    \
                 {}\nexpires:   {}",
                l.holder,
                l.machine,
                stamp(l.issued),
                stamp(l.expires)
            );
        }
        Err(e) => println!("signature: {e}"),
    }
    Ok(())
}

/// A serial that is unique per machine and issue, and readable in a message
fn serial(now: i64, machine: &str) -> String {
    let digest = ring::digest::digest(
        &ring::digest::SHA256,
        format!("{now}{machine}").as_bytes(),
    );
    let short: String = digest
        .as_ref()
        .iter()
        .take(3)
        .map(|b| format!("{b:02X}"))
        .collect();
    format!("L{}-{short}", stamp(now).replace('-', ""))
}

fn stamp(unix: i64) -> String {
    chrono::DateTime::from_timestamp(unix, 0)
        .map_or_else(|| "?".into(), |d| d.format("%Y-%m-%d").to_string())
}
