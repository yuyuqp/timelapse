# Terminal User Interface (TUI) Documentation

The `timelapse` interactive Terminal User Interface (TUI) is launched via:

```bash
cargo run -- tui
# or
timelapse tui
```

It offers a graphical terminal dashboard containing four main navigation tabs: **Capture**, **Render**, **Sessions**, and **Diagnostics**.

---

## Feature Comparison: CLI vs. TUI

| Feature Area | CLI Command | TUI tab / Control | UX Difference & Details |
| :--- | :--- | :--- | :--- |
| **Screenshot Capture** | `timelapse collect` | **Capture Tab** | **CLI**: Relies on starting and stopping via terminal signals (Ctrl+C). Option flags are static parameters.<br>**TUI**: Starts and stops capture using `[Space]`. Capture runs in a background thread while the UI remains interactive. Capture interval is adjustable in real-time via `[Up/Down]` arrow keys, and the display target can be toggled using `[D]`. |
| **Video Rendering** | `timelapse render <target>` | **Render Tab** | **CLI**: Requires typing session paths or `"latest"`. Rendering output is printed to stdout/stderr.<br>**TUI**: Automatically binds rendering to the session selected in the Sessions Tab. Shows real-time rendering state (Ready, Rendering, Success, Error). FPS is adjustable via `[Up/Down]` arrow keys. Runs in a background thread to prevent TUI freeze. |
| **Sessions Listing** | `timelapse list` | **Sessions Tab** | **CLI**: Prints a static text list of all sessions in chronological order.<br>**TUI**: Displays an interactive table with columns (Name, Frames, Videos, Path). Supports arrow key navigation (`[Up/Down]`) and list refreshes (`[U]`). |
| **Open Session Folder** | `timelapse open <target>` | **Sessions Tab** (`[O]` key) | **CLI**: Opens specified folder directly.<br>**TUI**: Opens the currently selected folder from the active row list directly in the system's default file manager. |
| **Session Cleanup** | `timelapse clean <target>` | **Sessions Tab** (`[C]` key) | **CLI**: Destructive commands require flags like `--frames` / `--videos` and manual CLI prompt confirmation.<br>**TUI**: Pressing `[C]` on a session displays a red popup modal overlay showing file counts. Pressing `[F]` cleans frames only, `[V]` cleans videos only, `[A]` cleans both, and any other key cancels. |
| **Environment Check** | `timelapse doctor` | **Diagnostics Tab** | **CLI**: Runs checks once and writes log lines to the terminal.<br>**TUI**: Automatically executes checks on load and displays results in a color-coded status panel (`[ok]` in green, `[warn]` in yellow, `[error]` in red). Diagnostic checks can be rerun at any time using `[D]` or `[U]`. |

## CLI Features Missing in TUI

To keep the terminal user interface streamlined, several advanced configuration flags from the CLI are omitted or automated in the TUI:

1. **Custom Target Sessions & Append Mode (`collect`)**:
   - **CLI**: Supports targeting specific session folders with `--session <path>`, appending new screenshots with `--append`, and bypassing checks with `--force`.
   - **TUI**: Always initiates a fresh timestamped session directory under `library/sessions/`. It does not support manual session path naming or append modes.

2. **Custom Output Paths & Overwrite Prompts (`render`)**:
   - **CLI**: Supports configuring the exact output file path via `--output` and prevents accidental overrides unless `--overwrite` is explicitly supplied.
   - **TUI**: Automatically renders to the default timestamped video file inside the session folder and automatically overwrites without prompting.

3. **Granular File Cleanup (`clean`)**:
   - **CLI**: Allows running a `--dry-run` or bypassing checks via `--yes`.
   - **TUI**: Supports granular deletion (`[F]` for frames only, `[V]` for videos only, or `[A]` for both) directly in the interactive modal, but does not support dry-runs or bypassing confirmations.

---

## Keyboard Controls & Navigation Reference

- **Tab Navigation**:
  - `[Tab]` or `[Right Arrow]`: Switch to the next tab.
  - `[Left Arrow]`: Switch to the previous tab.
  - `[1]`, `[2]`, `[3]`, `[4]`: Direct jump to Capture, Render, Sessions, or Diagnostics tab.
- **Global Actions**:
  - `[Q]`: Safely exit the TUI (will request capture background loops to stop and restore terminal raw modes).
- **Tab-Specific Keys**:
  - **Capture Tab**:
    - `[Space]`: Start or stop capture.
    - `[Up/Down]`: Increase/decrease capture interval (when idle).
    - `[D]`: Toggle display target (All Displays / Primary Display only).
  - **Render Tab**:
    - `[Enter]` or `[R]`: Start render for selected session.
    - `[Up/Down]`: Increase/decrease video FPS rate.
  - **Sessions Tab**:
    - `[Up/Down]`: Move row selection pointer.
    - `[O]`: Open selected session folder.
    - `[C]`: Trigger session clean popup modal (where `[F]`, `[V]`, `[A]` select target to clean, other keys cancel).
    - `[U]`: Reload sessions list from disk.
  - **Diagnostics Tab**:
    - `[D]` or `[U]`: Rerun diagnostic checks.
