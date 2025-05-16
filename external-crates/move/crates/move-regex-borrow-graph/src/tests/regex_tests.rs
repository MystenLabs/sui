// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

type Regex = crate::regex::Regex<char>;
type Extension = crate::regex::Extension<char>;

/// See `Regex::remove_prefix` for details
const REGEX_REM_PREFIX_CASES: &[(&str, &str, &[&str])] = &[
    ("", "", &[""]),
    ("a", "a", &[""]),
    ("a", "", &["a"]),
    (".*", "", &[".*"]),
    ("a.*", "", &["a.*"]),
    ("ab", "", &["ab"]),
    ("ab", "a", &["b"]),
    ("abc", "a", &["bc"]),
    (".*", "a", &[".*"]),
    ("a.*", "a", &[".*"]),
    ("ab.*", "a", &["b.*"]),
    ("", ".*", &[""]),
    ("a", ".*", &["a", ""]),
    ("ab", ".*", &["ab", "b", ""]),
    ("abc", ".*", &["abc", "bc", "c", ""]),
    (".*", ".*", &[".*"]),
    ("a.*", ".*", &[".*"]),
    ("ab.*", ".*", &[".*"]),
    ("", "a", &[]),
    ("a", "b", &[]),
    ("ab", "b", &[]),
    ("abc", "b", &[]),
    ("a.*", "b", &[]),
];

#[test]
fn test_regex_remove_prefix() {
    for &(r, ext, expected) in REGEX_REM_PREFIX_CASES {
        let result = regex_remove_prefix(r, ext);
        assert_eq!(
            result, expected,
            "Failed for case: {:?}.remove_prefix({:?})",
            r, ext
        );
    }
}

/// See `Extension::remove_prefix` for details
const EXT_REM_PREFIX_CASES: &[(&str, &str, &[&str])] = &[
    ("", "", &[""]),
    ("", ".*", &[""]),
    ("a", "", &["a"]),
    ("a", "a", &[""]),
    ("a", ".*", &["a", ""]),
    ("a", "a.*", &[""]),
    (".*", "", &[".*"]),
    (".*", "a", &[".*"]),
    (".*", "ab", &[".*"]),
    (".*", ".*", &[".*"]),
    (".*", "a.*", &[".*"]),
    ("", "a", &[]),
    ("", "ab", &[]),
    ("", "a.*", &[]),
    ("a", "b", &[]),
    ("a", "bc.*", &[]),
    ("a", "ab", &[]),
];

#[test]
fn test_extension_remove_prefix() {
    for &(ext, r, expected) in EXT_REM_PREFIX_CASES {
        let result = extension_remove_prefix(ext, r);
        assert_eq!(
            result, expected,
            "Failed for case: {:?}.remove_prefix({:?})",
            ext, r
        );
    }
}

// Tests that the regex and extension remove_prefix functions are equivalent
#[test]
fn test_remove_prefix_equivalence() {
    for &(r, ext, _expected) in REGEX_REM_PREFIX_CASES {
        if !is_extension(r) {
            continue;
        }
        let regex_result = regex_remove_prefix(r, ext);
        let extension_result = extension_remove_prefix(r, ext);
        assert_eq!(
            regex_result, extension_result,
            "Failed for case: {:?}.remove_prefix({:?})",
            r, ext
        );
    }
    for &(ext, r, _expected) in EXT_REM_PREFIX_CASES {
        if !is_extension(r) {
            continue;
        }
        let regex_result = regex_remove_prefix(ext, r);
        let extension_result = extension_remove_prefix(ext, r);
        assert_eq!(
            regex_result, extension_result,
            "Failed for case: {:?}.remove_prefix({:?})",
            ext, r
        );
    }
}

//**************************************************************************************************
// Helpers
//**************************************************************************************************

fn regex_remove_prefix(r: &str, ext: &str) -> Vec<String> {
    let (rem, ext) = peel_ext(ext);
    assert_eq!(rem, "");
    from_str(r)
        .remove_prefix(&ext)
        .into_iter()
        .map(to_string)
        .collect()
}

fn extension_remove_prefix(ext: &str, r: &str) -> Vec<String> {
    let (rem, ext) = peel_ext(ext);
    assert_eq!(rem, "");
    ext.remove_prefix(&from_str(r))
        .into_iter()
        .map(to_string)
        .collect()
}

fn from_str(mut s: &str) -> Regex {
    let mut regex = Regex::epsilon();
    loop {
        let (s_, ext) = peel_ext(s);
        s = s_;
        match &ext {
            Extension::Epsilon => break regex,
            _ => regex = regex.extend(&ext),
        }
    }
}

fn to_string(regex: Regex) -> String {
    let (chars, ends_in_dot_star) = regex.query_api_path();
    let mut s = String::new();
    s.extend(chars);
    if ends_in_dot_star {
        s.push_str(".*");
    }
    s
}

fn is_extension(s: &str) -> bool {
    match peel_ext_opt(s) {
        Some((rem, _)) => rem.is_empty(),
        None => false,
    }
}

fn peel_ext(s: &str) -> (&str, Extension) {
    match peel_ext_opt(s) {
        Some((s, ext)) => (s, ext),
        None => panic!("Invalid Extension: {}", s),
    }
}

fn peel_ext_opt(s: &str) -> Option<(&str, Extension)> {
    Some(match (s, s.chars().next()) {
        ("", _) => (s, Extension::Epsilon),
        (".*", _) => (&s[2..], Extension::DotStar),
        (_, Some(c @ 'a'..='z')) => (&s[1..], Extension::Label(c)),
        _ => return None,
    })
}
