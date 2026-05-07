# Story 24: First-Run Onboarding (Blocking) + Re-run from Settings

## Summary
Implement a guided onboarding flow that is shown on first start and blocks all normal app UI until completed.

Also add a **"Show onboarding"** action in the **General** settings tab so users can run onboarding again later.

## User goals
- New users can configure LazyTime quickly without needing advanced settings.
- Existing users can re-run onboarding from Settings.

## Scope
1. Add a 6-step onboarding flow in GUI.
2. Show onboarding on first start only (unless manually re-triggered).
3. Prevent any normal app view from showing before onboarding is finished.
4. Persist onboarding results (config + initial project/rules).
5. Add General-tab action to re-run onboarding.
6. Integrate secure Jira credential storage using OS keyring.

## Onboarding steps (required)
1. **How LazyTime works**
   - Explain automatic tracking via window titles + working hours.
   - Explain manual tracking is possible.

2. **Working hours setup (simple)**
   - Offer two ranges:
     - Morning: `08:00-12:00`
     - Afternoon: `13:00-18:00`
   - Apply these ranges to working days only (Monday-Friday).
   - Explain autotracking is active only during these ranges.
   - Explain advanced setup is available later in Settings.
   - Configure default project to `Default` by default.

3. **Add first project**
   - Fields:
     - Project name (**editable**, required, no SAP number in onboarding)
     - Regex list (simple list UI, no `app_id`, no `prec` input)
   - Explain additional projects can be added later.

4. **Jira sync (optional)**
   - Inputs:
     - URL
     - Username (also used as assignee)
     - Token (masked, with eye toggle like Settings)
     - Project key
     - SAP field name
   - Explain sync behavior and SAP usage:
     - SAP number is used automatically when Jira instance supports that integration.
     - User cannot configure that Jira-side integration inside LazyTime.
   - Jira credentials must be stored securely in OS keyring (see Security section).

5. **Reminder dialogs explanation**
   - Explain reminder dialogs for tracking prompts and post-screen-lock actions.

6. **Ready**
   - Final confirmation screen: "Ready - have fun."
   - Finishing onboarding starts app normally.

## Functional requirements
1. Onboarding appears when config indicates it is not completed.
2. While onboarding is active, no sidebar/top/bottom/normal content is rendered.
3. Back/Next navigation is available; optional Jira step can be skipped.
4. Finishing onboarding:
   - Marks onboarding as done.
   - Persists config.
   - Applies working hours to weekdays.
   - Ensures default project is configured.
   - Creates first project and its regex rules.
5. Settings -> General includes **Show onboarding** action:
   - sets onboarding state back to incomplete,
   - persists state,
   - onboarding is shown again (immediately or next launch; prefer immediate).

## Data model changes
Add to `Config`:
- `onboarding_done: bool` with serde default support for backward compatibility.

Defaults:
- New config templates set `onboarding_done = false`.

Compatibility:
- Existing configs without this field deserialize with default `false`.

## Security requirements (Jira credentials)
Token must not be persisted in plain config JSON.

Use OS keyring abstraction with platform backends:
- Linux: libsecret (and/or kwallet fallback where supported)
- Windows: Credential Manager
- macOS: Keychain

Expected behavior:
1. On save (onboarding/settings), token is written to keyring under a stable service/account key.
2. Config stores only non-secret Jira metadata and optionally a key reference/flag.
3. UI reads token from keyring when needed and supports masked display with eye toggle behavior.
4. Graceful fallback/errors if keyring unavailable:
   - clear user-visible message,
   - do not silently downgrade to insecure plain-text persistence.

## Implementation outline
1. **Config** (`src/config.rs`)
   - Add `onboarding_done` field with default.
   - Update default template.

2. **Onboarding view** (`src/gui/views/onboarding.rs`, new)
   - Local onboarding state per step.
   - Input validation for required fields/regex/time.

3. **GUI app wiring** (`src/gui/app.rs`)
   - Add onboarding view state.
   - In update loop, short-circuit rendering when onboarding is active.

4. **Views module export** (`src/gui/views/mod.rs`)
   - Export onboarding view.

5. **Settings integration** (`src/gui/views/settings*.rs`)
   - Add **Show onboarding** button in **General** tab.
   - Trigger re-run by toggling config state and persisting.

6. **Project/rules persistence**
   - Use existing DB helpers to create project and regex rules.
   - No SAP field entry in onboarding step 3.

7. **Keyring integration**
   - Add secure secret storage layer used by onboarding + settings Jira fields.
   - Keep behavior consistent with existing Jira sync flows.

## Acceptance criteria
1. Fresh install starts directly in onboarding; no other UI visible.
2. Completing onboarding persists all entered data and starts normal app UI.
3. Restart after completion does not show onboarding again.
4. General settings tab has **Show onboarding** and it re-triggers onboarding.
5. Step 2 applies `08:00-12:00` and `13:00-18:00` to Mon-Fri only.
6. First project creation accepts editable name and regex list.
7. Jira token is stored and retrieved via OS keyring; not plain config.
8. If keyring is unavailable, user gets actionable error and can still proceed without Jira sync.

## Test plan
1. **Fresh config path**:
   - launch GUI, verify onboarding gates full UI.
2. **Complete flow**:
   - enter project + regex + optional Jira fields,
   - finish, verify normal GUI appears.
3. **Persistence checks**:
   - config has onboarding_done=true,
   - working hours populated for Mon-Fri,
   - project/rules exist in DB.
4. **Re-run from settings**:
   - General tab action triggers onboarding again.
5. **Keyring checks**:
   - verify token not present in config,
   - token can be retrieved for Jira operations,
   - verify behavior on keyring failure path.
