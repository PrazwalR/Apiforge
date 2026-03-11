use crate::error::{ApiForgError, Result};
use std::env;

pub fn resolve_env_vars(input: &str) -> Result<String> {
    let mut result = input.to_string();
    let re = regex::Regex::new(r"\$\{([A-Za-z0-9_]+)\}").unwrap();

    for cap in re.captures_iter(input) {
        let var_name = &cap[1];
        let value = env::var(var_name)
            .map_err(|_| ApiForgError::EnvVarMissing(var_name.to_string()))?;

        result = result.replace(&format!("${{{}}}", var_name), &value);
    }

    Ok(result)
}

pub fn resolve_config_env_vars<T: serde::de::DeserializeOwned + serde::Serialize>(
    config: &T,
) -> Result<T> {
    let json_str = serde_json::to_string(config).map_err(|e| {
        ApiForgError::Serialization(format!("Failed to serialize config: {}", e))
    })?;

    let resolved = resolve_env_vars(&json_str)?;

    serde_json::from_str(&resolved).map_err(|e| {
        ApiForgError::Serialization(format!("Failed to deserialize config: {}", e))
    })
}

pub fn check_missing_env_vars(input: &str) -> Vec<String> {
    let re = regex::Regex::new(r"\$\{([A-Za-z0-9_]+)\}").unwrap();
    let mut missing = Vec::new();

    for cap in re.captures_iter(input) {
        let var_name = &cap[1];
        if env::var(var_name).is_err() {
            missing.push(var_name.to_string());
        }
    }

    missing
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_env_vars() {
        env::set_var("TEST_VAR", "test_value");
        let result = resolve_env_vars("Hello ${TEST_VAR}!").unwrap();
        assert_eq!(result, "Hello test_value!");
    }

    #[test]
    fn test_missing_env_var() {
        env::remove_var("NONEXISTENT_VAR");
        let result = resolve_env_vars("Hello ${NONEXISTENT_VAR}!");
        assert!(result.is_err());
    }

    #[test]
    fn test_check_missing_env_vars() {
        env::remove_var("MISSING1");
        env::remove_var("MISSING2");
        let missing = check_missing_env_vars("${MISSING1} and ${MISSING2}");
        assert_eq!(missing, vec!["MISSING1", "MISSING2"]);
    }
}
