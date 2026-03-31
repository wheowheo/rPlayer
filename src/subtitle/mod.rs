pub mod parser_srt;
pub mod parser_smi;

#[derive(Debug, Clone)]
pub struct SubtitleEntry {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

pub struct SubtitleTrack {
    entries: Vec<SubtitleEntry>,
    sync_offset: f64,
}

impl SubtitleTrack {
    pub fn new(entries: Vec<SubtitleEntry>) -> Self {
        Self {
            entries,
            sync_offset: 0.0,
        }
    }

    pub fn load_file(path: &str) -> Option<Self> {
        let content = std::fs::read_to_string(path).ok()?;
        let lower = path.to_lowercase();

        let entries = if lower.ends_with(".srt") {
            parser_srt::parse_srt(&content)
        } else if lower.ends_with(".smi") || lower.ends_with(".sami") {
            parser_smi::parse_smi(&content)
        } else {
            return None;
        };

        if entries.is_empty() { return None; }
        Some(Self::new(entries))
    }

    pub fn adjust_sync(&mut self, delta: f64) {
        self.sync_offset += delta;
    }

    pub fn current_text(&self, time: f64) -> Option<&str> {
        let t = time - self.sync_offset;
        // Binary search for the entry
        let idx = self.entries.partition_point(|e| e.start <= t);
        if idx == 0 { return None; }
        let entry = &self.entries[idx - 1];
        if t >= entry.start && t <= entry.end {
            Some(&entry.text)
        } else {
            None
        }
    }
}
