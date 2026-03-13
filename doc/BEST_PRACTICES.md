# WTF TUI - Best Practices

> **Note**: This file will be automatically loaded by AI coding agents working on this project. Add coding conventions, architecture patterns, and quality guidelines here as they are identified.

## Coding Conventions

### Function Parameters

**Rule: Limit function parameters to a maximum of 5-7 parameters.**

When a function needs more parameters:
- **Pass the struct instead of individual fields** - If multiple parameters come from the same struct, pass a reference to the entire struct
- **Group related parameters into a struct** - Create a dedicated parameter struct or builder pattern for complex operations
- **Use the builder pattern** - For optional parameters or configuration-heavy functions

#### Examples

❌ **Bad - Too many parameters from the same struct:**
```rust
fn create_worklog(
    issue_key: &str,
    time_spent: i64,
    started_at: DateTime<Utc>,
    comment: &str,
    author_id: &str,
    author_name: &str,
    author_email: &str,
    // ... 15+ more fields
) -> Result<Worklog>
```

✅ **Good - Pass the struct or a subset:**
```rust
fn create_worklog(issue_key: &str, worklog_data: &WorklogData) -> Result<Worklog>
// or if only a few fields are needed:
fn create_worklog(issue_key: &str, time_spent: i64, started_at: DateTime<Utc>) -> Result<Worklog>
```

❌ **Bad - Many unrelated parameters:**
```rust
fn process_sprint(id: i32, name: &str, state: &str, start: DateTime<Utc>, 
                  end: DateTime<Utc>, goal: &str, board_id: i32, 
                  fetch_issues: bool, auto_link: bool) -> Result<()>
```

✅ **Good - Group into config struct:**
```rust
struct SprintProcessConfig {
    fetch_issues: bool,
    auto_link: bool,
}

fn process_sprint(sprint: &Sprint, config: &SprintProcessConfig) -> Result<()>
```

## Architecture & Design Patterns

### File Size and Module Organization

**Rule: Keep source files under 500 lines when possible, never exceed 1000 lines without justification.**

Large files become difficult to navigate, review, and maintain. When a file grows too large, split it into focused modules.

#### File Size Guidelines

| Size | Status | Action |
|------|--------|--------|
| < 300 lines | ✅ Good | Ideal for most modules |
| 300-500 lines | ⚠️ Warning | Consider splitting if clear boundaries exist |
| 500-1000 lines | 🔴 Too Large | Must split into smaller modules |
| > 1000 lines | 🚨 Critical | Immediate refactoring required |

#### When to Split a Module

Split a file when:
- **Multiple responsibilities** - File handles distinct concerns (e.g., rendering + business logic)
- **Clear logical sections** - Code naturally groups into cohesive units
- **Difficult navigation** - Hard to find specific functions quickly
- **High churn rate** - Frequent merge conflicts due to size
- **Testing difficulties** - Hard to write focused unit tests

#### What Goes in `mod.rs`

**mod.rs should be MINIMAL** - use it only for:
- Module declarations (`pub mod submodule;`)
- Re-exports (`pub use submodule::Type;`)
- Small core types shared across submodules (< 50 lines)
- Module-level documentation

**Don't put in mod.rs:**
- ❌ Implementation details (move to dedicated files)
- ❌ Large structs or enums with many methods (move to separate file)
- ❌ Business logic (create handler/service files)
- ❌ UI rendering (create ui modules)

#### Module Splitting Examples

❌ **Bad - Everything in mod.rs:**
```rust
// dashboard/mod.rs - 2800 lines
pub struct Dashboard { /* 20 fields */ }
impl Dashboard {
    // 50+ methods for all operations
    fn handle_sprints_key() { }
    fn handle_meetings_key() { }
    fn wizard_step_sync() { }
    fn wizard_step_autolink() { }
    // ... 40+ more methods
}
```

✅ **Good - Split by responsibility:**
```rust
// dashboard/mod.rs - ~100 lines
pub mod handlers;
pub mod wizard;
pub mod state;

pub use state::Dashboard;
pub use wizard::WizardState;

// dashboard/state.rs - ~150 lines
pub struct Dashboard { /* fields */ }
impl Dashboard {
    pub fn new() -> Self { }
    pub fn run() -> Result<()> { }
}

// dashboard/handlers/sprints.rs - ~200 lines
impl Dashboard {
    pub(crate) fn handle_sprints_key(&mut self, key: KeyCode) { }
}

// dashboard/wizard/mod.rs - ~400 lines
pub struct WizardState { }
impl Dashboard {
    pub(crate) fn launch_wizard(&mut self) { }
    pub(crate) fn wizard_step_sync(&mut self) { }
}
```

#### UI Module Organization

For large UI files, split by screen/component:

