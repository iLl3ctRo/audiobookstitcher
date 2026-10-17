//! Locating bundled FFmpeg / ffprobe executables.
//!
//! Lookup order (implemented by the stitcher): environment variable, `PATH`,
//! then the bundled directories computed here.

use std::path::{Path, PathBuf};

/// Directory name used under `resources/bin/` for the current platform.
pub fn platform_directory() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos-universal"
    } else if cfg!(target_os = "windows") {
        "windows-x64"
    } else if cfg!(target_os = "linux") {
        "linux-x64"
    } else {
        "unknown"
    }
}

/// Rust-style target triple suffix used for sidecar-style file names
/// (e.g. `ffmpeg-aarch64-apple-darwin`).
pub fn target_triple() -> String {
    let arch = std::env::consts::ARCH;
    match std::env::consts::OS {
        "macos" => format!("{arch}-apple-darwin"),
        "windows" => format!("{arch}-pc-windows-msvc"),
        "linux" => format!("{arch}-unknown-linux-gnu"),
        other => format!("{arch}-{other}"),
    }
}

/// Directories that may hold bundled binaries, in priority order.
pub fn bundled_dirs(exe_dir: Option<&Path>, manifest_dir: Option<&Path>) -> Vec<PathBuf> {
    let platform = platform_directory();
    let mut dirs = Vec::new();
    if let Some(exe_dir) = exe_dir {
        dirs.push(exe_dir.join("resources").join("bin").join(platform));
        // macOS .app bundle: Contents/MacOS/<exe> -> Contents/Resources
        if let Some(contents) = exe_dir.parent() {
            let resources = contents.join("Resources");
            dirs.push(resources.join("bin").join(platform));
            // cargo-bundle keeps the `resources` directory name
            dirs.push(resources.join("resources").join("bin").join(platform));
        }
    }
    if let Some(manifest_dir) = manifest_dir {
        dirs.push(manifest_dir.join("resources").join("bin").join(platform));
    }
    dirs
}

/// File names to try for a given executable name (plain, then target-triple suffixed).
pub fn candidate_names(executable: &str) -> Vec<String> {
    let triple = target_triple();
    match executable.strip_suffix(".exe") {
        Some(stem) => vec![executable.to_string(), format!("{stem}-{triple}.exe")],
        None => vec![executable.to_string(), format!("{executable}-{triple}")],
    }
}

/// All full candidate paths, in priority order.
pub fn candidate_paths(
    exe_dir: Option<&Path>,
    manifest_dir: Option<&Path>,
    executable: &str,
) -> Vec<PathBuf> {
    let names = candidate_names(executable);
    bundled_dirs(exe_dir, manifest_dir)
        .into_iter()
        .flat_map(|dir| names.iter().map(move |n| dir.join(n)))
        .collect()
}

/// Finds a bundled executable relative to the running binary (and, in debug
/// builds, the crate's own `resources/` directory).
pub fn find_bundled(executable: &str) -> Option<PathBuf> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    let manifest_dir = if cfg!(debug_assertions) {
        Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")))
    } else {
        None
    };
    candidate_paths(exe_dir.as_deref(), manifest_dir.as_deref(), executable)
        .into_iter()
        .find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dirs_cover_exe_bundle_and_manifest() {
        let dirs = bundled_dirs(
            Some(Path::new("/App/Contents/MacOS")),
            Some(Path::new("/src/proj")),
        );
        let p = platform_directory();
        assert_eq!(dirs[0], Path::new("/App/Contents/MacOS/resources/bin").join(p));
        assert_eq!(dirs[1], Path::new("/App/Contents/Resources/bin").join(p));
        assert_eq!(
            dirs[2],
            Path::new("/App/Contents/Resources/resources/bin").join(p)
        );
        assert_eq!(dirs[3], Path::new("/src/proj/resources/bin").join(p));
    }

    #[test]
    fn dirs_without_manifest() {
        assert_eq!(bundled_dirs(None, None).len(), 0);
        assert_eq!(bundled_dirs(Some(Path::new("/x/bin")), None).len(), 3);
    }

    #[test]
    fn names_include_sidecar_triple() {
        let names = candidate_names("ffmpeg");
        assert_eq!(names[0], "ffmpeg");
        assert_eq!(names[1], format!("ffmpeg-{}", target_triple()));
        let win = candidate_names("ffprobe.exe");
        assert!(win[1].starts_with("ffprobe-") && win[1].ends_with(".exe"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_triple_matches_bundled_sidecar_naming() {
        assert!(target_triple().ends_with("-apple-darwin"));
    }

    #[test]
    fn candidate_paths_order_dir_major() {
        let paths = candidate_paths(Some(Path::new("/e")), None, "ffmpeg");
        assert_eq!(paths[0].file_name().unwrap(), "ffmpeg");
        assert_eq!(
            paths[1].file_name().unwrap().to_str().unwrap(),
            format!("ffmpeg-{}", target_triple())
        );
        assert!(paths[0].starts_with("/e/resources/bin"));
        assert!(paths[2].starts_with("/Resources/bin"));
    }
}
