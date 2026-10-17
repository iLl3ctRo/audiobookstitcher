//! End-to-end run of the stitcher with real ffmpeg. Run with:
//! `cargo test --test e2e -- --ignored`

use std::{
    path::Path,
    process::Command,
    sync::{Arc, Mutex},
};

use audiobook_stitcher::stitcher::{stitch_audiobook, ProgressSink, StitchOptions};

fn tool(name: &str) -> std::path::PathBuf {
    which::which(name)
        .ok()
        .or_else(|| audiobook_stitcher::binaries::find_bundled(name))
        .unwrap_or_else(|| panic!("{name} not found on PATH or in resources/bin"))
}

fn make_sine(ffmpeg: &Path, out: &Path, freq: u32) {
    let status = Command::new(ffmpeg)
        .args([
            "-y",
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
        ])
        .arg(format!("sine=frequency={freq}:duration=2"))
        .arg(out)
        .status()
        .unwrap();
    assert!(status.success());
}

#[tokio::test]
#[ignore = "requires ffmpeg/ffprobe"]
async fn stitches_three_files_into_three_chapters() {
    let ffmpeg = tool("ffmpeg");
    let ffprobe = tool("ffprobe");
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("My Book");
    std::fs::create_dir(&src).unwrap();
    for (i, freq) in [(1, 440), (2, 550), (10, 660)] {
        make_sine(&ffmpeg, &src.join(format!("chapter_{i}.wav")), freq);
    }
    let output = dir.path().join("out").join("book.m4b");

    let events = Arc::new(Mutex::new(Vec::new()));
    let sink: ProgressSink = {
        let events = events.clone();
        Arc::new(move |p| events.lock().unwrap().push(p))
    };
    let result = stitch_audiobook(
        sink,
        StitchOptions {
            source_dir: src.display().to_string(),
            output_file: Some(output.display().to_string()),
            title: Some("Test Book".into()),
            ..Default::default()
        },
    )
    .await
    .expect("stitch failed");

    assert_eq!(result.file_count, 3);
    assert!(output.is_file());
    assert!(!events.lock().unwrap().is_empty());

    let probe = Command::new(&ffprobe)
        .args(["-v", "error", "-show_chapters", "-of", "json"])
        .arg(&output)
        .output()
        .unwrap();
    let json = String::from_utf8_lossy(&probe.stdout);
    let chapters = json.matches("\"start_time\"").count();
    assert_eq!(chapters, 3, "ffprobe output: {json}");
    // natural sort: chapter_10 comes last
    assert!(json.contains("chapter 10"), "ffprobe output: {json}");
}
