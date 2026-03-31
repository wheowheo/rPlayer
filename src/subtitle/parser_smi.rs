use super::SubtitleEntry;

pub fn parse_smi(content: &str) -> Vec<SubtitleEntry> {
    let mut entries = Vec::new();
    let mut pending_start: Option<f64> = None;
    let mut pending_text = String::new();

    for line in content.lines() {
        let trimmed = line.trim();

        // Look for <SYNC Start=...>
        if let Some(start_ms) = extract_sync_start(trimmed) {
            let start_secs = start_ms / 1000.0;

            // Close previous entry
            if let Some(prev_start) = pending_start {
                let text = clean_smi_text(&pending_text);
                if !text.is_empty() && text != "&nbsp;" {
                    entries.push(SubtitleEntry {
                        start: prev_start,
                        end: start_secs,
                        text,
                    });
                }
            }

            pending_start = Some(start_secs);
            pending_text.clear();

            // Text may be on the same line after the SYNC tag
            if let Some(after) = extract_after_sync(trimmed) {
                pending_text = after;
            }
        } else if pending_start.is_some() {
            if !pending_text.is_empty() { pending_text.push(' '); }
            pending_text.push_str(trimmed);
        }
    }

    entries
}

fn extract_sync_start(line: &str) -> Option<f64> {
    let upper = line.to_uppercase();
    let idx = upper.find("<SYNC")?;
    let rest = &upper[idx..];
    let start_idx = rest.find("START=")?;
    let val_start = start_idx + 6;
    let val = &rest[val_start..];
    // Skip optional quote
    let val = val.trim_start_matches('"').trim_start_matches('\'');
    let end = val.find(|c: char| !c.is_ascii_digit() && c != '.').unwrap_or(val.len());
    val[..end].parse::<f64>().ok()
}

fn extract_after_sync(line: &str) -> Option<String> {
    // Find the closing > of <SYNC ...>
    let idx = line.find('>')?;
    let after = line[idx + 1..].trim();
    if after.is_empty() { None } else { Some(after.to_string()) }
}

fn clean_smi_text(text: &str) -> String {
    // Remove HTML tags
    let mut result = String::new();
    let mut in_tag = false;
    for c in text.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(c),
            _ => {}
        }
    }
    result.trim().to_string()
}
