use stratum_core::mode::Mode;

pub fn filter_by_mode(source: &str, mode: Mode) -> String {
    let mut out = String::with_capacity(source.len());
    let mut keep = true;
    for line in source.lines() {
        if let Some(rest) = line.strip_prefix("<!-- stratum:mode:") {
            let directive = rest.strip_suffix("-->").unwrap_or(rest).trim();
            keep = directive == "all"
                || directive.split(',').any(|m| m.trim() == mode.as_str());
            continue;
        }
        if keep {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# Rules\n<!-- stratum:mode:all -->\nall\n<!-- stratum:mode:full,ultra -->\nfull\n<!-- stratum:mode:ultra -->\nultra\n";

    #[test]
    fn off_mode_strips_full_rules() {
        let out = filter_by_mode(SAMPLE, Mode::Off);
        assert!(out.contains("all"));
        assert!(!out.contains("full"));
        assert!(!out.contains("ultra"));
    }

    #[test]
    fn ultra_mode_includes_everything() {
        let out = filter_by_mode(SAMPLE, Mode::Ultra);
        assert!(out.contains("all"));
        assert!(out.contains("full"));
        assert!(out.contains("ultra"));
    }
}
