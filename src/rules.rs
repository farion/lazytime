use anyhow::{Result, anyhow};
use regex::Regex;
use rusqlite::Connection;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::db;
use crate::platform::types::WindowEventInfo;

#[derive(Debug, Clone)]
pub struct CompiledRule {
    pub project_name: String,
    pub sap_number: Option<String>,
    pub app_id: Option<String>,
    pub name_regex: Regex,
    pub precedence: i64,
}

#[derive(Debug, Clone, Default)]
pub struct RuleSet {
    pub app_rules: HashMap<String, Vec<CompiledRule>>,
    pub fallback_rules: Vec<CompiledRule>,
}

impl RuleSet {
    pub fn detect_project(&self, event: &WindowEventInfo) -> Option<String> {
        // Primary: if we have an app_id from the event, try rules keyed by that app_id
        if let Some(app_id) = &event.app_id {
            if let Some(rules) = self.app_rules.get(app_id) {
                for rule in rules {
                    if rule.name_regex.is_match(&event.title) {
                        return Some(rule.project_name.clone());
                    }
                }
            }
        } else {
            // If no app_id provided by the window system, try matching the
            // stored app_id values against the instance or class reported by
            // the window system. This preserves behavior where some
            // environments don't provide a canonical app_id but do provide an
            // instance/class value.
            if let Some(instance) = &event.instance {
                for (stored_app_id, rules) in &self.app_rules {
                    if stored_app_id == instance {
                        for rule in rules {
                            if rule.name_regex.is_match(&event.title) {
                                return Some(rule.project_name.clone());
                            }
                        }
                    }
                }
            }
            if let Some(class) = &event.class {
                for (stored_app_id, rules) in &self.app_rules {
                    if stored_app_id == class {
                        for rule in rules {
                            if rule.name_regex.is_match(&event.title) {
                                return Some(rule.project_name.clone());
                            }
                        }
                    }
                }
            }
        }

        // Finally, try fallback rules that have no app_id (apply regardless
        // of instance/class). These are general title-based rules.
        for rule in &self.fallback_rules {
            if rule.name_regex.is_match(&event.title) {
                return Some(rule.project_name.clone());
            }
        }

        None
    }
}

pub fn load_rules(conn: &Connection) -> Result<RuleSet> {
    let projects = db::projects(conn)?;
    let rules = db::project_rules(conn)?;

    let mut project_map = HashMap::new();
    for p in projects {
        project_map.insert(p.id, (p.name, p.sap_number));
    }

    let mut app_rules: HashMap<String, Vec<CompiledRule>> = HashMap::new();
    let mut fallback_rules = Vec::new();

    for rule in rules {
        let (project_name, sap_number) = project_map
            .get(&rule.project_id)
            .cloned()
            .ok_or_else(|| anyhow!("missing project id {}", rule.project_id))?;

        let compiled = Regex::new(&rule.name_regex)
            .map_err(|e| anyhow!("invalid regex {}: {}", rule.name_regex, e))?;

        let compiled_rule = CompiledRule {
            project_name,
            sap_number,
            app_id: rule.app_id.clone(),
            name_regex: compiled,
            precedence: rule.precedence,
        };

        if let Some(app_id) = &rule.app_id {
            if app_id.trim() == "*" {
                fallback_rules.push(compiled_rule);
                continue;
            }
            app_rules
                .entry(app_id.clone())
                .or_default()
                .push(compiled_rule);
        } else {
            fallback_rules.push(compiled_rule);
        }
    }

    for rules in app_rules.values_mut() {
        rules.sort_by_key(|r| r.precedence);
    }
    fallback_rules.sort_by_key(|r| r.precedence);

    Ok(RuleSet {
        app_rules,
        fallback_rules,
    })
}

#[derive(Clone, Default)]
pub struct RuleCache {
    inner: Arc<RwLock<RuleSet>>,
}

impl RuleCache {
    pub async fn get(&self) -> RuleSet {
        self.inner.read().await.clone()
    }

    pub async fn replace(&self, new_rules: RuleSet) {
        *self.inner.write().await = new_rules;
    }
}
