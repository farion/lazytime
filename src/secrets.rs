use anyhow::{Context, Result, anyhow};

const JIRA_TOKEN_KEY: &str = "jira_token";

fn keyring_entry(key: &str) -> Result<keyring::Entry> {
    tracing::info!(key = key, "secrets: creating keyring entry");
    let username = whoami::username();
    keyring::Entry::new("lazytime", &format!("{}:{}", username, key))
        .context("failed to initialize keyring entry")
}

pub fn store_jira_token(token: &str) -> Result<()> {
    if token.trim().is_empty() {
        tracing::info!("secrets: store token requested with empty value; clearing token");
        clear_jira_token()?;
        return Ok(());
    }
    tracing::info!("secrets: storing jira token in keyring");
    let entry = keyring_entry(JIRA_TOKEN_KEY)?;
    entry
        .set_password(token)
        .map_err(|err| anyhow!("failed to store jira token in keyring: {err}"))
}

pub fn effective_jira_token(config_token: Option<&str>) -> Option<String> {
    let config_token = config_token
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToString::to_string);
    if config_token.is_some() {
        tracing::info!("secrets: using jira token from config fallback");
        return config_token;
    }
    tracing::info!("secrets: loading jira token from keyring");
    load_jira_token().ok().flatten()
}

pub fn persist_jira_token(token: &str, config_token: &mut Option<String>) {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        tracing::info!("secrets: persisting empty jira token (clear)");
        if let Err(err) = clear_jira_token() {
            tracing::warn!("failed to clear jira token from keyring; continuing: {err}");
        }
        *config_token = None;
        return;
    }

    tracing::info!("secrets: persisting jira token to keyring");
    match store_jira_token(trimmed) {
        Ok(()) => {
            *config_token = None;
        }
        Err(err) => {
            tracing::warn!(
                "failed to store jira token in keyring; using config fallback: {err}"
            );
            *config_token = Some(trimmed.to_string());
        }
    }
}

pub fn load_jira_token() -> Result<Option<String>> {
    tracing::info!("secrets: load jira token from keyring start");
    let entry = keyring_entry(JIRA_TOKEN_KEY)?;
    match entry.get_password() {
        Ok(v) => {
            tracing::info!("secrets: load jira token from keyring done");
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
    tracing::info!("secrets: clear jira token from keyring start");
    let entry = keyring_entry(JIRA_TOKEN_KEY)?;
    match entry.delete_credential() {
        Ok(()) => {
            tracing::info!("secrets: clear jira token from keyring done");
            Ok(())
        }
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(anyhow!("failed to clear jira token from keyring: {err}")),
    }
}
