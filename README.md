# Audiobook Stitcher (Tauri Edition)

A cross-platform desktop app built with Tauri (Rust backend + React frontend) that merges a directory of chapter audio files into a polished `.m4b` audiobook. The application normalises source audio in parallel via FFmpeg, embeds metadata/cover art, and provides real-time progress feedback in a modern UI.

## Project structure

```
.
├── tauri-app/
│   ├── frontend/         # React + Vite UI
│   └── src-tauri/        # Rust backend + Tauri configuration
└── legacy-python/        # (Optional) original Python CLI, retained for reference
```

> The legacy Python implementation has been archived under `legacy-python/` for posterity. The Tauri app is now the primary codebase.

## Features

- Natural sort of supported chapter files (`.m4a`, `.mp3`, `.aac`, `.wav`, `.flac`, `.ogg`, `.aiff`, `.m4b`).
- Parallel FFmpeg normalisation with configurable worker count, bitrate, and sample rate.
- Metadata embedding (title, author, narrator, album, year) and optional cover artwork.
- Automatic chapter marker generation—each source file becomes a chapter in the final `.m4b`.
- Bundled FFmpeg binary support for fully self-contained distributions.
- Real-time progress stream with per-stage updates and structured result summaries.

## Requirements

- Node.js ≥ 18 (for the React frontend).
- Rust toolchain ≥ 1.91 (install via `rustup`).
- `cargo-tauri` CLI (`cargo install tauri-cli --locked`).
- FFmpeg binaries for your target platforms (see [Bundling FFmpeg](#bundling-ffmpeg)).

## Getting started

Install frontend dependencies:

```bash
cd tauri-app/frontend
npm install
```

Run the app in development mode (watches both frontend and backend):

```bash
cd tauri-app
cargo tauri dev
```

The UI will open in a Tauri window. Select a chapter directory, adjust metadata/encoding preferences, and click **Start stitching** to launch the Rust pipeline.

## Building a release bundle

1. (Optional) Place platform-specific FFmpeg binaries under `tauri-app/src-tauri/resources/bin/...` (see below).
2. Build the frontend once: `npm --prefix tauri-app/frontend run build`
3. Produce installers/binaries: `cd tauri-app && cargo tauri build`

Artifacts are emitted under `tauri-app/src-tauri/target/release/`. Tauri produces platform-appropriate bundles (DMG/APP for macOS, MSI/EXE for Windows, AppImage/Deb for Linux).

## Bundling FFmpeg

The app looks for FFmpeg in three places (in order):

1. `FFMPEG_PATH` environment variable (useful during development).
2. A system installation discoverable via `which ffmpeg`.
3. A bundled binary in `resources/bin/<platform>/ffmpeg`.

To ship FFmpeg with your build, drop static binaries into the following paths before running `cargo tauri build`:

- `tauri-app/src-tauri/resources/bin/macos-universal/ffmpeg` (and `ffprobe`)
- `tauri-app/src-tauri/resources/bin/windows-x64/ffmpeg.exe` (and `ffprobe.exe`)
- `tauri-app/src-tauri/resources/bin/linux-x64/ffmpeg` (and `ffprobe`)

Then ensure these assets are copied into the final bundle by adding the corresponding glob to `bundle.resources` inside `tauri.conf.json`, e.g.

```json
{
  "bundle": {
    "resources": [
      "resources/bin/**"
    ]
  }
}
```

The repository keeps small placeholder files in each folder so the layout is tracked in version control; replace them with real FFmpeg binaries for release builds.

## Rust backend overview

The backend logic lives in `tauri-app/src-tauri/src/stitcher.rs` and mirrors the behaviour of the previous Python tool:

- Collects/validates source files, computes natural ordering, and derives a default output location.
- Normalises each chapter via FFmpeg in a thread pool (`tokio::process::Command` + `buffer_unordered` for concurrency).
- Samples track durations with `ffprobe` and emits an FFmetadata manifest so every chapter is preserved in the final file.
- Writes a temporary concat manifest and runs a final FFmpeg muxing pass to assemble the `.m4b`.
- Emits structured progress events (`stitch-progress`) to the renderer and returns a summary payload when complete.
- Provides detailed error propagation when FFmpeg fails (exit code + stderr).

## React frontend overview

The UI in `tauri-app/frontend/src/App.tsx` offers:

- Form-driven configuration with directory/file pickers powered by `@tauri-apps/plugin-dialog`.
- Live progress bar, stage indicators, and timestamped log stream.
- Result snapshot (output path, chapters merged, elapsed time) on success.
- Error section for failed runs.

State is managed with plain React hooks; commands are invoked via `@tauri-apps/api/core`.

## Legacy CLI (optional)

The historical Python CLI has been moved to `legacy-python/` in case you need a headless workflow or a reference for future migrations. The Dockerfile and related scripts were retired as part of the Tauri rewrite.

## Next steps

1. Allow chapter title editing/reordering before export.
2. Add cancellation support and background queueing for multiple jobs.
3. Package signed binaries for distribution channels (Homebrew, Winget, etc.).
4. Integrate with audiobook libraries (e.g. OpenAudible) via drag-and-drop or watched folders.
