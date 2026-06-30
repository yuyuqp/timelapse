# CLI & TUI UX Review

This document contains a comprehensive UX and code-level review of the **Timelapse** CLI and TUI.

---

## 1. CLI UX Review

The CLI is a solid, clean implementation utilizing `clap`. Below is an analysis of its strengths, limitations, and potential improvements.

### Strengths
- **Logical Command Set**: Commands like `collect`, `render`, `list`, `open`, `clean`, and `doctor` cover the full lifecycle of timelapse capture and management.
- **Smart Defaults**:
  - `collect` defaults to a `6s` interval when starting a new session.
  - When `--append` is specified, it automatically reads the interval from the existing session metadata (or warns if mismatching).
  - `render latest` and `clean latest` save the user from copy-pasting complex timestamped folder names.
- **Environment Diagnostics**: The `doctor` command is incredibly helpful for checking `ffmpeg` availability, screenshot backend configurations, and write permissions in a single command.
- **Safety Safeguards**:
  - `clean` requires explicit `--frames` or `--videos` flags so users do not accidentally delete files.
  - `clean` warns the user and requests interactive confirmation before deletion (bypassed only via `-y` / `--yes`).
  - `clean --dry-run` shows exactly what files would be deleted before running.

### UX Gaps & Friction Points
- **Strict Duration Formatting**:
  - The `--interval` parser requires unit suffixes (e.g. `6s` or `1m`). If a user inputs `6`, it fails with: `expected unit like 's', 'm', 'h'`.
  - *Recommendation*: If no unit is provided, default to seconds (e.g. treat `6` as `6s`).
- **Minimal Interactive Feedback in `collect`**:
  - The live capture feedback is limited to a single carriage-return line: `Frame X`.
  - The user cannot see how long the capture has been running, the current storage consumed by PNG frames, or which displays are being recorded.
  - There is no countdown or heartbeat spinner, making it hard to tell if the capture is stuck.
- **Strict Frame Validation on `render`**:
  - The rendering engine scans frames and throws a hard error if any frame number is missing (e.g., if a user manually deleted a corrupted frame).
  - *Recommendation*: Instead of failing, allow options like skipping missing frames, duplicating the adjacent frame, or warning without crashing.

---

## 2. TUI UX Review

The TUI (powered by `ratatui`) provides a visually structured environment that prevents terminal blocking.

### Strengths
- **Clear Navigation**: Quick keys `[1]`, `[2]`, `[3]`, `[4]` and `[Tab]` allow fluid movements.
- **Rich Contextual Metadata**:
  - The **Render Tab** calculates estimated playback duration, capture intervals, and speed multipliers (e.g., `90x`) dynamically.
  - The **Sessions Tab** features a dual pane with the sessions list and details from the selected session's `session.toml`.
- **Threaded Tasks**:
  - Capture runs on a background thread while keeping the TUI responsive.
  - Rendering runs on a background thread so the terminal does not freeze during CPU-intensive FFmpeg encodes.
- **Responsive Sizing**: The window size guard (`80x20` requirement) prevents UI distortion on smaller terminals.

### UX Gaps & Friction Points
- **Unconditional Redraws (CPU overhead)**:
  - The TUI main loop calls `terminal.draw` on every tick (up to 20 times a second), even when completely idle.
  - *Recommendation*: Change the event loop to redraw only on keystrokes, resize events, background thread messages, or a periodic slow timer tick (e.g. 1 second for elapsed capture time).
- **Silent rendering overwrites**:
  - Unlike the CLI, the TUI renders with `overwrite: true` hardcoded. Initiating a render silently overwrites existing output files with no confirmation or prompt.
- **No live FFmpeg progress**:
  - While rendering runs in the background, the TUI displays a static `Rendering: FFmpeg rendering in progress... Running ffmpeg...` message. The user cannot see progress percentages or estimated times.

---

## 3. Code-Level Glitches & Edge Cases

Upon reviewing the Rust source files, we identified several edge cases and silent bugs:

### A. Silently Retained Sessions on Invalid Library Path
- **File**: [tui.rs](file:///c:/Users/yuyue/projects/timelapse/src/tui.rs#L135-L142)
- **Problem**: When changing the library path in the TUI via `[L]`, the `refresh_sessions()` helper does this:
  ```rust
  fn refresh_sessions(&mut self) {
      if let Ok(list) = list_sessions(self.library_path.clone()) {
          self.sessions = list;
          ...
      }
  }
  ```
  If the new path is invalid or non-existent, `list_sessions` returns an `Err`. The TUI silently ignores the error and leaves `self.sessions` with the **old** library's sessions. The user sees a "Library path updated successfully" toast but continues viewing stale sessions, thinking they belong to the new library.
- *Recommendation*: Clear the list or show an error state if `list_sessions` fails.

### B. Display Target Mismatches on Append Mode
- **File**: [session.rs](file:///c:/Users/yuyue/projects/timelapse/src/session.rs#L245-L279)
- **Problem**: `open_append` validates and warns if capture intervals differ, but does *not* validate if display targets differ.
  - If a session was started with `primary` display (e.g. single 2560x1440 monitor) and appended to using `all` displays (resulting in multiple screens stitched side-by-side, e.g. 4480x1440), the frames directory will end up with mixed-resolution PNGs.
  - FFmpeg rendering will subsequently fail or produce distorted videos because the frame sizes vary.
- *Recommendation*: Raise a warning (or error out without `--force`) if the display targets in `session.toml` and capture arguments do not match.

### C. Uninterruptible Initialization
- **File**: [tui.rs](file:///c:/Users/yuyue/projects/timelapse/src/tui.rs#L360) and [tui.rs](file:///c:/Users/yuyue/projects/timelapse/src/tui.rs#L835-L839)
- **Problem**: While the capture state is `Starting`, the user cannot cancel or abort. If the screenshot backend (`xcap`) hangs during initialization (which is possible on macOS/Linux due to permission dialogs or Windows during display reconfigurations), the TUI freezes in the starting state. The user must force-quit the application.
- *Recommendation*: Allow the `[Space]` key to request an abort even during the `Starting` phase.
