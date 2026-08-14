//! Defense-in-depth redaction for allowlisted flag values.
//!
//! The primary privacy mechanism is structural: values are captured only for
//! flags a human explicitly allowlisted. This module is the second layer —
//! even an allowlisted value is checked against secret shapes before it can
//! enter an event, because allowlists are written by people and people pass
//! `--token` where `--format` was intended.
//!
//! A match replaces the whole value with `[REDACTED:<pattern>]` and keeps
//! the event, so aggregate counts stay correct. False positives are
//! acceptable; false negatives are the failure mode this module exists to
//! prevent. The rule set is deliberately a handful of readable checks in one
//! file rather than a pattern-plugin system: this code is the audited
//! artifact.

use std::borrow::Cow;

/// Secret shapes the redaction pass recognizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretPattern {
    /// AWS access key id: `AKIA` followed by 16 uppercase alphanumerics.
    AwsAccessKey,
    /// `sk-`/`sk_` prefixed API keys (OpenAI, Stripe, and imitators).
    SkPrefixedKey,
    /// GitHub tokens: `ghp_`, `gho_`, `ghu_`, `ghs_`, `ghr_`, `github_pat_`.
    GithubToken,
    /// JSON Web Token: three base64url segments, first decoding to `{"`.
    Jwt,
    /// Email address (structural check, not RFC 5322).
    Email,
    /// Long string with high Shannon entropy — the generic-secret backstop.
    HighEntropy,
}

impl SecretPattern {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            SecretPattern::AwsAccessKey => "aws-access-key",
            SecretPattern::SkPrefixedKey => "sk-key",
            SecretPattern::GithubToken => "github-token",
            SecretPattern::Jwt => "jwt",
            SecretPattern::Email => "email",
            SecretPattern::HighEntropy => "high-entropy",
        }
    }
}

/// Minimum length before the entropy backstop applies. Shorter strings
/// cannot carry enough randomness to look like a secret.
const ENTROPY_MIN_LEN: usize = 16;

/// Bits per character above which a string is treated as a secret.
/// Random alphanumerics of qualifying length measure ~3.8-4.0; English
/// words and file paths measure well below. Aligned with the thresholds
/// used by detect-secrets-style scanners.
const ENTROPY_THRESHOLD: f64 = 3.7;

/// Redact `value` if it matches a secret shape; pass it through untouched
/// otherwise.
#[must_use]
pub fn redact(value: &str) -> Cow<'_, str> {
    match detect(value) {
        Some(pattern) => Cow::Owned(format!("[REDACTED:{}]", pattern.label())),
        None => Cow::Borrowed(value),
    }
}

/// Classify `value`, checking specific shapes before the entropy backstop.
#[must_use]
pub fn detect(value: &str) -> Option<SecretPattern> {
    if is_aws_access_key(value) {
        Some(SecretPattern::AwsAccessKey)
    } else if is_sk_prefixed_key(value) {
        Some(SecretPattern::SkPrefixedKey)
    } else if is_github_token(value) {
        Some(SecretPattern::GithubToken)
    } else if is_jwt(value) {
        Some(SecretPattern::Jwt)
    } else if is_email(value) {
        Some(SecretPattern::Email)
    } else if is_high_entropy(value) {
        Some(SecretPattern::HighEntropy)
    } else {
        None
    }
}

