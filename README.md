# READ FOR SPEED

A focused, native speed-reading app built with Rust and GPUI. Text is shown
one word at a time, with a highlighted focal letter and optional natural
pacing for punctuation, long words, and paragraph endings.

The native app in [`native/`](native/) is the only maintained version.
The previous React/Tauri implementation has been removed; it remains
available in Git history.

## Run on macOS

Install current stable Rust and the macOS command-line developer tools, then:

```sh
./native/run.sh
```

For an optimized, locally signed app bundle:

```sh
./native/run.sh --release
```

## Run on Windows

Install stable Rust and Visual Studio's C++ Build Tools with the Windows SDK:

```powershell
./native/run.ps1 -Release
```

Windows support is not yet runtime-verified. The manual **Build native Windows
app** workflow produces a portable executable artifact.

## Features

- Paste, open, write, and save readings locally.
- Read at 100–1,200 WPM with Natural or Steady pacing.
- Pause, replay, scrub progress, and resume saved readings.
- Dark/light themes, adjustable text size, and keyboard shortcuts.
- Native rendering with no webview or JavaScript runtime.

See [`native/README.md`](native/README.md) for build instructions, shortcuts,
storage locations, and current limitations. macOS bundles are ad-hoc signed,
not notarized for distribution.

## Development

```sh
cargo test --locked --manifest-path native/Cargo.toml
cargo clippy --locked --manifest-path native/Cargo.toml --all-targets -- -D warnings
```