❌ **Bad - Single 3900-line ui.rs:**
```rust
// ui.rs - 3900 lines
pub fn render() { }
fn render_sprints_tab() { }
fn render_meetings_tab() { }
fn render_worklogs_tab() { }
fn render_github_tab() { }
// ... 30+ more render functions
```

✅ **Good - Split by UI concern:**
```rust
// ui/mod.rs - ~100 lines
mod sprints_ui;
mod meetings_ui;
mod worklogs_ui;
mod popups_ui;
mod helpers;

pub fn render(frame: &mut Frame, dashboard: &Dashboard, logs: &[String]) {
    // Main layout only
    match dashboard.current_tab {
        Tab::Sprints => sprints_ui::render_sprints_tab(frame, ...),
        Tab::Meetings => meetings_ui::render_meetings_tab(frame, ...),
        // ...
    }
}

// ui/sprints_ui.rs - ~300 lines
// ui/meetings_ui.rs - ~300 lines
// ui/worklogs_ui.rs - ~450 lines
// ui/popups_ui.rs - ~500 lines
```

#### Refactoring Strategy

When splitting a large file:
1. **Identify logical boundaries** - Group related functions
2. **Create new module files** - One responsibility per file
3. **Move code incrementally** - Small commits, verify builds
4. **Update imports** - Use `pub(crate)` for module-internal visibility
5. **Test after each move** - Ensure no regressions

---

### Feature Ownership, Cohesion, and Encapsulation

**Rule: Keep feature-specific behavior with the feature that owns it. Prefer high cohesion over scattering logic across unrelated files.**

If code exists only to support one screen, tab, popup, or workflow, it should usually live in that feature's module. Avoid spreading one feature's rendering, keyboard shortcuts, popup state, filtering rules, and business decisions across generic files unless the logic is truly shared.

#### What belongs together

Keep these close to the owning feature/module whenever possible:
- Rendering for that feature
- Keyboard shortcuts and interaction rules for that feature
- Popup/dialog state used only by that feature
- Filtering, sorting, validation, and mapping rules specific to that feature
- Small helper functions that are not reused elsewhere

Extract code only when it is genuinely shared by multiple features, or when you are isolating a reusable abstraction.

#### Use traits/interfaces for shared mechanics

When multiple tabs or workflows follow the same pattern, define a shared trait/interface for the common mechanics and let each feature implement its own behavior behind that contract.

The shared layer should depend on the trait, not on concrete feature-specific code.

❌ **Bad - feature logic scattered across generic modules:**
```rust
// tui/mod.rs
fn handle_meetings_key(&mut self, key: KeyEvent) { /* meetings-only behavior */ }

// ui/popups/mod.rs
fn render_unlink_confirmation(...) { /* only used by meetings */ }

// ui_helpers.rs
fn meeting_filter(...) -> bool { /* meetings-only rule in a generic file */ }
```

✅ **Good - feature owns its behavior, shared layer uses contracts:**
```rust
// ui/tabs/meetings/mod.rs
pub struct MeetingsTab { /* meetings state */ }

impl MeetingsTab {
    fn render(&self, frame: &mut Frame, area: Rect) { }
    fn handle_key(&mut self, key: KeyEvent) { }
    fn render_popups(&self, frame: &mut Frame) { }
    fn filter_meetings(&self, meeting: &Meeting) -> bool { }
}

trait TabController {
    fn render(&self, frame: &mut Frame, area: Rect);
    fn handle_key(&mut self, key: KeyEvent);
}

impl TabController for MeetingsTab { /* ... */ }
```

#### Meetings tab example

For the meetings feature, code related to meeting shortcuts, selection dialogs, unlink confirmations, auto-linking triggers, and meeting-specific filtering should be grouped with the meetings tab/module. The top-level TUI should only orchestrate tab switching and call a well-defined interface, rather than owning meetings-specific behavior directly.

#### Heuristic

Ask: **"If I rename or delete this feature, would this code move with it?"**

If the answer is yes, the code probably belongs in that feature's module.

---

### Current Violations

**Files requiring refactoring:**
- 🚨 `wtf-cli/src/dashboard/ui.rs` - 3,895 lines (split into 8 modules)
- 🚨 `wtf-cli/src/dashboard/mod.rs` - 2,804 lines (split into 4 modules)

## Error Handling
<!-- Add Result/Option usage patterns, error propagation guidelines -->

## Testing
<!-- Add testing strategies, coverage expectations, test organization -->

## Dependencies & Security
<!-- Add dependency management policies, security practices -->

## AI Agent Guidelines
<!-- Add specific rules for AI-generated code, what to avoid, required patterns -->

---
*This file should be updated as quality issues are discovered and patterns emerge.*
