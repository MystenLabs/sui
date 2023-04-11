// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::{bail, Result};
use codespan_reporting::{
    diagnostic::{Diagnostic, Label},
    files::SimpleFiles,
    term::{
        emit,
        termcolor::{ColorChoice, StandardStream},
        Config,
    },
};
use move_command_line_common::character_sets::is_permitted_chars;
use move_ir_to_bytecode_syntax::syntax::{self, ParseError};
use move_ir_types::{ast, location::*};

// We restrict strings to only ascii visual characters (0x20 <= c <= 0x7E) or a permitted newline
// character--\r--,--\n--or a tab--\t. Checking each character in the input string is more fool-proof
// than checking each character later during lexing & tokenization, since that would require special
// handling of characters inside of comments (usually not included as tokens) and within byte
// array literals.
fn verify_string(string: &str) -> Result<()> {
    string
        .chars()
        .enumerate()
        .find(|(idx, _)| !is_permitted_chars(string.as_bytes(), *idx))
        .map_or(Ok(()), |(_, c)| {
            bail!(
                "Parser Error: invalid character {} found when reading file.\
                 Only ascii printable, tabs (\\t), lf (\\n) and crlf (\\r\\n) characters are permitted.",
                c
            )
        })
}

/// Given the raw input of a file, creates a `ScriptOrModule` enum
/// Fails with `Err(_)` if the text cannot be parsed`
pub fn parse_script_or_module(s: &str) -> Result<ast::ScriptOrModule> {
    verify_string(s)?;
    syntax::parse_script_or_module_string(s).or_else(|e| handle_error(e, s))
}

/// Given the raw input of a file, creates a `Script` struct
/// Fails with `Err(_)` if the text cannot be parsed
pub fn parse_script(script_str: &str) -> Result<ast::Script> {
    verify_string(script_str)?;
    syntax::parse_script_string(script_str).or_else(|e| handle_error(e, script_str))
}

/// Given the raw input of a file, creates a single `ModuleDefinition` struct
/// Fails with `Err(_)` if the text cannot be parsed
pub fn parse_module(modules_str: &str) -> Result<ast::ModuleDefinition> {
    verify_string(modules_str)?;
    syntax::parse_module_string(modules_str).or_else(|e| handle_error(e, modules_str))
}

fn handle_error<T>(e: syntax::ParseError<Loc, anyhow::Error>, code_str: &str) -> Result<T> {
    let location = match &e {
        ParseError::InvalidToken { location, .. } => location,
        ParseError::User { location, .. } => location,
    };
    let mut files = SimpleFiles::new();
    let id = files.add(location.file_hash(), code_str.to_string());
    let lbl = match &e {
        ParseError::InvalidToken { message, .. } => Label::primary(id, location.usize_range())
            .with_message(format!("Invalid Token: {}", message)),
        ParseError::User { error, .. } => {
            Label::primary(id, location.usize_range()).with_message(format!("{}", error))
        }
    };
    let message = lbl.message.clone();
    let error = Diagnostic::error()
        .with_message("Parser Error")
        .with_labels(vec![lbl]);
    let writer = &mut StandardStream::stderr(ColorChoice::Auto);
    emit(writer, &Config::default(), &files, &error).unwrap();
    bail!("ParserError: {}", message)
}

#[cfg(test)]
mod tests {
    #[test]
    fn verify_character_allowlist() {
        let mut good_chars = (0x20..=0x7E).collect::<Vec<u8>>();
        good_chars.push(0x0A);
        good_chars.push(0x09);

        let mut bad_chars = (0x0..0x09).collect::<Vec<_>>();
        bad_chars.append(&mut vec![0x0B, 0x0C]);
        bad_chars.append(&mut (0x0E..=0x1F).collect::<Vec<_>>());
        bad_chars.push(0x7F);

        // Test to make sure that all the characters that are in the allowlist pass.
        {
            let s = std::str::from_utf8(&good_chars)
                .expect("Failed to construct string containing an invalid character. This shouldn't happen.");
            assert!(super::verify_string(s).is_ok());
        }

        // Test to make sure that we fail for all characters not in the allowlist.
        for bad_char in bad_chars {
            good_chars.push(bad_char);
            let s = std::str::from_utf8(&good_chars)
                .expect("Failed to construct string containing an invalid character. This shouldn't happen.");
            assert!(super::verify_string(s).is_err());
            good_chars.pop();
        }
    }
}