fn is_aws_access_key(value: &str) -> bool {
    value.len() == 20
        && value.starts_with("AKIA")
        && value[4..]
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

fn is_sk_prefixed_key(value: &str) -> bool {
    (value.starts_with("sk-") || value.starts_with("sk_"))
        && value.len() >= 20
        && !value.contains(char::is_whitespace)
}

fn is_github_token(value: &str) -> bool {
    const PREFIXES: [&str; 6] = ["ghp_", "gho_", "ghu_", "ghs_", "ghr_", "github_pat_"];
    PREFIXES.iter().any(|prefix| {
        value.strip_prefix(prefix).is_some_and(|rest| {
            rest.len() >= 16 && rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
    })
}

fn is_jwt(value: &str) -> bool {
    // Structural: header.payload.signature, all base64url. A JWT header is
    // base64("{"...), which always starts with "eyJ".
    let mut segments = value.split('.');
    let (Some(header), Some(payload), Some(signature), None) = (
        segments.next(),
        segments.next(),
        segments.next(),
        segments.next(),
    ) else {
        return false;
    };
    header.starts_with("eyJ")
        && [header, payload, signature].into_iter().all(|segment| {
            segment.len() >= 8
                && segment
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '=')
        })
}

fn is_email(value: &str) -> bool {
    if value.contains(char::is_whitespace) {
        return false;
    }
    let Some((local, domain)) = value.split_once('@') else {
        return false;
    };
    if local.is_empty() || domain.contains('@') {
        return false;
    }
    domain.split('.').count() >= 2 && domain.split('.').all(|label| !label.is_empty())
}

fn is_high_entropy(value: &str) -> bool {
    if value.len() < ENTROPY_MIN_LEN || value.contains(char::is_whitespace) {
        return false;
    }
    // Real secrets virtually always mix character classes; requiring it
    // spares long plain words and lowercase paths from false positives.
    let has_digit = value.chars().any(|c| c.is_ascii_digit());
    let mixed_case = value.chars().any(char::is_uppercase) && value.chars().any(char::is_lowercase);
    if !(has_digit || mixed_case) {
        return false;
    }
    shannon_entropy(value) > ENTROPY_THRESHOLD
}

/// Character-level Shannon entropy in bits per character.
fn shannon_entropy(value: &str) -> f64 {
    let mut counts: std::collections::HashMap<char, u32> = std::collections::HashMap::new();
    let mut total = 0u32;
    for c in value.chars() {
        *counts.entry(c).or_insert(0) += 1;
        total += 1;
    }
    let total = f64::from(total);
    counts
        .values()
        .map(|&count| {
            let p = f64::from(count) / total;
            -p * p.log2()
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_each_pattern() {
        let cases: &[(&str, SecretPattern)] = &[
            ("AKIAIOSFODNN7EXAMPLE", SecretPattern::AwsAccessKey),
            (
                "sk-proj-FAKE00000000000000000000",
                SecretPattern::SkPrefixedKey,
            ),
            ("sk_live_FAKE0000000000000000", SecretPattern::SkPrefixedKey),
            ("ghp_FAKE000000000000FAKE", SecretPattern::GithubToken),
            (
                "github_pat_11ABCDEFG0abcdefghijklmnop",
                SecretPattern::GithubToken,
            ),
            (
                "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dQw4w9WgXcQdQw4w9WgXcQ",
                SecretPattern::Jwt,
            ),
            ("dev@example.com", SecretPattern::Email),
            ("first.last@sub.domain.co", SecretPattern::Email),
            ("x9Kf2mQp7Rt4Vw1zB6nH3jL8", SecretPattern::HighEntropy),
        ];
        for (input, expected) in cases {
            assert_eq!(detect(input), Some(*expected), "input: {input}");
        }
    }

    #[test]
    fn near_misses_pass_through() {
        let cases: &[&str] = &[
            // AWS: too short for any secret shape
            "AKIAIOSFODNN7",
            // sk-: too short, or whitespace
            "sk-short",
            // github: prefix but short tail
            "ghp_short",
            // JWT-ish: wrong segment count, or no eyJ header
            "one.two.three",
            "abc.def.ghi.jkl",
            // email-ish
            "user@",
            "@example.com",
            "not an@email.com",
            "user@nodot",
            // ordinary values that must never be redacted
            "json",
            "table",
            "supercalifragilisticexpialidocious",
            "/usr/local/share/applications",
            "my-project-name-2",
            "2026-08-14T12:00:00Z",
        ];
        for input in cases {
            assert_eq!(detect(input), None, "input: {input}");
        }
    }

    #[test]
    fn key_like_near_misses_still_fail_closed() {
        // Not valid AWS key shapes (lowercase tail; prefix inside a word),
        // but key-like enough that the entropy backstop catches them — the
        // desired fail-closed behavior for anything that looks secret-ish.
        for input in [
            "AKIAiosfodnn7example",
            "xAKIAIOSFODNN7EXAMPLE",
            // Not JWT-shaped (no eyJ header) but carries a base64 segment.
            "notajwt.eyJzdWIiOiIx.signature",
        ] {
            assert_eq!(
                detect(input),
                Some(SecretPattern::HighEntropy),
                "input: {input}"
            );
        }
    }

    #[test]
    fn redact_replaces_whole_value_with_label() {
        assert_eq!(redact("AKIAIOSFODNN7EXAMPLE"), "[REDACTED:aws-access-key]");
        assert_eq!(redact("dev@example.com"), "[REDACTED:email]");
        assert_eq!(redact("json"), "json");
    }

    #[test]
    fn redacted_output_never_equals_a_detected_input() {
        let secrets = [
            "AKIAIOSFODNN7EXAMPLE",
            "ghp_FAKE000000000000FAKE",
            "dev@example.com",
        ];
        for secret in secrets {
            let output = redact(secret);
            assert_ne!(output.as_ref(), secret);
            assert!(output.starts_with("[REDACTED:"));
        }
    }

    #[test]
    fn entropy_boundaries() {
        // 15 chars of randomness: below the length floor, kept.
        assert_eq!(detect("x9Kf2mQp7Rt4Vw1"), None);
        // Long but low-entropy repetition: kept.
        assert_eq!(detect("aaaaaaaaaaaaaaaa1"), None);
        // Long, digits present, high entropy: redacted.
        assert_eq!(
            detect("f3Kx9mQ2pR7tV4wZ1bN6hJ8L"),
            Some(SecretPattern::HighEntropy)
        );
    }

    #[test]
    fn entropy_of_uniform_string_is_zero() {
        assert!(shannon_entropy("aaaa").abs() < f64::EPSILON);
    }
}
