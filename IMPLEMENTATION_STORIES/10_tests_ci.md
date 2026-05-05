Title: Tests & CI

Summary
Add unit and integration tests for DB migrations, rule loading, detection logic, and CLI output. Configure a basic CI workflow to run tests.

Acceptance Criteria
- Tests cover DB init, rule loading (valid/invalid regex), detection debounce logic, and CLI summary formatting.
- CI runs cargo test on PRs and main branch.

Tasks
1. Add unit tests for src/db.rs migration functions.
2. Add unit tests for src/rules.rs compiling regexes and precedence.
3. Add integration tests for the daemon state machine (using a simulated event source) to verify debounce and tracking creation.
4. Add GitHub Actions workflow (/.github/workflows/ci.yml) running cargo test.

Files/Modules
- tests/* (integration)
- .github/workflows/ci.yml

Estimate: 4-8 hours
