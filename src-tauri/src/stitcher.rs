use std::{
    collections::BTreeMap,
    env,
    ffi::{OsStr, OsString},
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Instant,
};

use futures::{stream, StreamExt};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use shellexpand::tilde;
use tauri::{AppHandle, Emitter, Manager, Window};
use tempfile::TempDir;
use tokio::process::Command;

#[derive(Debug, thiserror::Error)]
pub enum StitchError {
    #[error("source directory not found: {0}")]
    SourceNotFound(String),
    #[error("source directory is not a directory: {0}")]
    SourceNotDirectory(String),
    #[error("no supported audio files found in {0}")]
    NoAudioFiles(String),
    #[error("cover image not found: {0}")]
    CoverNotFound(String),
    #[error("failed to determine resource directory")]
    ResourcePath,
    #[error("ffmpeg not found. add it to PATH or bundle it under resources/bin.")]
    FfmpegNotFound,
    #[error("ffprobe not found. add it to PATH or bundle it under resources/bin.")]
    FfprobeNotFound,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("utf-8 error: {0}")]
    Utf8(#[from] std::str::Utf8Error),
    #[error("ffmpeg failed (code {code:?}): {command}\n{stderr}")]
    FfmpegFailed {
        command: String,
        code: Option<i32>,
        stderr: String,
    },
    #[error("ffprobe failed (code {code:?}): {command}\n{stderr}")]
    FfprobeFailed {
        command: String,
        code: Option<i32>,
        stderr: String,
    },
    #[error("failed to parse ffprobe duration from output: {0}")]
    InvalidProbeOutput(String),
    #[error("{0}")]
    Other(String),
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum ProgressStage {
    Scanning,
    Normalizing,
    Finalizing,
    Complete,
    Failed,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StitchProgress {
    pub stage: ProgressStage,
    pub current: usize,
    pub total: usize,
    pub message: Option<String>,
}

#[derive(Debug, Clone)]
struct NormalizedTrack {
    normalized: PathBuf,
    duration_ms: u64,
    title: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StitchResult {
    pub output: String,
    pub file_count: usize,
    pub duration_ms: u128,
}

#[serde_as]
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StitchOptions {
    pub source_dir: String,
    pub output_file: Option<String>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub narrator: Option<String>,
    pub album: Option<String>,
    pub year: Option<String>,
    pub cover_image: Option<String>,
    pub bitrate: Option<String>,
    pub sample_rate: Option<u32>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub workers: Option<usize>,
}

static SUPPORTED_EXTENSIONS: &[&str] = &["aac", "aiff", "flac", "m4a", "m4b", "mp3", "ogg", "wav"];

static NATURAL_TOKEN_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(\d+|\D+)").expect("valid natural sort regex"));

pub async fn stitch_audiobook(
    window: Window,
    options: StitchOptions,
) -> Result<StitchResult, StitchError> {
    let start = Instant::now();
    let app_handle = window.app_handle();

    let source_dir = expand_path(&options.source_dir);
    let source_dir = source_dir
        .canonicalize()
        .map_err(|_| StitchError::SourceNotFound(options.source_dir.clone()))?;
    if !source_dir.is_dir() {
        return Err(StitchError::SourceNotDirectory(
            source_dir.display().to_string(),
        ));
    }

    let audio_files = collect_audio_files(&source_dir)?;
    if audio_files.is_empty() {
        return Err(StitchError::NoAudioFiles(source_dir.display().to_string()));
    }

    emit_progress(
        &window,
        ProgressStage::Scanning,
        0,
        audio_files.len(),
        Some(format!("Found {} audio files", audio_files.len())),
    );

    let cover_image_path = match options.cover_image.clone() {
        Some(path) => {
            let expanded = expand_path(&path);
            let canonical = expanded
                .canonicalize()
                .map_err(|_| StitchError::CoverNotFound(path))?;
            if !canonical.is_file() {
                return Err(StitchError::CoverNotFound(canonical.display().to_string()));
            }
            Some(canonical)
        }
        None => None,
    };

    let output_path = determine_output_path(&options, &source_dir)?;
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let ffmpeg_path = resolve_ffmpeg(&app_handle)?;
    let ffprobe_path = resolve_ffprobe(&app_handle, &ffmpeg_path)?;

    let temp_dir = TempDir::new()?;
    let temp_path = temp_dir.path().to_path_buf();

    let workers = normalize_workers(options.workers);
    let bitrate = options.bitrate.clone().unwrap_or_else(|| "96k".to_string());
    let sample_rate = options.sample_rate.unwrap_or(44_100);

    emit_progress(
        &window,
        ProgressStage::Normalizing,
        0,
        audio_files.len(),
        Some(format!(
            "Normalizing {} sources using {} worker(s)",
            audio_files.len(),
            workers
        )),
    );

    let normalized_files = normalize_sources(
        &window,
        &ffmpeg_path,
        &ffprobe_path,
        &audio_files,
        &temp_path,
        workers,
        &bitrate,
        sample_rate,
    )
    .await?;

    let metadata = build_metadata_map(&options);
    let chapter_metadata = write_chapter_metadata(&normalized_files, &temp_path)?;

    emit_progress(
        &window,
        ProgressStage::Finalizing,
        0,
        1,
        Some("Combining normalized audio and embedding metadata".to_string()),
    );

    let concat_manifest = temp_path.join("concat.txt");
    write_concat_manifest(&normalized_files, &concat_manifest)?;

    finalize_audiobook(
        &ffmpeg_path,
        &concat_manifest,
        &output_path,
        &cover_image_path,
        &metadata,
        Some(&chapter_metadata),
    )
    .await?;

    emit_progress(
        &window,
        ProgressStage::Complete,
        audio_files.len(),
        audio_files.len(),
        Some(format!("Created audiobook at {}", output_path.display())),
    );

    Ok(StitchResult {
        output: output_path.display().to_string(),
        file_count: audio_files.len(),
        duration_ms: start.elapsed().as_millis(),
    })
}

fn emit_progress(
    window: &Window,
    stage: ProgressStage,
    current: usize,
    total: usize,
    message: Option<String>,
) {
    let payload = StitchProgress {
        stage,
        current,
        total,
        message,
    };
    if let Err(err) = window.emit("stitch-progress", payload) {
        log::warn!("failed to emit progress event: {err}");
    }
}

fn expand_path(path: &str) -> PathBuf {
    PathBuf::from(tilde(path).into_owned())
}

fn collect_audio_files(directory: &Path) -> Result<Vec<PathBuf>, StitchError> {
    let mut files: Vec<PathBuf> = fs::read_dir(directory)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().map(|ft| ft.is_file()).unwrap_or(false))
        .filter(|entry| {
            entry
                .path()
                .extension()
                .and_then(OsStr::to_str)
                .map(|ext| SUPPORTED_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
                .unwrap_or(false)
        })
        .map(|entry| entry.path())
        .collect();

    files.sort_by(|a, b| natural_compare(a, b));
    Ok(files)
}

fn natural_compare(a: &Path, b: &Path) -> std::cmp::Ordering {
    let a_key = natural_key(a);
    let b_key = natural_key(b);
    a_key.cmp(&b_key)
}

fn natural_key(path: &Path) -> Vec<NaturalComponent> {
    let name = path
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    NATURAL_TOKEN_RE
        .captures_iter(&name)
        .filter_map(|caps| caps.get(0))
        .map(|m| {
            let token = m.as_str();
            if let Ok(num) = token.parse::<u64>() {
                NaturalComponent::Number(num)
            } else {
                NaturalComponent::Text(token.to_string())
            }
        })
        .collect()
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum NaturalComponent {
    Number(u64),
    Text(String),
}

fn determine_output_path(
    options: &StitchOptions,
    source_dir: &Path,
) -> Result<PathBuf, StitchError> {
    if let Some(ref explicit) = options.output_file {
        return Ok(expand_path(explicit));
    }

    let default_name = source_dir
        .file_name()
        .and_then(OsStr::to_str)
        .filter(|name| !name.is_empty())
        .map(|name| format!("{name}.m4b"))
        .unwrap_or_else(|| "audiobook.m4b".to_string());
    Ok(source_dir.join(default_name))
}

fn normalize_workers(workers: Option<usize>) -> usize {
    match workers {
        Some(value) if value > 0 => value,
        _ => num_cpus::get().max(1),
    }
}

async fn normalize_sources(
    window: &Window,
    ffmpeg: &Path,
    ffprobe: &Path,
    sources: &[PathBuf],
    temp_dir: &Path,
    workers: usize,
    bitrate: &str,
    sample_rate: u32,
) -> Result<Vec<NormalizedTrack>, StitchError> {
    let total = sources.len();
    let completed = Arc::new(AtomicUsize::new(0));
    let mut tasks = stream::iter(sources.iter().cloned().enumerate().map(|(index, source)| {
        let window = window.clone();
        let ffmpeg = ffmpeg.to_path_buf();
        let ffprobe = ffprobe.to_path_buf();
        let temp_dir = temp_dir.to_path_buf();
        let bitrate = bitrate.to_string();
        let progress_counter = completed.clone();
        async move {
            let target = temp_dir.join(format!("{:04}.m4a", index + 1));
            let mut args: Vec<OsString> = Vec::with_capacity(16);
            args.push("-y".into());
            args.push("-hide_banner".into());
            args.push("-loglevel".into());
            args.push("error".into());
            args.push("-i".into());
            args.push(source.as_os_str().to_os_string());
            args.push("-c:a".into());
            args.push("aac".into());
            args.push("-b:a".into());
            args.push(bitrate.clone().into());
            args.push("-ar".into());
            args.push(sample_rate.to_string().into());
            args.push("-vn".into());
            args.push(target.as_os_str().to_os_string());

            match run_ffmpeg(&ffmpeg, args).await {
                Ok(()) => {
                    let duration_ms = probe_duration(&ffprobe, &target).await?;
                    let title = derive_chapter_title(&source, index + 1);
                    let finished = progress_counter.fetch_add(1, Ordering::SeqCst) + 1;
                    emit_progress(
                        &window,
                        ProgressStage::Normalizing,
                        finished,
                        total,
                        Some(format!(
                            "Normalized {} ({}/{})",
                            source
                                .file_name()
                                .and_then(OsStr::to_str)
                                .unwrap_or_default(),
                            finished,
                            total
                        )),
                    );
                            Ok((
                                index,
                                NormalizedTrack {
                                    normalized: target,
                                    duration_ms,
                                    title,
                                },
                            ))
                }
                Err(err) => {
                    emit_progress(
                        &window,
                        ProgressStage::Failed,
                        progress_counter.load(Ordering::SeqCst),
                        total,
                        Some(format!("Failed to normalize {}: {err}", source.display())),
                    );
                    Err(err)
                }
            }
        }
    }))
    .buffer_unordered(workers);

    let mut sorted: Vec<Option<NormalizedTrack>> = vec![None; total];
    while let Some(result) = tasks.next().await {
        let (index, track) = result?;
        sorted[index] = Some(track);
    }

    let mut normalized = Vec::with_capacity(total);
    for entry in sorted {
        let track = entry.ok_or_else(|| StitchError::Other("Normalisation task failed".into()))?;
        normalized.push(track);
    }
    Ok(normalized)
}

fn write_concat_manifest(
    tracks: &[NormalizedTrack],
    manifest_path: &Path,
) -> Result<(), StitchError> {
    let mut manifest = File::create(manifest_path)?;
    for track in tracks {
        writeln!(manifest, "file {}", quote_path(&track.normalized))?;
    }
    Ok(())
}

fn write_chapter_metadata(
    tracks: &[NormalizedTrack],
    temp_dir: &Path,
) -> Result<PathBuf, StitchError> {
    let metadata_path = temp_dir.join("chapters.ffmeta");
    let mut file = File::create(&metadata_path)?;
    writeln!(file, ";FFMETADATA1")?;

    let mut cursor = 0u64;
    for track in tracks {
        let duration = track.duration_ms.max(1);
        let end = cursor + duration;
        writeln!(file)?;
        writeln!(file, "[CHAPTER]")?;
        writeln!(file, "TIMEBASE=1/1000")?;
        writeln!(file, "START={cursor}")?;
        writeln!(file, "END={end}")?;
        writeln!(file, "title={}", escape_ffmetadata_value(&track.title))?;
        cursor = end;
    }

    Ok(metadata_path)
}

fn quote_path(path: &Path) -> String {
    let path_str = path.display().to_string();
    let mut quoted = String::with_capacity(path_str.len() + 2);
    quoted.push('\'');
    for ch in path_str.chars() {
        if ch == '\'' {
            quoted.push_str("'\\''");
        } else {
            quoted.push(ch);
        }
    }
    quoted.push('\'');
    quoted
}

fn escape_ffmetadata_value(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' | ';' | '#' | '=' => {
                escaped.push('\\');
                escaped.push(ch);
            }
            '\n' => {
                escaped.push('\\');
                escaped.push('n');
            }
            '\r' => {
                escaped.push('\\');
                escaped.push('r');
            }
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn derive_chapter_title(source: &Path, index: usize) -> String {
    let fallback = format!("Chapter {}", index);
    let stem = match source.file_stem().and_then(OsStr::to_str) {
        Some(raw) if !raw.trim().is_empty() => raw,
        _ => return fallback,
    };
    let cleaned = stem.replace('_', " ").replace('-', " ");
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        fallback
    } else {
        trimmed.to_string()
    }
}

async fn finalize_audiobook(
    ffmpeg: &Path,
    concat_manifest: &Path,
    output_file: &Path,
    cover_image: &Option<PathBuf>,
    metadata: &BTreeMap<String, String>,
    chapter_metadata: Option<&Path>,
) -> Result<(), StitchError> {
    let mut args: Vec<OsString> = Vec::with_capacity(32);
    args.push("-y".into());
    args.push("-hide_banner".into());
    args.push("-loglevel".into());
    args.push("error".into());
    args.push("-f".into());
    args.push("concat".into());
    args.push("-safe".into());
    args.push("0".into());
    args.push("-i".into());
    args.push(concat_manifest.as_os_str().to_os_string());

    let mut next_input_index = 1usize;
    let concat_index = 0usize;
    let mut cover_index = None;
    if let Some(cover) = cover_image {
        args.push("-i".into());
        args.push(cover.as_os_str().to_os_string());
        cover_index = Some(next_input_index);
        next_input_index += 1;
    }

    let mut chapter_index = None;
    if let Some(chapter_file) = chapter_metadata {
        args.push("-f".into());
        args.push("ffmetadata".into());
        args.push("-i".into());
        args.push(chapter_file.as_os_str().to_os_string());
        chapter_index = Some(next_input_index);
    }

    for (key, value) in metadata {
        args.push("-metadata".into());
        args.push(format!("{key}={value}").into());
    }

    args.push("-c:a".into());
    args.push("copy".into());
    args.push("-movflags".into());
    args.push("+faststart".into());

    if let Some(idx) = cover_index {
        args.push("-map".into());
        args.push(format!("{concat_index}:a").into());
        args.push("-map".into());
        args.push(format!("{idx}:v").into());
        args.push("-c:v".into());
        args.push("mjpeg".into());
        args.push("-disposition:v".into());
        args.push("attached_pic".into());
        args.push("-metadata:s:v".into());
        args.push("title=Cover".into());
        args.push("-metadata:s:v".into());
        args.push("comment=Cover (front)".into());
    }

    if let Some(idx) = chapter_index {
        args.push("-map_chapters".into());
        args.push(idx.to_string().into());
    }

    args.push(output_file.as_os_str().to_os_string());

    run_ffmpeg(ffmpeg, args).await
}

async fn run_ffmpeg(ffmpeg: &Path, args: Vec<OsString>) -> Result<(), StitchError> {
    let mut command = Command::new(ffmpeg);
    command.args(&args);
    let command_str = to_command_string(ffmpeg, &args);

    let output = command.output().await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(StitchError::FfmpegFailed {
            command: command_str,
            code: output.status.code(),
            stderr,
        });
    }

    Ok(())
}

async fn probe_duration(ffprobe: &Path, file: &Path) -> Result<u64, StitchError> {
    let mut args: Vec<OsString> = Vec::with_capacity(12);
    args.push("-v".into());
    args.push("error".into());
    args.push("-select_streams".into());
    args.push("a:0".into());
    args.push("-show_entries".into());
    args.push("stream=duration".into());
    args.push("-of".into());
    args.push("default=noprint_wrappers=1:nokey=1".into());
    args.push(file.as_os_str().to_os_string());

    let mut command = Command::new(ffprobe);
    command.args(&args);
    let command_str = to_command_string(ffprobe, &args);

    let output = command.output().await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(StitchError::FfprobeFailed {
            command: command_str,
            code: output.status.code(),
            stderr,
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Err(StitchError::InvalidProbeOutput("empty duration".into()));
    }

    let seconds: f64 = trimmed
        .parse()
        .map_err(|_| StitchError::InvalidProbeOutput(trimmed.to_string()))?;
    if !seconds.is_finite() {
        return Err(StitchError::InvalidProbeOutput(trimmed.to_string()));
    }

    let millis = (seconds.max(0.0) * 1000.0).round();
    if !millis.is_finite() {
        return Err(StitchError::InvalidProbeOutput(trimmed.to_string()));
    }

    let duration_ms = millis as u64;
    Ok(duration_ms.max(1))
}

fn to_command_string(executable: &Path, args: &[OsString]) -> String {
    let mut output = executable.display().to_string();
    for arg in args {
        output.push(' ');
        output.push_str(&arg.to_string_lossy());
    }
    output
}

fn resolve_ffprobe(app: &AppHandle, ffmpeg_path: &Path) -> Result<PathBuf, StitchError> {
    if let Ok(custom) = env::var("FFPROBE_PATH") {
        let candidate = expand_path(&custom);
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    if let Ok(path) = which::which("ffprobe") {
        return Ok(path);
    }

    if let Some(parent) = ffmpeg_path.parent() {
        let candidate = parent.join(ffprobe_executable_name());
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|_| StitchError::ResourcePath)?;
    let bin_dir = resource_dir.join("bin").join(current_platform_directory());
    let candidate = bin_dir.join(ffprobe_executable_name());
    if candidate.exists() {
        return Ok(candidate);
    }

    Err(StitchError::FfprobeNotFound)
}

fn resolve_ffmpeg(app: &AppHandle) -> Result<PathBuf, StitchError> {
    if let Ok(custom) = env::var("FFMPEG_PATH") {
        let candidate = expand_path(&custom);
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    if let Ok(path) = which::which("ffmpeg") {
        return Ok(path);
    }

    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|_| StitchError::ResourcePath)?;
    let bin_dir = resource_dir.join("bin").join(current_platform_directory());
    let candidate = bin_dir.join(ffmpeg_executable_name());
    if candidate.exists() {
        return Ok(candidate);
    }

    Err(StitchError::FfmpegNotFound)
}

fn current_platform_directory() -> &'static str {
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

fn ffmpeg_executable_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    }
}

fn ffprobe_executable_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "ffprobe.exe"
    } else {
        "ffprobe"
    }
}

fn build_metadata_map(options: &StitchOptions) -> BTreeMap<String, String> {
    let mut metadata = BTreeMap::new();
    if let Some(title) = options.title.as_ref().filter(|s| !s.is_empty()) {
        metadata.insert("title".into(), title.clone());
    }
    if let Some(author) = options.author.as_ref().filter(|s| !s.is_empty()) {
        metadata.insert("artist".into(), author.clone());
    }
    if let Some(narrator) = options.narrator.as_ref().filter(|s| !s.is_empty()) {
        metadata.insert("album_artist".into(), narrator.clone());
    }
    if let Some(album) = options.album.as_ref().filter(|s| !s.is_empty()) {
        metadata.insert("album".into(), album.clone());
    } else if let Some(title) = options.title.as_ref().filter(|s| !s.is_empty()) {
        metadata.insert("album".into(), title.clone());
    }
    if let Some(year) = options.year.as_ref().filter(|s| !s.is_empty()) {
        metadata.insert("date".into(), year.clone());
    }

    metadata
}
