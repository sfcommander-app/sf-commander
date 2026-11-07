//! Windows Credential Manager boundary for remembered game-account hashes.
//!
//! `PWHash` is replayable login material.  It used to be serialized into
//! `helper.toml`, which made every settings backup a credential backup too.
//! The TOML now keeps account identity and preferences only; this module owns
//! the secret and deliberately fails closed when the OS vault is unavailable.

use sf_api::session::PWHash;
use sha2::{Digest, Sha256};

use crate::config::{AccountConfig, Config};

#[cfg(not(test))]
const SERVICE: &str = "SF Commander";

fn target(kind: &str, name: &str, server: Option<&str>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(kind.as_bytes());
    hasher.update([0]);
    hasher.update(name.trim().to_lowercase().as_bytes());
    hasher.update([0]);
    if let Some(server) = server {
        hasher.update(server.trim().to_lowercase().as_bytes());
    }
    format!("account-{:x}", hasher.finalize())
}

fn account_target(account: &AccountConfig) -> String {
    match account {
        AccountConfig::Regular { name, server, .. } => {
            target("regular", name, Some(server))
        }
        AccountConfig::SF { name, .. } => target("sf-account", name, None),
    }
}

fn hash(account: &AccountConfig) -> &PWHash {
    match account {
        AccountConfig::Regular { pw_hash, .. }
        | AccountConfig::SF { pw_hash, .. } => pw_hash,
    }
}

fn hash_mut(account: &mut AccountConfig) -> &mut PWHash {
    match account {
        AccountConfig::Regular { pw_hash, .. }
        | AccountConfig::SF { pw_hash, .. } => pw_hash,
    }
}

#[cfg(not(test))]
fn store(target: &str, value: &str) -> Result<(), String> {
    use keyring::v1::Entry;
    Entry::new(SERVICE, target)
        .map_err(|_| "Windows Credential Manager is unavailable".to_string())?
        .set_password(value)
        .map_err(|_| {
            "Could not save the remembered login in Windows Credential Manager"
                .to_string()
        })
}

#[cfg(not(test))]
fn load(target: &str) -> Result<Option<String>, String> {
    use keyring::v1::{Entry, Error};
    match Entry::new(SERVICE, target)
        .map_err(|_| "Windows Credential Manager is unavailable".to_string())?
        .get_password()
    {
        Ok(value) => Ok(Some(value)),
        Err(Error::NoEntry) => Ok(None),
        Err(_) => Err(
            "Could not read the remembered login from Windows Credential Manager"
                .to_string(),
        ),
    }
}

#[cfg(test)]
fn vault(
) -> &'static std::sync::Mutex<std::collections::HashMap<String, String>> {
    static VAULT: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, String>>,
    > = std::sync::OnceLock::new();
    VAULT.get_or_init(Default::default)
}

#[cfg(test)]
fn store(target: &str, value: &str) -> Result<(), String> {
    vault()
        .lock()
        .map_err(|_| "test credential vault is poisoned".to_string())?
        .insert(target.to_string(), value.to_string());
    Ok(())
}

#[cfg(test)]
fn load(target: &str) -> Result<Option<String>, String> {
    Ok(vault()
        .lock()
        .map_err(|_| "test credential vault is poisoned".to_string())?
        .get(target)
        .cloned())
}

/// Stores every in-memory credential before a settings file can be written.
/// The caller must serialize only after this succeeds.
pub fn persist(config: &Config) -> Result<(), String> {
    for account in &config.accounts {
        let value = hash(account).get();
        if value.is_empty() {
            return Err(format!(
                "No Windows Credential Manager entry exists for {}",
                account_target(account)
            ));
        }
        store(&account_target(account), value)?;
    }
    Ok(())
}

/// Loads credentials omitted from new TOML files and migrates credentials
/// still present in a legacy file. Returns whether legacy material was found.
pub fn hydrate(config: &mut Config) -> Result<bool, String> {
    let mut migrated = false;
    for account in &mut config.accounts {
        let target = account_target(account);
        if hash(account).get().is_empty() {
            let value = load(&target)?.ok_or_else(|| {
                "A remembered account has no matching Windows Credential Manager entry"
                    .to_string()
            })?;
            *hash_mut(account) = PWHash::from_hash(value);
        } else {
            store(&target, hash(account).get())?;
            migrated = true;
        }
    }
    Ok(migrated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AccountConfig, AccountCreds};

    #[test]
    fn credential_target_does_not_expose_account_identity() {
        let account = AccountConfig::new(AccountCreds::Regular {
            name: "visible@example.test".into(),
            pw_hash: PWHash::from_hash("secret-hash".into()),
            server: "s1.example.test".into(),
        });
        let target = account_target(&account);
        assert!(!target.contains("visible"));
        assert!(!target.contains("example"));
    }

    #[test]
    fn helper_toml_omits_replayable_password_hash() {
        let mut config = Config::default();
        config
            .accounts
            .push(AccountConfig::new(AccountCreds::Regular {
                name: "player".into(),
                pw_hash: PWHash::from_hash("replayable-secret-hash".into()),
                server: "s1.example.test".into(),
            }));

        let serialized = toml::to_string(&config).expect("serialize config");
        assert!(!serialized.contains("replayable-secret-hash"));
        assert!(!serialized.contains("pw_hash"));
    }

    #[test]
    fn new_toml_is_hydrated_from_the_os_vault() {
        let mut config = Config::default();
        config.accounts.push(AccountConfig::new(AccountCreds::SF {
            name: "account".into(),
            pw_hash: PWHash::from_hash("stored-hash".into()),
        }));
        persist(&config).expect("store credential");

        let serialized = toml::to_string(&config).expect("serialize config");
        let mut restored: Config = toml::from_str(&serialized).expect("parse");
        assert_eq!(hash(&restored.accounts[0]).get(), "");
        assert!(!hydrate(&mut restored).expect("hydrate"));
        assert_eq!(hash(&restored.accounts[0]).get(), "stored-hash");
    }
}
