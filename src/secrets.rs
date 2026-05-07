use anyhow::{Context, Result, anyhow};

const JIRA_TOKEN_KEY: &str = "jira_token";

fn keyring_entry(key: &str) -> Result<keyring::Entry> {
    let username = whoami::username();
    keyring::Entry::new("lazytime", &format!("{}:{}", username, key))
        .context("failed to initialize keyring entry")
}

pub fn store_jira_token(token: &str) -> Result<()> {
    if token.trim().is_empty() {
        clear_jira_token()?;
        return Ok(());
    }
    let entry = keyring_entry(JIRA_TOKEN_KEY)?;
    entry
        .set_password(token)
        .map_err(|err| anyhow!("failed to store jira token in keyring: {err}"))
}

pub fn load_jira_token() -> Result<Option<String>> {
    let entry = keyring_entry(JIRA_TOKEN_KEY)?;
    match entry.get_password() {
        Ok(v) => {
            if v.trim().is_empty() {
                Ok(None)
            } else {
                Ok(Some(v))
            }
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(err) => Err(anyhow!("failed to load jira token from keyring: {err}")),
    }
}

pub fn clear_jira_token() -> Result<()> {
    let entry = keyring_entry(JIRA_TOKEN_KEY)?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(anyhow!("failed to clear jira token from keyring: {err}")),
    }
}
