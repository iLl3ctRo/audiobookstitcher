# Audiobook Stitcher

Audiobook Stitcher is an open source desktop app that merges a folder of chapter audio files into a polished `.m4b` audiobook. It is a pure-Rust native app (GPUI) with a high-performance FFmpeg backend, so you get cross-platform builds, multi-core FFmpeg processing, and real-time progress feedback in one package.

![Audiobook Stitcher UI](docs/screenshot.png)

## ✨ Features

- 🗂 Natural sorting of chapter files (`.m4a`, `.mp3`, `.aac`, `.wav`, `.flac`, `.ogg`, `.aiff`, `.m4b`)
- ⚙️ Parallel FFmpeg normalisation with configurable bitrate, sample rate, and worker count
- 🏷 Metadata embedding (title, author, narrator, album, year) plus optional cover art
- 📚 Automatic chapter generation—each input file becomes a chapter marker in the final `.m4b`
- 📦 Bundled FFmpeg support so releases are self-contained on macOS, Windows, and Linux
- 📈 Rich progress stream (log view, status pill, result summary) for long-running jobs

## 📁 Repository layout

```
Cargo.toml          Crate manifest (+ cargo-bundle metadata)
src/main.rs         Entry point
src/app.rs          GPUI user interface
src/form.rs         UI-free form logic (options conversion, log cap, ...)
src/stitcher.rs     FFmpeg pipeline
src/binaries.rs     Bundled ffmpeg/ffprobe lookup
tests/e2e.rs        Ignored end-to-end test (needs ffmpeg)
resources/bin/      Optional bundled FFmpeg binaries per platform
assets/             App icons
```

## 🚀 Getting started

Prerequisites:

- Rust toolchain 1.91+ (`rustup default stable`)
- macOS: Xcode with the Metal toolchain (`xcodebuild -downloadComponent MetalToolchain`)
- FFmpeg binaries available on your `PATH` (or bundled, see below)

Run the app:

```bash
cargo run
```

Pick a chapter directory, tweak metadata, and hit **Start stitching** to produce your `.m4b`.

The UI is built with [GPUI](https://gpui.rs) (`gpui 0.2.2`) and
[`gpui-component`](https://github.com/longbridge/gpui-component) (`0.5.1`, the last release line built against crates.io `gpui`).

## 🏗 Building releases

```bash
cargo build --release      # binary at target/release/audiobook-stitcher
```

On macOS you can produce an `.app` bundle with [`cargo-bundle`](https://github.com/burtonageo/cargo-bundle):

```bash
cargo install cargo-bundle
cargo bundle --release
```

The `[package.metadata.bundle]` section in `Cargo.toml` configures the icon and copies `resources/bin` into the bundle.

### FFmpeg resolution

At runtime the app looks for FFmpeg/ffprobe in this order:

1. `FFMPEG_PATH` / `FFPROBE_PATH` environment variables
2. Executables discoverable on `PATH` (`ffmpeg` / `ffprobe`)
3. Bundled binaries (non-empty files), checked in this order, as `ffmpeg` or `ffmpeg-<target-triple>` (e.g. `ffmpeg-aarch64-apple-darwin`):
   - `<exe_dir>/resources/bin/<platform>/`
   - `<exe_dir>/../Resources/bin/<platform>/` (macOS `.app`)
   - `<exe_dir>/../Resources/resources/bin/<platform>/` (`cargo bundle` layout)
   - `<crate>/resources/bin/<platform>/` (debug builds only)

`<platform>` is `macos-universal`, `windows-x64` or `linux-x64`. ffprobe is additionally looked up next to the resolved ffmpeg.

## 🧪 Testing

```bash
cargo test                          # unit tests
cargo test --test e2e -- --ignored  # real FFmpeg end-to-end run (needs ffmpeg/ffprobe)
```

## 🧠 Architecture at a glance

- `stitcher::stitch_audiobook` orchestrates the pipeline:
  1. Gather & naturally sort audio files
  2. Normalise each file via FFmpeg in an async worker pool
  3. Use ffprobe to capture durations and emit an FFmetadata chapter file
  4. Concatenate everything into a fast-start `.m4b`, embedding metadata and cover art
- Progress is reported through a `ProgressSink` callback; the UI runs the job on a background tokio runtime and streams updates into the view over an async channel.

## 🤝 Contributing

Bug reports, feature ideas, and pull requests are welcome! If you’re looking for somewhere to jump in, check the open issues or tackle one of these roadmap items:

- UI support for editing/reordering chapter titles prior to export
- Job cancellation and queue management for batch processing
- Signed installers / package feeds (Homebrew, winget, etc.)
- Integrations with library managers (drag-and-drop, watch folders)

To submit a change:

1. Fork & clone the repo
2. Create a feature branch
3. Run `cargo fmt && cargo clippy --all-targets && cargo test`
4. Open a PR describing the change

## 📄 License

Released under the MIT License. See [LICENSE](LICENSE) for details.

---

Built with ❤️ by the Audiobook Stitcher community. Happy listening!
