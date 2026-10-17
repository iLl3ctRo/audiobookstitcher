# Bundled FFmpeg binaries

Place platform specific static FFmpeg builds in the folders below before packaging:

- `macos-universal/ffmpeg`
- `macos-universal/ffprobe`
- `windows-x64/ffmpeg.exe`
- `windows-x64/ffprobe.exe`
- `linux-x64/ffmpeg`
- `linux-x64/ffprobe`

The app will look in these locations at runtime before falling back to the system `PATH`, so shipping the binaries here ensures the stitcher runs on machines without FFmpeg pre-installed.
