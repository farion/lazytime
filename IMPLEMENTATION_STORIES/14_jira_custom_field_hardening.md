Title: Jira Payload Hardening for sap_project Custom Field

Summary
Harden Jira integration to support real Jira custom field configuration for `sap_project` and make payload/JQL generation configurable per instance.

Acceptance Criteria
- Jira sync supports configurable custom field id/name for sap_project matching.
- Issue search JQL and create payload use configured field mapping correctly.
- Sync handles Jira validation errors gracefully and surfaces actionable messages.
- Added dry-run mode for Jira sync to verify payload/JQL without writing.

Tasks
1. Extend config with Jira custom field settings (e.g. jira_sap_field_id, jira_issue_type).
2. Update search JQL builder to use configured field key.
3. Update create issue payload to include configured custom field where required.
4. Improve error handling and response logging for Jira API failures.
5. Add tests with mocked Jira responses for success and validation-failure scenarios.

Files/Modules
- src/config.rs
- src/jira.rs
- src/main.rs
- tests/ (new jira-focused tests)

Estimate: 6-12 hours
