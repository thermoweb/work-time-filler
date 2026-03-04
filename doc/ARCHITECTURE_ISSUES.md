# WTF – Architecture & Best Practices Issues

## 🔴 High Priority

### 1. Multiple Tokio runtimes spawned per action
**Files:** `wtf-cli/src/tui/mod.rs` (lines 76, 678, 1168, 2030),
`wtf-cli/src/tui/operations/worklogs.rs:51`,
`wtf-cli/src/tui/operations/meetings.rs:36, 205`,
`wtf-cli/src/tui/operations/github.rs:27`

Every async operation creates a fresh `tokio::runtime::Runtime::new()` and calls `block_on`. This is expensive (thread pool created/torn down each time), prevents sharing connections/caches, and can cause subtle bugs when runtimes are nested.

**Fix:** Create one `Arc<Runtime>` at the TUI entrypoint and pass it to operations, or make the TUI itself run inside `#[tokio::main]` and use `tokio::spawn` + channels for background work.

---

### 2. Global Lazy statics for all databases
**Files:** `wtf-lib/src/services/`

Most services still own their sled collection as a process-wide singleton:
- `ISSUES_DATABASE`, `BOARD_DATABASE`, `SPRINT_DATABASE` – `jira_service.rs`
- `LOCAL_WORKLOGS_DB`, `LOCAL_WORKLOGS_HISTORY_DB`, `WORKLOGS_DATABASE` – `worklogs_service.rs`
- `MEETINGS_DATABASE`, `UNTRACKED_MEETINGS_DATABASE`, `ABSENCES_DATABASE` – `meetings_service.rs`
- `GITHUB_EVENTS_DB`, `GITHUB_SESSIONS_DB` – `github_service.rs`

Consequences: impossible to unit test (no way to inject a test DB), hidden coupling between modules, `Mutex` lock across threads can deadlock if held over an await point.

**Fix:** Use dependency injection — `AchievementService` has been refactored as a reference implementation (see `wtf-lib/src/services/achievement_service.rs`).

---

### 3. Inconsistent error handling (unwrap / panic / Result)
**Files:** throughout

- `wtf-cli/src/commands/issue.rs:24` — bare `panic!("OH !")` on a duration parse error
- `wtf-lib/src/client/paginated.rs:56` — `.unwrap()` on JSON parse, network error crashes TUI
- 60+ `.unwrap()` calls on `DateTime` arithmetic, regex, and database operations

**Fix:** Add a crate-level `AppError` enum using `thiserror`, propagate with `?` throughout, and handle at the TUI event loop boundary with a user-facing error message.

---

## 🟡 Medium Priority

### 4. TuiData is a God struct
**File:** `wtf-cli/src/tui/data.rs`

All UI state lives in one flat struct. This makes the rendering code hard to follow and means any tab re-render re-reads everything.

**Fix:** Split into focused sub-states:
```rust
pub struct TuiData {
    pub meetings: MeetingsState,
    pub sprints: SprintsState,
    pub achievements: AchievementsState,
    // …
}
```

---

### 5. Services do direct DB access with no abstraction layer
**Files:** `wtf-lib/src/services/*.rs`

`ISSUES_DATABASE.insert(...).unwrap()` is called inline in service methods. There is no repository/storage trait, so you can't swap backends or mock for tests.

**Fix:** Introduce a `Repository<T>` trait with `get`, `insert`, `delete`, `get_all` methods. Services take `&dyn Repository<T>` (or `impl Repository<T>`).

---

### 6. Mutex-wrapped caches can deadlock
**Files:** `wtf-lib/src/services/`

`Lazy<Mutex<Vec<...>>>` caches locked with `.lock().unwrap()`. Holding a `std::sync::Mutex` guard across an async await point causes a deadlock (or a panic if another thread panics while holding it).

**Fix:** Replace with `tokio::sync::Mutex` if used in async contexts, or use `DashMap`/`RwLock` for read-heavy caches.

---

## 🟢 Low Priority / Nice-to-have

### 7. Regex patterns compiled at call time
**Files:** `wtf-cli/src/tui/wizard.rs:63`, `wtf-cli/src/tui/operations/meetings.rs`, etc.

`Regex::new(r"([A-Z]+-\d+)").unwrap()` is called every time the function runs. Minor perf issue, and the `.unwrap()` will panic if the pattern is ever broken.

**Fix:** Use `std::sync::LazyLock` to compile regexes once at startup.

---

### 8. Limited test coverage
**Files:** `wtf-lib/src/services/achievement_service.rs` has 8 tests; `wtf-lib/src/utils/version.rs` has tests. The rest has zero coverage.

Refactoring the issues above (especially #1 and #2) is risky without a test baseline.

**Fix:** Start with pure-logic unit tests (date calculations, `is_untracked`, achievement triggers) — these require no DB or network mock. Use `Database::temporary()` (already available) for service-level tests.

---

## ✅ Already good

- `wtf-lib` / `wtf-cli` separation is clean — lib has zero TUI imports
- `models/` layer is well-separated from services
- The `Task` trait pattern in `wtf-cli/src/tasks/` is the right abstraction for async work
- Config loading is centralised in `wtf-lib/src/config.rs`
- `AchievementService` fully refactored to DI pattern with unit tests (reference implementation)
