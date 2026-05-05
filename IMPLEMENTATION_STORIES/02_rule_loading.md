Title: Rule Loading & In-Memory Cache

Summary
Load projects and project_rules from the database at startup, compile regexes, and maintain an in-memory cache used by the detection engine. Provide a reload trigger when config_store.projects_updated_at changes.

Acceptance Criteria
- At startup the daemon loads all rules and compiles regex patterns; errors in regexes prevent startup with a clear error message.
- The compiled rules are stored in an efficient in-memory structure optimized for matching by app_id/instance/class and ordered by precedence.
- The daemon reloads rules when config_store.projects_updated_at timestamp is updated.

Tasks
1. Implement a loader module (src/rules.rs) that reads projects and project_rules and returns compiled structures.
2. Define an in-memory RuleSet structure: map from app_id -> Vec<Rule> and fallback lists for instance/class.
3. Implement reload() that checks config_store.projects_updated_at and replaces in-memory cache atomically.
4. Add unit tests: invalid regex -> error, precedence ordering, reload updates cache.

Files/Modules
- src/rules.rs

Estimate: 3-4 hours
