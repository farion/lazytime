Title: Jira Sync

Summary
Implement the --jira-sync feature that finds or creates a Jira issue per project (using config.jira_project and project.sap_number) and adds a worklog for each unsynced tracking.

Acceptance Criteria
- `lazytime --jira-sync` attempts to sync unsynced trackings and marks them jira_synced on success.
- If no matching issue exists, an issue is created in config.jira_project and assigned to config.jira_assignee. The created issue key is stored on the tracking row.
- Worklogs are added with the correct durations and timestamps.

Tasks
1. Implement a Jira client module (src/jira.rs) that supports search-issues (JQL), create-issue, add-worklog.
2. Implement --jira-sync loop: for each unsynced tracking with end_ts not null, find/create issue, add worklog, mark synced.
3. Add safe error handling and retry/backoff for API failures.
4. Add unit tests mocking Jira responses.

Files/Modules
- src/jira.rs

Estimate: 6-10 hours
