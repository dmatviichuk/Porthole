/// Binary suffixes come first so "Mi" is not read as "M" + junk.
const SUFFIXES: [(&str, f64); 15] = [
    ("Ki", 1024.0),
    ("Mi", 1_048_576.0),
    ("Gi", 1_073_741_824.0),
    ("Ti", 1_099_511_627_776.0),
    ("Pi", 1_125_899_906_842_624.0),
    ("Ei", 1_152_921_504_606_846_976.0),
    ("n", 1e-9),
    ("u", 1e-6),
    ("m", 1e-3),
    ("k", 1e3),
    ("M", 1e6),
    ("G", 1e9),
    ("T", 1e12),
    ("P", 1e15),
    ("E", 1e18),
];

/// Parses a Kubernetes quantity ("250m", "1.5", "128Mi", "1e3", "2G") into base units:
/// cores for CPU, bytes for memory, a plain count for pods.
pub fn parse(quantity: &str) -> Option<f64> {
    let q = quantity.trim();
    for (suffix, multiplier) in SUFFIXES {
        if let Some(number) = q.strip_suffix(suffix)
            && let Ok(value) = number.parse::<f64>()
        {
            return Some(value * multiplier);
        }
    }
    // No suffix, or a decimal exponent such as "1e3".
    q.parse::<f64>().ok()
}

/// CPU in millicores, the unit the UI shows.
pub fn millicores(quantity: &str) -> Option<f64> {
    parse(quantity).map(|cores| cores * 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cpu_memory_and_exponents() {
        assert_eq!(millicores("250m"), Some(250.0));
        assert_eq!(millicores("2"), Some(2000.0));
        assert_eq!(millicores("1.5"), Some(1500.0));
        assert_eq!(millicores("500000n").map(f64::round), Some(1.0));
        assert_eq!(parse("128Mi"), Some(134_217_728.0));
        assert_eq!(parse("16302624Ki"), Some(16_693_886_976.0));
        assert_eq!(parse("2G"), Some(2e9));
        assert_eq!(parse("1e3"), Some(1000.0));
        assert_eq!(parse("1E"), Some(1e18));
        assert_eq!(parse("110"), Some(110.0));
        assert_eq!(parse("lots"), None);
    }
}
