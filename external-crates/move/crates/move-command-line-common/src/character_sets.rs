// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::fmt::{self, Display};

/// Determine if a character is an allowed eye-visible (printable) character.
///
/// The only allowed printable characters are the printable ascii characters (SPACE through ~) and
/// tabs. All other characters are invalid and we return false.
pub fn is_permitted_printable_char(c: char) -> bool {
    let x = c as u32;
    let is_above_space = x >= 0x20; // Don't allow meta characters
    let is_below_tilde = x <= 0x7E; // Don't allow DEL meta character
    let is_tab = x == 0x09; // Allow tabs
    (is_above_space && is_below_tilde) || is_tab
}

/// Determine if a character is a permitted newline lf character.
///
/// The only permitted newline lf character is \n All others are invalid.
pub fn is_permitted_newline_lf_char(c: char) -> bool {
    let x = c as u32;
    x == 0x0A // \n
}

/// Determine if a character is a permitted newline crlf character.
///
/// The only permitted newline character is \r\n. All others are invalid.
pub fn is_permitted_newline_crlf_chars(c1: char, c2: char) -> bool {
    let x1 = c1 as u32;
    let x2 = c2 as u32;
    let is_cr = x1 == 0x0D; // \r
    let is_lf = x2 == 0x0A; // \n
    is_cr && is_lf
}

// Taken from https://en.wikipedia.org/wiki/Bidirectional_text
// TODO Double check 200F 200E 061C
const UNICODE_TEXT_DIRECTIONALITY_CONTROL_CHARS: &[char] = &[
    '\u{202A}', '\u{202B}', '\u{202C}', '\u{202D}', '\u{202E}', '\u{2066}', '\u{2067}', '\u{2068}',
    '\u{2069}',
];

/// Is some other non-whitespace unicode character
pub fn is_permitted_non_ascii_unicode(c: char) -> bool {
    !c.is_ascii() && !c.is_whitespace() && !UNICODE_TEXT_DIRECTIONALITY_CONTROL_CHARS.contains(&c)
}

/// Determine if a character is permitted character.
///
/// A permitted character is either a permitted printable character, or a permitted
/// newline. Any other characters are disallowed from appearing in the file.
pub fn is_permitted_char(c: char) -> bool {
    is_permitted_printable_char(c)
        || is_permitted_newline_lf_char(c)
        || is_permitted_non_ascii_unicode(c)
}

/// Determine if the characters is permitted characters.
///
/// A permitted characters is either a permitted printable character, or a permitted
/// newlines. Any other characters are disallowed from appearing in the file.
pub fn is_permitted_chars(chars: &[char], idx: usize) -> bool {
    let c1 = chars[idx];
    if is_permitted_char(c1) {
        return true;
    }

    if idx + 1 >= chars.len() {
        return false;
    }

    let c2 = chars[idx + 1];
    is_permitted_newline_crlf_chars(c1, c2)
}

/// simple wrapper for displaying characters. Escapes non-printable characters for error messages
pub struct DisplayChar(pub char);

impl Display for DisplayChar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 == ' ' || self.0.is_alphanumeric() || self.0.is_ascii_punctuation() {
            self.0.fmt(f)
        } else {
            self.0.escape_default().fmt(f)
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_permitted_characters() {
        let mut good_chars = (0x20..=0x7Eu8).map(|u| u as char).collect::<Vec<char>>();
        good_chars.push('\r');
        good_chars.push('\n');
        good_chars.push('\t');

        for idx in 0..good_chars.len() {
            assert!(super::is_permitted_chars(&good_chars, idx));
        }
    }

    #[test]
    fn test_forbidden_last_lf_characters() {
        let mut good_chars = (0x20..=0x7Eu8).map(|u| u as char).collect::<Vec<char>>();
        good_chars.push('\r');

        assert!(!super::is_permitted_chars(
            &good_chars,
            good_chars.len() - 1
        ));
    }

    #[test]
    fn test_forbidden_characters() {
        let mut bad_chars = (0x0..0x09u8).map(|u| u as char).collect::<Vec<char>>();
        bad_chars.extend((0x0B..=0x1Fu8).map(|u| u as char));
        bad_chars.push(0x7Fu8 as char);

        for idx in 0..bad_chars.len() {
            assert!(
                !super::is_permitted_chars(&bad_chars, idx),
                "{:X?} is permitted",
                bad_chars[idx] as u8
            );
        }
    }
}
