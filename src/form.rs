//! UI-free form state and its conversion into stitcher options.

use crate::stitcher::StitchOptions;

pub const LOG_CAP: usize = 250;
pub const EMPTY_SOURCE_ERROR: &str = "Select a source directory to continue.";
pub const SAMPLE_RATE_ERROR: &str = "Sample rate must be a whole number, e.g. 44100";
pub const BITRATE_ERROR: &str = "Bitrate must look like 96k, 128k or 64000";
pub const WORKERS_ERROR: &str = "Workers must be a whole number or empty";
pub const COVER_ERROR: &str = "Cover image must be a .jpg, .jpeg or .png file.";

/// Empty, or a number with an optional k/M suffix that ffmpeg's `-b:a` accepts.
fn is_valid_bitrate(value: &str) -> bool {
    let v = value.trim();
    let digits = v.strip_suffix(['k', 'K', 'm', 'M']).unwrap_or(v);
    if v.is_empty() {
        return true;
    }
    let mut parts = digits.splitn(2, '.');
    let whole = parts.next().unwrap_or("");
    let frac = parts.next();
    !whole.is_empty()
        && whole.chars().all(|c| c.is_ascii_digit())
        && frac.is_none_or(|f| !f.is_empty() && f.chars().all(|c| c.is_ascii_digit()))
        && digits.chars().any(|c| c != '0' && c != '.')
}

/// Empty -> `Ok(None)`; a positive integer -> `Ok(Some(n))`; anything else -> `Err(())`.
fn parse_positive<T: std::str::FromStr + PartialEq + Default>(
    value: &str,
) -> Result<Option<T>, ()> {
    let v = value.trim();
    if v.is_empty() {
        return Ok(None);
    }
    match v.parse::<T>() {
        Ok(n) if n != T::default() => Ok(Some(n)),
        _ => Err(()),
    }
}

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
    /// Mirrors `buildPayload` + the React validation, but rejects (instead of
    /// silently ignoring) a non-empty sample rate / workers that isn't a positive integer.
    pub fn to_options(&self) -> Result<StitchOptions, &'static str> {
        let source_dir = self.source_dir.trim();
        if source_dir.is_empty() {
            return Err(EMPTY_SOURCE_ERROR);
        }
        if !is_valid_bitrate(&self.bitrate) {
            return Err(BITRATE_ERROR);
        }
        let sample_rate = parse_positive(&self.sample_rate).map_err(|_| SAMPLE_RATE_ERROR)?;
        let workers = parse_positive(&self.workers).map_err(|_| WORKERS_ERROR)?;
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
            sample_rate,
            workers,
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
    fn bitrate_validation() {
        for ok in ["", "  ", "96k", "128K", "64000", "1.5M", " 96k "] {
            assert!(is_valid_bitrate(ok), "{ok:?} should be valid");
        }
        for bad in [
            "abc", "k", "96 k", "-96k", "0", "0k", "1.k", ".5k", "96kbps", "9e3",
        ] {
            assert!(!is_valid_bitrate(bad), "{bad:?} should be invalid");
        }
        let f = FormState {
            source_dir: "/x".into(),
            bitrate: "abc".into(),
            ..FormState::default()
        };
        assert_eq!(f.to_options().unwrap_err(), BITRATE_ERROR);
    }

    #[test]
    fn empty_source_rejected() {
        let f = FormState {
            source_dir: "   ".into(),
            ..Default::default()
        };
        assert_eq!(
            f.to_options().unwrap_err(),
            "Select a source directory to continue."
        );
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
    fn values_trimmed_and_ints_parsed() {
        let f = FormState {
            source_dir: "/a".into(),
            title: "  Dune ".into(),
            bitrate: "  ".into(),
            workers: " 4 ".into(),
            sample_rate: " 48000 ".into(),
            ..Default::default()
        };
        let o = f.to_options().unwrap();
        assert_eq!(o.title.as_deref(), Some("Dune"));
        assert_eq!(o.bitrate, None);
        assert_eq!(o.workers, Some(4));
        assert_eq!(o.sample_rate, Some(48000));
    }

    #[test]
    fn empty_sample_rate_and_workers_are_none() {
        let f = FormState {
            source_dir: "/a".into(),
            sample_rate: " ".into(),
            ..Default::default()
        };
        let o = f.to_options().unwrap();
        assert_eq!((o.sample_rate, o.workers), (None, None));
    }

    #[test]
    fn invalid_sample_rate_blocks() {
        for bad in ["abc", "-1", "0", "44.1k", "44100x"] {
            let f = FormState {
                source_dir: "/a".into(),
                sample_rate: bad.into(),
                ..Default::default()
            };
            assert_eq!(f.to_options().unwrap_err(), SAMPLE_RATE_ERROR, "{bad}");
        }
    }

    #[test]
    fn invalid_workers_blocks() {
        for bad in ["abc", "-3", "0", "2.5"] {
            let f = FormState {
                source_dir: "/a".into(),
                workers: bad.into(),
                ..Default::default()
            };
            assert_eq!(f.to_options().unwrap_err(), WORKERS_ERROR, "{bad}");
        }
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
