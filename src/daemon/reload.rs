use anyhow::Result;

use crate::rules::{RuleCache, load_rules};

pub async fn reload_rules_from_db(db_path: &std::path::Path, cache: &RuleCache) -> Result<()> {
    let conn = crate::db::open(db_path)?;
    let new_rules = load_rules(&conn)?;
    cache.replace(new_rules).await;
    Ok(())
}
