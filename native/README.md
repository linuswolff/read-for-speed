# READ FOR SPEED — native

READ FOR SPEED is a native RSVP reader built with Rust + GPUI, rendered
with Metal on macOS and DirectX on Windows. There is no webview, JavaScript
runtime, or local server. macOS builds are verified locally; Windows builds and runtime
still need verification on Windows.

## macOS

Requires current stable Rust and the macOS command-line developer tools.
GPUI is pinned to 0.2.2 and compiles Metal shaders at runtime, so a full Xcode
installation or a global `xcode-select` change is not necessary.

From the repository root:

```sh
./native/run.sh
# Optimized, standalone macOS app bundle:
./native/run.sh --release
# Unsaved demo, separate from your library:
./native/run.sh --demo
```

The script creates `native/target/debug/SPEED Native.app` (or `release/`)
and opens it. You can move the release bundle wherever you keep local apps.
It is locally ad-hoc signed, not notarized for distribution.

For development without bundling:

```sh
cargo run --locked --manifest-path native/Cargo.toml
cargo test --locked --manifest-path native/Cargo.toml
cargo clippy --locked --manifest-path native/Cargo.toml --all-targets -- -D warnings
```

## Windows

Targets Windows 10/11 x64. Build on Windows with current stable Rust
(`x86_64-pc-windows-msvc`) and Visual Studio Build Tools' **Desktop development
with C++** workload, including the Windows SDK. Release builds need the SDK's
`fxc.exe`; the script finds it, or you can set `GPUI_FXC_PATH` explicitly.

From the repository root in PowerShell:

```powershell
./native/run.ps1 -Release
# Unsaved demo:
./native/run.ps1 -Release -Demo
# Build without launching:
./native/run.ps1 -Release -BuildOnly
```

The portable executable is
`native/target/x86_64-pc-windows-msvc/release/speed-native.exe`.
Copy it to the work laptop; Rust and the SDK are build-time requirements,
not required to run the app. The script statically links the MSVC runtime.
The executable is unsigned: SmartScreen or company policy may block it.

The **Build native Windows app** GitHub Actions workflow is manual-only
and uploads an executable artifact; it does not publish a release.
Run it from the Actions tab to produce a Windows download.
No Windows build or runtime test has been performed yet.

## Reading

- Paste clipboard text, open a UTF-8 text/Markdown file, or write/edit text.
- Read at 100–1,200 WPM. Natural pacing adds dwell time at punctuation,
  paragraph endings, and long words; steady pacing uses a fixed interval.
- The highlighted grapheme stays on ticks at 38% of the reading area. It prefers
  the first vowel in the first four letters, with a length-based fallback.
  Long words shrink to fit; accents, combining characters, and emoji stay intact.
- Pause, replay, jump ten words, or scrub progress.
- Saved readings remember their positions; completion survives reopening.
- The library opens as a centered card over a softly dimmed reader, without
  moving the word. Opening it pauses playback; selecting a reading closes it.
  Click outside, use the close icon, or press Esc to dismiss it. The backdrop
  is dimmed rather than blurred because GPUI 0.2 has no backdrop-blur API.
- One bottom toolbar: pacing and playback on the left, progress in the middle,
  speed, library, paste, and settings on the right. The title and toolbar hide
  after three idle seconds and return when the pointer moves anywhere in the
  window or a key is pressed. They stay visible while the toolbar is hovered,
  progress is being dragged, or the library, settings, or editor are open.
  The word and guides do not move when controls hide. There is no manual calm
  mode or context view.
- Dark by default, with the saved theme preference preserved. Theme and text
  size live in the settings popover. The compact editor supports native text
  entry, selection, clipboard operations, and undo/redo; it fits at the smaller
  680 × 360 minimum window size.
- Lucide SVG icons are embedded in the executable, including debug builds.
  They come from [Lucide](https://github.com/lucide-icons/lucide/tree/70562c1ee1c4fdcf736fe97bc893fb8511927934);
  their ISC and Feather MIT notices are in `assets/lucide/LICENSE`.
- No separate native title bar. macOS traffic lights sit in the app header;
  Windows minimize/maximize/close buttons sit at the top right.
  Drag the heading or reading area to move the window; edge resizing remains
  available. Double-click to maximize.
  Close with `⌘ W` / `Ctrl+W`, minimize with `⌘ M` / `Ctrl+M`, or use the OS
  dock/taskbar. Windows also supports `Alt+F4`.
- `?` opens the keyboard reference. `Space` reads/pauses, `↑/↓` changes speed,
  `←/→` steps, `Shift ←/→` jumps ten, `Esc` closes library/settings,
  `L` toggles the library, `P` toggles natural pacing, `T` toggles the theme,
  `R` restarts, `⌘ V` pastes,
  `⌘ O` opens, `⌘ N` writes, `⌘ E` edits, and `⌘ Enter` saves the editor.
  On Windows, use `Ctrl` instead of `⌘`.

## Storage and limits

Readings and preferences live only in the OS-local app data directory:
- macOS: `~/Library/Application Support/com.linuswolff.speed-native/library.json`
- Windows: `%LOCALAPPDATA%\com.linuswolff.speed-native\library.json`

Writes use a temporary file and
atomic rename. An unreadable library is left untouched; the app shows a
warning and uses an unsaved fallback session instead of overwriting it.

Imports are limited to 2 MB of UTF-8 text. Markdown is read as plain text;
PDF, EPUB, URL extraction, and rich-text conversion are not implemented.
Windows builds/runtime, Linux, screen-reader support, code signing for distribution,
and notarization are not verified in this iteration. RSVP is a presentation
choice, not a claim of improved comprehension.
