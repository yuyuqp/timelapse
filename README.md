# Timelapse

Session-based screenshot collection and timelapse rendering.

This project is being rewritten from the original Python implementation to Rust. The Python files are still present as historical reference, but the current active CLI is the Rust binary.

## Requirements

- Rust toolchain
- `ffmpeg` available in `PATH` for rendering

## Build

```sh
cargo build
```

Run from the workspace:

```sh
cargo run -- --help
```

## Session Layout

By default, `collect` creates timestamped sessions under the Timelapse library:

```text
~/Videos/Timelapse/
  sessions/
    2026-06-29_220503/
      frames/
        000000001.png
        000000002.png
      session.toml
      2026-06-29_220503.mp4
```

The default library path prefers the user's video directory. You can override it with `--library`.

## Commands

```sh
timelapse collect
timelapse render latest
timelapse list
timelapse open latest
timelapse clean latest --frames
```

When running through Cargo, place arguments after `--`:

```sh
cargo run -- collect
cargo run -- render latest
```

## Collect

Collect screenshots into a session folder:

```sh
cargo run -- collect
cargo run -- collect --interval 6s
cargo run -- collect --display all
cargo run -- collect --library ~/Videos/Timelapse
```

Use an exact session directory:

```sh
cargo run -- collect --session ./my-session
```

Append to an existing explicit session:

```sh
cargo run -- collect --session ./my-session --append
```

Append reads `session.toml` when available. If the requested interval differs from the existing session metadata, append fails unless `--force` is passed:

```sh
cargo run -- collect --session ./my-session --append --interval 10s --force
```

`--force` is intentionally narrow: it only allows append metadata mismatch or missing metadata. It does not overwrite sessions or delete files.

## Render

Render the newest session in the library:

```sh
cargo run -- render latest
cargo run -- render latest --library ~/Videos/Timelapse
```

Render a session directory or a plain numbered PNG frames directory:

```sh
cargo run -- render ./my-session
cargo run -- render ./my-session/frames
```

Render options:

```sh
cargo run -- render ./my-session --fps 15
cargo run -- render ./my-session --output ./out.mp4
cargo run -- render ./my-session --overwrite
cargo run -- render ./my-session --verbose
```

Rendering:

- Uses external `ffmpeg`.
- Infers frame padding and start number from numbered PNG files.
- Fails if there are missing frame numbers.
- Does not require `session.toml`.
- Does not overwrite existing output unless `--overwrite` is passed.
- Hides ffmpeg output by default; use `--verbose` to show it.

## List

Inspect sessions in the Timelapse library:

```sh
cargo run -- list
cargo run -- list --library ~/Videos/Timelapse
```

`list` shows each session path, frame count/range, video count, and metadata when available.

## Open

Open a session in the system file manager:

```sh
cargo run -- open latest
cargo run -- open latest --library ~/Videos/Timelapse
cargo run -- open ./my-session
```

## Clean

Permanently delete generated files from a recognized session:

```sh
cargo run -- clean latest --frames
cargo run -- clean latest --videos
cargo run -- clean ./my-session --frames --videos
cargo run -- clean ./my-session --frames --dry-run
cargo run -- clean ./my-session --frames --yes
```

Clean is intentionally conservative:

- It only works on session folders, not arbitrary raw frame directories.
- You must pass `--frames`, `--videos`, or both.
- It deletes numbered PNG frames from `frames/`.
- It deletes MP4 videos from the session directory.
- `--dry-run` shows the deletion plan without deleting files.
- It asks for confirmation unless `--yes` is passed.

## Not Implemented Yet

- `doctor`
- TUI
- Tauri GUI
- Config file support

## License

MIT License. See [LICENSE](./LICENSE) for details.
