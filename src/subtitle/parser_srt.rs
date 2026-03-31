use super::SubtitleEntry;

pub fn parse_srt(content: &str) -> Vec<SubtitleEntry> {
    let mut entries = Vec::new();
    let mut lines = content.lines().peekable();

    while lines.peek().is_some() {
        // Skip sequence number
        if let Some(line) = lines.next() {
            let line = line.trim();
            if line.is_empty() { continue; }
            if line.parse::<u32>().is_err() { continue; }
        }

        // Parse timestamp line: 00:01:23,456 --> 00:01:25,789
        let Some(ts_line) = lines.next() else { break };
        let Some((start, end)) = parse_srt_timestamp_line(ts_line.trim()) else { continue };

        // Collect text lines until empty line
        let mut text = String::new();
        while let Some(line) = lines.peek() {
            if line.trim().is_empty() {
                lines.next();
                break;
            }
            if !text.is_empty() { text.push('\n'); }
            text.push_str(lines.next().unwrap().trim());
        }

        if !text.is_empty() {
            entries.push(SubtitleEntry { start, end, text });
        }
    }

    entries
}

fn parse_srt_timestamp_line(line: &str) -> Option<(f64, f64)> {
    let parts: Vec<&str> = line.split("-->").collect();
    if parts.len() != 2 { return None; }
    let start = parse_srt_time(parts[0].trim())?;
    let end = parse_srt_time(parts[1].trim())?;
    Some((start, end))
}

fn parse_srt_time(s: &str) -> Option<f64> {
    // Format: HH:MM:SS,mmm
    let s = s.replace(',', ".");
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 3 { return None; }
    let h: f64 = parts[0].parse().ok()?;
    let m: f64 = parts[1].parse().ok()?;
    let s: f64 = parts[2].parse().ok()?;
    Some(h * 3600.0 + m * 60.0 + s)
}
