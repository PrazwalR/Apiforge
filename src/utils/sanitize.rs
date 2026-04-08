use once_cell::sync::Lazy;
use regex::Regex;

/// Patterns that may leak sensitive info in AWS error messages
static AWS_SENSITIVE_PATTERNS: Lazy<Vec<(Regex, &str)>> = Lazy::new(|| {
    vec![
        // AWS account IDs (12 digits)
        (Regex::new(r"\b\d{12}\b").unwrap(), "***ACCOUNT***"),
        // AWS Access Key IDs (AKIA...)
        (
            Regex::new(r"\bAKIA[A-Z0-9]{16}\b").unwrap(),
            "***ACCESS_KEY***",
        ),
        // AWS Secret Keys (appear in some error contexts)
        (
            Regex::new(r"(?i)secret[_-]?key\s*[:=]\s*[A-Za-z0-9/+=]{40}").unwrap(),
            "secret_key=***REDACTED***",
        ),
        // ARNs (may contain account info)
        (
            Regex::new(r"arn:aws:[a-z0-9-]+:[a-z0-9-]*:\d{12}:[^\s]+").unwrap(),
            "arn:aws:***:***:***ACCOUNT***:***",
        ),
        // Request IDs (not sensitive but reduce noise)
        (
            Regex::new(r"(?i)request[_-]?id\s*[:=]\s*[a-f0-9-]{36}").unwrap(),
            "request_id=***",
        ),
    ]
});

/// Patterns for token redaction in URLs and logs
static URL_TOKEN_PATTERNS: Lazy<Vec<(Regex, &str)>> = Lazy::new(|| {
    vec![
        // GitHub tokens (ghp_, gho_, ghu_, ghs_, ghr_)
        (
            Regex::new(r"gh[pousr]_[A-Za-z0-9_]{36,255}").unwrap(),
            "***GITHUB_TOKEN***",
        ),
        // Generic tokens in URLs
        (
            Regex::new(r"(?i)[?&]token=[^&\s]+").unwrap(),
            "?token=***REDACTED***",
        ),
        (
            Regex::new(r"(?i)[?&]access_token=[^&\s]+").unwrap(),
            "?access_token=***REDACTED***",
        ),
        (
            Regex::new(r"(?i)[?&]api_key=[^&\s]+").unwrap(),
            "?api_key=***REDACTED***",
        ),
        // Authorization headers
        (
            Regex::new(r"(?i)authorization:\s*bearer\s+[^\s]+").unwrap(),
            "Authorization: Bearer ***REDACTED***",
        ),
        (
            Regex::new(r"(?i)authorization:\s*token\s+[^\s]+").unwrap(),
            "Authorization: token ***REDACTED***",
        ),
        // Basic auth in URLs (user:pass@host)
        (Regex::new(r"://[^:/@]+:[^@/]+@").unwrap(), "://***:***@"),
    ]
});

/// Sanitize AWS error messages to remove potentially sensitive information.
/// Returns a cleaned message safe for logging.
pub fn sanitize_aws_error(message: &str) -> String {
    let mut result = message.to_string();
    for (pattern, replacement) in AWS_SENSITIVE_PATTERNS.iter() {
        result = pattern.replace_all(&result, *replacement).to_string();
    }
    result
}

/// Redact tokens and credentials from URLs and log messages.
/// Returns a cleaned string safe for logging.
pub fn redact_tokens(message: &str) -> String {
    let mut result = message.to_string();
    for (pattern, replacement) in URL_TOKEN_PATTERNS.iter() {
        result = pattern.replace_all(&result, *replacement).to_string();
    }
    result
}

/// Sanitize any message - combines AWS and token redaction
pub fn sanitize_message(message: &str) -> String {
    let result = sanitize_aws_error(message);
    redact_tokens(&result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_aws_account_id() {
        let input = "Error in account 123456789012";
        let result = sanitize_aws_error(input);
        assert!(!result.contains("123456789012"));
        assert!(result.contains("***ACCOUNT***"));
    }

    #[test]
    fn test_sanitize_aws_access_key() {
        let input = "Invalid key AKIAIOSFODNN7EXAMPLE";
        let result = sanitize_aws_error(input);
        assert!(!result.contains("AKIAIOSFODNN7EXAMPLE"));
        assert!(result.contains("***ACCESS_KEY***"));
    }

    #[test]
    fn test_redact_github_token() {
        let input = "Using token ghp_abc123def456ghi789jkl012mno345pqr678stu";
        let result = redact_tokens(input);
        assert!(!result.contains("ghp_"));
        assert!(result.contains("***GITHUB_TOKEN***"));
    }

    #[test]
    fn test_redact_url_token() {
        let input = "Calling https://api.example.com?token=secret123&other=value";
        let result = redact_tokens(input);
        assert!(!result.contains("secret123"));
        assert!(result.contains("***REDACTED***"));
    }

    #[test]
    fn test_redact_basic_auth() {
        let input = "Connecting to https://user:password@registry.example.com/repo";
        let result = redact_tokens(input);
        assert!(!result.contains("password"));
        assert!(result.contains("***:***@"));
    }
}
