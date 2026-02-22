//! Content filtering rules and patterns for the output sanitizer.

/// Content filter patterns: (pattern, replacement).
pub fn get_content_filter_patterns() -> Vec<(&'static str, &'static str)> {
    vec![
        // Self-harm indicators (replace with resources)
        ("I want to kill myself",
         "If you're having thoughts of self-harm, please reach out to a crisis helpline: 988"),
        ("I want to die",
         "If you're having thoughts of self-harm, please reach out to a crisis helpline: 988"),
        // Dangerous instructions
        ("how to make a bomb", "[CONTENT FILTERED: Dangerous content]"),
        ("how to create a virus", "[CONTENT FILTERED: Dangerous content]"),
    ]
}

/// Apply content filter patterns to text. Returns (filtered_text, count).
pub fn filter_content_patterns(text: &str) -> (String, usize) {
    let mut result = text.to_string();
    let mut count = 0;
    for (pattern, replacement) in get_content_filter_patterns() {
        if result.to_lowercase().contains(pattern) {
            result = result.replace(pattern, replacement);
            count += 1;
        }
    }
    (result, count)
}

/// Check for excessive repetition (model degradation indicator).
pub fn has_excessive_repetition(text: &str) -> bool {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() < 10 {
        return false;
    }
    let mut phrase_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for window in words.windows(3) {
        let phrase = window.join(" ");
        *phrase_counts.entry(phrase).or_insert(0) += 1;
    }
    phrase_counts.values().any(|&count| count > 5)
}

/// Validate output format for common issues.
pub fn validate_format(output: &str) -> Result<(), String> {
    if output.chars().any(|c| c == '\0') {
        return Err("Output contains null characters".to_string());
    }
    if has_excessive_repetition(output) {
        return Err("Output contains excessive repetition".to_string());
    }
    if output.contains('\u{00c3}') || output.contains('\u{00c2}') {
        return Err("Output may have encoding issues".to_string());
    }
    Ok(())
}

/// Find a safe trim point that doesn't split potential PII patterns.
pub fn find_safe_trim_point(buffer: &str, max_trim: usize) -> usize {
    const MAX_PII_LENGTH: usize = 100;

    if buffer.len() <= MAX_PII_LENGTH {
        return 0;
    }

    let candidate = max_trim.min(buffer.len() - MAX_PII_LENGTH);
    let search_start = candidate.saturating_sub(20);
    let search_end = (candidate + 20).min(buffer.len());

    if let Some(safe_pos) = buffer[search_start..search_end]
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_whitespace() || *c == '.' || *c == ',' || *c == ';' || *c == ':')
        .map(|(i, _)| search_start + i)
    {
        if safe_pos > 0 && safe_pos <= max_trim {
            return safe_pos;
        }
    }

    buffer.len().saturating_sub(MAX_PII_LENGTH * 2).min(max_trim)
}
