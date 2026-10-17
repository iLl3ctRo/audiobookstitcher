//! UI-free form state and its conversion into stitcher options.

use crate::stitcher::StitchOptions;

pub const LOG_CAP: usize = 250;
pub const EMPTY_SOURCE_ERROR: &str = "Select a source directory to continue.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormState {
    pub source_dir: String,
    pub output_file: String,
    pub title: String,
    pub author: String,
    pub narrator: String,
    pub album: String,
    pub year: String,
    pub cover_image: String,
    pub bitrate: String,
    pub sample_rate: String,
    pub workers: String,
}

impl Default for FormState {
    fn default() -> Self {
        Self {
            source_dir: String::new(),
            output_file: String::new(),
            title: String::new(),
            author: String::new(),
            narrator: String::new(),
            album: String::new(),
            year: String::new(),
            cover_image: String::new(),
            bitrate: "96k".into(),
            sample_rate: "44100".into(),
            workers: String::new(),
        }
    }
}

fn opt(value: &str) -> Option<String> {
    let v = value.trim();
    (!v.is_empty()).then(|| v.to_string())
}

impl FormState {
    /// Mirrors `buildPayload` + the empty-source validation of the React UI.
    pub fn to_options(&self) -> Result<StitchOptions, &'static str> {
        let source_dir = self.source_dir.trim();
        if source_dir.is_empty() {
            return Err(EMPTY_SOURCE_ERROR);
        }
        Ok(StitchOptions {
            source_dir: source_dir.to_string(),
            output_file: opt(&self.output_file),
            title: opt(&self.title),
            author: opt(&self.author),
            narrator: opt(&self.narrator),
            album: opt(&self.album),
            year: opt(&self.year),
            cover_image: opt(&self.cover_image),
            bitrate: opt(&self.bitrate),
            sample_rate: self.sample_rate.trim().parse().ok(),
            workers: self.workers.trim().parse().ok(),
        })
    }
}

/// Last path segment of the source dir, or "audiobook".
pub fn infer_default_name(source_dir: &str) -> String {
    source_dir
        .split(['/', '\\'])
        .filter(|s| !s.is_empty())
        .next_back()
        .unwrap_or("audiobook")
        .to_string()
}

/// Whether the path has one of the allowed extensions (case-insensitive).
pub fn has_extension(path: &str, allowed: &[&str]) -> bool {
    std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| allowed.iter().any(|a| a.eq_ignore_ascii_case(e)))
        .unwrap_or(false)
}

/// Ensures an output path ends in `.m4b`.
pub fn ensure_m4b(path: &str) -> String {
    if has_extension(path, &["m4b"]) {
        path.to_string()
    } else {
        format!("{path}.m4b")
    }
}

/// Appends a line to the log, capped at [`LOG_CAP`] entries.
pub fn push_log(lines: &mut Vec<String>, stamped: String) {
    lines.push(stamped);
    if lines.len() > LOG_CAP {
        let excess = lines.len() - LOG_CAP;
        lines.drain(..excess);
    }
}

/// Percentage 0..=100 for the progress bar.
pub fn percent(current: usize, total: usize) -> u32 {
    if total == 0 {
        0
    } else {
        ((current as f64 / total as f64) * 100.0).round() as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_source_rejected() {
        let f = FormState {
            source_dir: "   ".into(),
            ..Default::default()
        };
        assert_eq!(f.to_options().unwrap_err(), "Select a source directory to continue.");
    }

    #[test]
    fn defaults_and_empty_to_none() {
        let f = FormState {
            source_dir: " /books/x ".into(),
            ..Default::default()
        };
        let o = f.to_options().unwrap();
        assert_eq!(o.source_dir, "/books/x");
        assert_eq!(o.bitrate.as_deref(), Some("96k"));
        assert_eq!(o.sample_rate, Some(44100));
        assert!(o.output_file.is_none() && o.title.is_none() && o.cover_image.is_none());
        assert!(o.workers.is_none());
    }

    #[test]
    fn invalid_ints_ignored_and_values_trimmed() {
        let f = FormState {
            source_dir: "/a".into(),
            sample_rate: "abc".into(),
            workers: "-3".into(),
            title: "  Dune ".into(),
            bitrate: "  ".into(),
            ..Default::default()
        };
        let o = f.to_options().unwrap();
        assert_eq!(o.sample_rate, None);
        assert_eq!(o.workers, None);
        assert_eq!(o.title.as_deref(), Some("Dune"));
        assert_eq!(o.bitrate, None);
        let g = FormState {
            source_dir: "/a".into(),
            workers: " 4 ".into(),
            ..Default::default()
        };
        assert_eq!(g.to_options().unwrap().workers, Some(4));
    }

    #[test]
    fn default_name_inference() {
        assert_eq!(infer_default_name(""), "audiobook");
        assert_eq!(infer_default_name("/a/b/My Book/"), "My Book");
        assert_eq!(infer_default_name("C:\\books\\Dune"), "Dune");
        assert_eq!(infer_default_name("/"), "audiobook");
    }

    #[test]
    fn extension_checks() {
        assert!(has_extension("/x/cover.JPG", &["jpg", "jpeg", "png"]));
        assert!(!has_extension("/x/cover.gif", &["jpg", "jpeg", "png"]));
        assert!(!has_extension("/x/cover", &["jpg"]));
        assert_eq!(ensure_m4b("/x/a"), "/x/a.m4b");
        assert_eq!(ensure_m4b("/x/a.M4B"), "/x/a.M4B");
    }

    #[test]
    fn log_is_capped_at_250() {
        let mut lines = Vec::new();
        for i in 0..300 {
            push_log(&mut lines, format!("line {i}"));
        }
        assert_eq!(lines.len(), 250);
        assert_eq!(lines[0], "line 50");
        assert_eq!(lines[249], "line 299");
    }

    #[test]
    fn percent_math() {
        assert_eq!(percent(0, 0), 0);
        assert_eq!(percent(1, 3), 33);
        assert_eq!(percent(2, 3), 67);
        assert_eq!(percent(5, 5), 100);
    }
}
