# Audiobook Stitcher

Audiobook Stitcher is an open source desktop app that merges a folder of chapter audio files into a polished `.m4b` audiobook. It ships a friendly React interface on top of a high-performance Rust/Tauri backend so you get cross-platform builds, multi-core FFmpeg processing, and real-time progress feedback in one package.

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
frontend/    React + Vite UI
src-tauri/   Rust backend + Tauri configuration
docs/        Screenshots and ancillary assets (optional)
```

## 🚀 Getting started

Prerequisites:

- Node.js 18+
- Rust toolchain 1.91+ (`rustup default stable`)
- `cargo-tauri` CLI (`cargo install tauri-cli --locked`)
- FFmpeg binaries available on your `PATH` (or bundled—see below)

Install dependencies:

```bash
cd frontend
npm install
```

Start the dev environment (launches Vite + Tauri in one go):

```bash
npm run tauri:dev        # from the repository root
# or
cd src-tauri
cargo tauri dev
```

The app window will open automatically. Pick a chapter directory, tweak metadata, and hit **Start stitching** to produce your `.m4b`.

## 🏗 Building releases

1. (Optional) Drop platform-specific FFmpeg/ffprobe binaries into `src-tauri/resources/bin/<platform>/`.
2. Build the frontend assets:
   ```bash
   npm run build:frontend
   ```
3. Produce native bundles:
   ```bash
   npm run tauri:build        # or: cd src-tauri && cargo tauri build
   ```

Artifacts land under `src-tauri/target/release/` (DMG/APP for macOS, MSI/EXE for Windows, AppImage/Deb for Linux).

### Bundling FFmpeg

At runtime the app looks for FFmpeg/ffprobe in this order:

1. `FFMPEG_PATH` / `FFPROBE_PATH` environment variables
2. Executables discoverable via `which ffmpeg` / `which ffprobe`
3. Bundled binaries inside `src-tauri/resources/bin/<platform>/`

To ship with bundled binaries, place the files here before building:

- `src-tauri/resources/bin/macos-universal/{ffmpeg,ffprobe}`
- `src-tauri/resources/bin/windows-x64/{ffmpeg.exe,ffprobe.exe}`
- `src-tauri/resources/bin/linux-x64/{ffmpeg,ffprobe}`

Then ensure `tauri.conf.json` includes:

```json
{
  "bundle": {
    "resources": [
      "resources/bin/**"
    ]
  }
}
```

## 🧠 Architecture at a glance

- The Tauri command in `src-tauri/src/stitcher.rs` orchestrates the pipeline:
  1. Gather & naturally sort audio files
  2. Normalise each file via FFmpeg in an async worker pool
  3. Use ffprobe to capture durations and emit an FFmetadata chapter file
  4. Concatenate everything into a fast-start `.m4b`, embedding metadata and cover art
- Progress events (`stitch-progress`) stream back to the React UI so the log and status indicators stay up to date.

## 🤝 Contributing

Bug reports, feature ideas, and pull requests are welcome! If you’re looking for somewhere to jump in, check the open issues or tackle one of these roadmap items:

- UI support for editing/reordering chapter titles prior to export
- Job cancellation and queue management for batch processing
- Signed installers / package feeds (Homebrew, winget, etc.)
- Integrations with library managers (drag-and-drop, watch folders)

To submit a change:

1. Fork & clone the repo
2. Create a feature branch
3. Run `npm run lint` (frontend) and `cargo fmt && cargo check`
4. Open a PR describing the change

## 📄 License

Released under the MIT License. See [LICENSE](LICENSE) for details.

---

Built with ❤️ by the Audiobook Stitcher community. Happy listening!
