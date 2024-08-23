// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use suioplib::cli::lib::utils::validate_project_name;

    #[test]
    fn test_validate_project_name_valid() {
        let names = vec!["a", "abc", "a1-bc2", "z12345678901234567890123456789"];
        for name in names {
            assert!(validate_project_name(name).is_ok());
        }
    }

    #[test]
    fn test_validate_project_name_invalid_start() {
        let names = vec!["1abc", "-abc", "_abc"];
        for name in names {
            assert!(validate_project_name(name).is_err());
        }
    }

    #[test]
    fn test_validate_project_name_invalid_chars() {
        let names = vec!["ab_c", "ab?c", "ab*c"];
        for name in names {
            assert!(validate_project_name(name).is_err());
        }
    }

    #[test]
    fn test_validate_project_name_too_long() {
        let name = "a123456789012345678901234567890";
        assert!(validate_project_name(name).is_err());
    }
}
