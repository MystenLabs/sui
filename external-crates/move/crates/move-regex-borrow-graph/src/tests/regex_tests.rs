// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

type Regex = crate::regex::Regex<char>;
type Extension = crate::regex::Extension<char>;

#[test]
pub fn test_regex_remove_prefix() {
    let cases: &[(&str, &str, &[&str])] = &[
        (("", ""), &[""]),
        (("a", "a"), &[""]),
        (("a", ""), &["a"]),
        ((".*", ""), &[".*"]),
        (("a.*", ""), &["a.*"]),
        (("ab", ""), &["ab"]),
        (("ab", "a"), &["b"]),
        (("abc", "a"), &["bc"]),
        ((".*", "a"), &[".*"]),
        (("a.*", "a"), &[".*"]),
        (("ab.*", "a"), &["b.*"]),
        (("", ".*"), &[""]),
        (("a", ".*"), &["a", ""]),
        (("ab", ".*"), &["ab", "b", ""]),
        (("abc", ".*"), &["abc", "bc", "c", ""]),
        ((".*", ".*"), &[".*"]),
        (("a.*", ".*"), &[".*"]),
        (("ab.*", ".*"), &[".*"]),
        (("", "a"), &[]),
        (("a", "b"), &[]),
        (("ab", "b"), &[]),
        (("abc", "b"), &[]),
        (("a.*", "b"), &[]),
    ];
    for ((r, ext), expected) in cases {
        let result = regex_remove_prefix(r, ext);
        assert_eq!(
            result, &**expected,
            "Failed for case: {:?}.remove_prefix({:?})",
            r, ext
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
        .map(|regex| to_string(regex))
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

fn peel_ext(s: &str) -> (&str, Extension) {
    match (s, s.chars().next()) {
        ("", _) => (s, Extension::Epsilon),
        (".*", _) => (&s[2..], Extension::DotStar),
        (_, Some(c @ 'a'..='z')) => (&s[1..], Extension::Label(c)),
        _ => panic!("Invalid Extension: {}", s),
    }
}
