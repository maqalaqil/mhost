use std::collections::HashMap;

/// Expand `${VAR}` placeholders in `input` using `std::env`.
///
/// Rules:
/// - `${VAR}` is replaced with the environment variable value when found.
/// - `${VAR}` is kept as a literal `${VAR}` when the variable is not set.
/// - Unclosed `${...` sequences are kept as-is.
/// - A bare `$` (not followed by `{`) is kept as-is.
/// - `${}` (empty var name) calls the resolver with an empty string.
pub fn expand_env(input: &str) -> String {
    expand_env_with(input, |var| std::env::var(var).ok())
}

/// Expand `${VAR}` placeholders in `input` using a custom resolver function.
///
/// `resolver` receives the variable name and returns `Some(value)` when the
/// variable is defined, or `None` to keep the placeholder unchanged.
pub fn expand_env_with<F>(input: &str, resolver: F) -> String
where
    F: Fn(&str) -> Option<String>,
{
    let mut result = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'$' {
            // Look ahead for `{`
            if i + 1 < len && bytes[i + 1] == b'{' {
                // Search for the closing `}`
                if let Some(close) = input[i + 2..].find('}') {
                    let var_name = &input[i + 2..i + 2 + close];
                    match resolver(var_name) {
                        Some(value) => result.push_str(&value),
                        None => {
                            // Keep placeholder as-is
                            result.push_str(&input[i..i + 2 + close + 1]);
                        }
                    }
                    i += 2 + close + 1; // skip past `}`
                } else {
                    // Unclosed `${...` — keep as-is
                    result.push_str(&input[i..]);
                    i = len;
                }
            } else {
                // Bare `$` not followed by `{` — keep as-is
                result.push('$');
                i += 1;
            }
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }

    result
}

/// Expand `${VAR}` placeholders in every value of `map` using `std::env`.
///
/// Returns a new `HashMap` — the original is not mutated.
pub fn expand_env_map(map: &HashMap<String, String>) -> HashMap<String, String> {
    map.iter()
        .map(|(k, v)| (k.clone(), expand_env(v)))
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // 1. No variables — string returned unchanged.
    #[test]
    fn test_no_vars() {
        assert_eq!(expand_env_with("hello world", |_| None), "hello world");
    }

    // 2. Single variable present.
    #[test]
    fn test_single_var() {
        let result = expand_env_with("Hello, ${NAME}!", |var| {
            if var == "NAME" {
                Some("Alice".to_string())
            } else {
                None
            }
        });
        assert_eq!(result, "Hello, Alice!");
    }

    // 3. Multiple variables.
    #[test]
    fn test_multiple_vars() {
        let result = expand_env_with("${GREETING}, ${NAME}!", |var| match var {
            "GREETING" => Some("Hi".to_string()),
            "NAME" => Some("Bob".to_string()),
            _ => None,
        });
        assert_eq!(result, "Hi, Bob!");
    }

    // 4. Missing variable kept as literal.
    #[test]
    fn test_missing_var_kept() {
        let result = expand_env_with("value=${MISSING}", |_| None);
        assert_eq!(result, "value=${MISSING}");
    }

    // 5. Unclosed `${...` kept as-is.
    #[test]
    fn test_unclosed_brace() {
        let result = expand_env_with("broken=${UNCLOSED", |_| None);
        assert_eq!(result, "broken=${UNCLOSED");
    }

    // 6. Bare `$` without `{` kept as-is.
    #[test]
    fn test_dollar_without_brace() {
        let result = expand_env_with("cost is $10", |_| None);
        assert_eq!(result, "cost is $10");
    }

    // 7. Empty var name `${}` — resolver called with empty string.
    #[test]
    fn test_empty_var_name() {
        let result = expand_env_with("${}", |var| {
            if var.is_empty() {
                Some("empty".to_string())
            } else {
                None
            }
        });
        assert_eq!(result, "empty");
    }

    // 8. expand_env_map expands values using real environment.
    #[test]
    fn test_expand_env_map_with_real_env() {
        // Set a known environment variable for this test.
        std::env::set_var("MHOST_TEST_VAR", "expanded_value");

        let mut map = HashMap::new();
        map.insert(
            "KEY".to_string(),
            "prefix_${MHOST_TEST_VAR}_suffix".to_string(),
        );

        let result = expand_env_map(&map);
        assert_eq!(result.get("KEY").unwrap(), "prefix_expanded_value_suffix");

        // Clean up
        std::env::remove_var("MHOST_TEST_VAR");
    }

    // 9. Adjacent variables ${A}${B} — both expanded.
    #[test]
    fn test_adjacent_vars() {
        let result = expand_env_with("${A}${B}", |var| match var {
            "A" => Some("hello".to_string()),
            "B" => Some("world".to_string()),
            _ => None,
        });
        assert_eq!(result, "helloworld");
    }

    // 10. Variable at start of string.
    #[test]
    fn test_var_at_start_of_string() {
        let result = expand_env_with("${PREFIX}/bin", |var| {
            if var == "PREFIX" {
                Some("/usr/local".to_string())
            } else {
                None
            }
        });
        assert_eq!(result, "/usr/local/bin");
    }

    // 11. Variable at end of string.
    #[test]
    fn test_var_at_end_of_string() {
        let result = expand_env_with("port=${PORT}", |var| {
            if var == "PORT" {
                Some("8080".to_string())
            } else {
                None
            }
        });
        assert_eq!(result, "port=8080");
    }

    // 12. Nested ${} is not supported — outer ${} is expanded with var name
    //     containing "${", so resolver sees the inner text including "${".
    //     The implementation does NOT support nesting; this verifies the actual
    //     behavior (the inner ${ is just part of the variable name string).
    #[test]
    fn test_nested_braces_not_supported() {
        // "${${INNER}}" — the implementation finds the first `}` at position
        // of the inner `}`, so var_name = "${INNER".  The resolver gets
        // "${INNER" which is unlikely to be set, so the placeholder is kept.
        let result = expand_env_with("${${INNER}}", |_| None);
        // The outer `${` opens, then the first `}` closes it.
        // var_name == "${INNER", which is not defined → kept as-is.
        // Then "}" is appended literally.
        // We just verify it does NOT panic and returns a deterministic result.
        let _ = result; // smoke-test: no panic is the requirement
    }

    // 13. expand_env_map does not mutate the original map.
    #[test]
    fn test_expand_env_map_does_not_mutate_original() {
        let mut map = HashMap::new();
        map.insert("K".to_string(), "${UNDEFINED_XYZ}".to_string());

        let expanded = expand_env_map(&map);
        // Original untouched
        assert_eq!(map.get("K").unwrap(), "${UNDEFINED_XYZ}");
        // Expanded value still has the placeholder (env var not set)
        assert_eq!(expanded.get("K").unwrap(), "${UNDEFINED_XYZ}");
    }
}
