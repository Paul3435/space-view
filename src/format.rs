//! Human-readable formatting helpers.

/// Formats a byte count using binary units (like Explorer: 1 KB = 1024 bytes).
pub fn bytes(n: u64) -> String {
    const UNITS: [&str; 6] = ["KB", "MB", "GB", "TB", "PB", "EB"];
    if n < 1024 {
        return format!("{n} B");
    }
    let mut value = n as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if value >= 100.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else if value >= 10.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

/// Formats an integer with thousands separators: 1234567 -> "1,234,567".
pub fn count(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

pub fn percent(part: u64, whole: u64) -> String {
    if whole == 0 {
        return "–".to_owned();
    }
    let p = part as f64 * 100.0 / whole as f64;
    if p >= 10.0 {
        format!("{p:.0}%")
    } else {
        format!("{p:.1}%")
    }
}

pub fn duration(secs: f64) -> String {
    if secs < 60.0 {
        format!("{secs:.1} s")
    } else {
        let m = (secs / 60.0).floor();
        format!("{m:.0} min {:.0} s", secs - m * 60.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_bytes() {
        assert_eq!(bytes(0), "0 B");
        assert_eq!(bytes(1023), "1023 B");
        assert_eq!(bytes(1024), "1.00 KB");
        assert_eq!(bytes(1536), "1.50 KB");
        assert_eq!(bytes(10 * 1024 * 1024), "10.0 MB");
        assert_eq!(bytes(500 * 1024 * 1024 * 1024), "500 GB");
        assert_eq!(bytes(u64::MAX), "16.0 EB");
    }

    #[test]
    fn formats_counts_and_percentages() {
        assert_eq!(count(0), "0");
        assert_eq!(count(999), "999");
        assert_eq!(count(1000), "1,000");
        assert_eq!(count(1234567), "1,234,567");
        assert_eq!(percent(1, 3), "33%");
        assert_eq!(percent(1, 300), "0.3%");
        assert_eq!(percent(1, 0), "–");
    }
}
