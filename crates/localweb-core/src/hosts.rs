//! Transforms the exact `LocalWeb` block in `/etc/hosts`.
//!
//! All functions preserve content outside the managed marker pair and reject malformed blocks.

use crate::{LocalWebError, Result, validate_domain};
use std::collections::BTreeSet;

/// Starts the LocalWeb-owned hosts section.
pub const BEGIN: &str = "# BEGIN LOCALWEB - DO NOT EDIT";
/// Ends the LocalWeb-owned hosts section.
pub const END: &str = "# END LOCALWEB";

/// Replaces the managed hosts section with exact loopback mappings.
///
/// Hostnames are validated, deduplicated, sorted, and mapped only to `127.0.0.1`.
/// Every line outside the `LocalWeb` block is preserved.
///
/// # Errors
///
/// Returns an error for invalid hostnames or malformed `LocalWeb` marker pairs.
pub fn transform_hosts(existing: &str, domains: &[String]) -> Result<String> {
    let mut names = BTreeSet::new();
    for domain in domains {
        validate_domain(domain)?;
        names.insert(domain.as_str());
    }
    let mut output = remove_managed_block(existing)?;
    output.push_str(BEGIN);
    output.push('\n');
    for name in names {
        output.push_str("127.0.0.1 ");
        output.push_str(name);
        output.push('\n');
    }
    output.push_str(END);
    output.push('\n');
    Ok(output)
}

/// Removes the managed hosts section while preserving all other lines.
///
/// # Errors
///
/// Returns an error when marker pairs are duplicated, unmatched, or unterminated.
pub fn remove_managed_block(existing: &str) -> Result<String> {
    let mut output = String::new();
    let mut in_block = false;
    let mut seen_block = false;
    for line in existing.lines() {
        if line == BEGIN {
            if in_block || seen_block {
                return Err(LocalWebError::Config(
                    "multiple LocalWeb hosts blocks".into(),
                ));
            }
            in_block = true;
            seen_block = true;
            continue;
        }
        if line == END {
            if !in_block {
                return Err(LocalWebError::Config(
                    "unmatched LocalWeb hosts block end".into(),
                ));
            }
            in_block = false;
            continue;
        }
        if !in_block {
            output.push_str(line);
            output.push('\n');
        }
    }
    if in_block {
        return Err(LocalWebError::Config(
            "unterminated LocalWeb hosts block".into(),
        ));
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_other_lines_and_replaces_only_managed_block() {
        let original = "127.0.0.1 localhost\n# custom\n1.2.3.4 public.dev\n# BEGIN LOCALWEB - DO NOT EDIT\n127.0.0.1 old.dev\n# END LOCALWEB\n::1 localhost\n";
        let result = transform_hosts(original, &["new.dev".into(), "second.test".into()]).unwrap();
        assert!(result.contains("1.2.3.4 public.dev\n"));
        assert!(result.contains("::1 localhost\n"));
        assert!(!result.contains("old.dev"));
        assert!(result.contains("127.0.0.1 new.dev\n"));
        assert_eq!(
            transform_hosts(&result, &["new.dev".into(), "second.test".into()]).unwrap(),
            result
        );
    }
    #[test]
    fn rejects_injection_and_malformed_blocks() {
        assert!(transform_hosts("", &["a.dev\n0.0.0.0 evil.dev".into()]).is_err());
        assert!(transform_hosts(BEGIN, &[]).is_err());
        assert!(transform_hosts(END, &[]).is_err());
    }

    #[test]
    fn removes_only_the_managed_block() {
        let original = "127.0.0.1 localhost\n# BEGIN LOCALWEB - DO NOT EDIT\n127.0.0.1 demo.dev\n# END LOCALWEB\n::1 localhost\n";
        assert_eq!(
            remove_managed_block(original).unwrap(),
            "127.0.0.1 localhost\n::1 localhost\n"
        );
    }
}
