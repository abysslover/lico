# M07-P02 Plan — Two-column workbench left `[ Directories | Past ]` sidebar with highlighted active `cwd`, filter, pagination, and inline preview

- Status: in progress
- Version: 1.0.0
- Date: 2026-11-12
- Milestone: [M07 — Two-pane tabbed workbench, session telemetry bar, Global general sessions, and paginated past-session resume (`lico 0.2.3`)](../../../milestones/202611/20261112/20261112_04_M07_two_pane_workbench_and_past_sessions_milestone.md)
- Branch: `phase/M07-P02-left-directory-sidebar`
- Approval: G1 and G2 approved by the owner on 2026-11-12.

## Why

With open sessions moved to the horizontal tab bar at the top of the right-hand
chat pane in `M07-P01`, Column 1 (`LIST_W = 330` logical px, collapsible via
`Ctrl+B`) can serve as a dedicated two-tab workspace navigator (`[ Directories ]`
and `[ Past ]`) without adding a third column that would narrow the conversation
view. `[ Directories ]` lets the administrator browse local and SSH remote
directories (`Host: local v` / `Host: <alias> v`), immediately see the active
session's working directory (`cwd` or `Global`) highlighted, filter filenames
in-memory, page through large directories, and click files for wide inline UTF-8
preview (`Esc` returns to chat) or system default opening.

## Scope and steps

| Step | Scope | Verification |
|---|---|---|
| S01 | Commit this `M07-P02` plan and update `M07` milestone, `PLAN.md`, and `doc/index_4.md` | Pre-commit guards pass |
| S02 | Add `gui/sidebar.rs` and `gui/sidebar_render.rs` implementing the `Ctrl+B` collapsible left column with two default tabs (`[ Directories ]` and `[ Past ]`), `Host: local / ssh:<alias>` combo-box selector, `.dotfiles` toggle, refresh, `+` new session in viewed dir, instant filename filter, highlighted active `cwd` / `Global` scope row (`[-> active]` jump-back when browsing elsewhere), async auto-sync on session tab switch, mouse-wheel & `< Page X/Y >` pagination, and unified file click (wide inline UTF-8 preview vs external opener); wire top-tab hover memory hints (`memrow.rs`) | Rust unit tests and `gui::scroll_oracle` at scales `1.0`, `1.25`, `2.0` pass |
| S03 | Update `ProductWindow` session-selection helpers to click top session tabs (`tabs.items`), add `tests/test_left_directory_sidebar.py` (`tests/testmap.toml`), capture `00015` manual inspection bundle, write `M07-P02` report, and commit cleanly | Changed-area suite, `test_size_ratchet.py`, and `test_selection.py` pass |

## Personal data and security

1. Sidebar directory listing and inline UTF-8 previews reuse the hardened
   `folder.list` and `folder.read` host operations (`O_NOFOLLOW | O_NONBLOCK`,
   protected `~/.local/state/lico` refusal, `<= 64 KiB` preview cap, bounded SSH
   timeouts, and zero background polling).
2. All tests and screenshots use synthetic repositories, filenames, and SSH host
   aliases only.
