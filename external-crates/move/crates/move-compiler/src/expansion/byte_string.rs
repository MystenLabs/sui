// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{diag, diagnostics::Diagnostic, parser::syntax::make_loc};
use move_command_line_common::files::FileHash;
use move_ir_types::location::*;

struct Context {
    file_hash: FileHash,
    start_offset: usize,
    diags: Vec<BytestringError>,
}

#[allow(clippy::enum_variant_names)]
pub(crate) enum BytestringError {
    InvalidHexCharacter { loc: Loc, bad_char: char },
    InvalidEscape { loc: Loc, bad_escape: String },
    InvalidEscapeSeq { loc: Loc, bad_escape: char },
}

impl Context {
    fn new(file_hash: FileHash, start_offset: usize) -> Self {
        Self {
            file_hash,
            start_offset,
            diags: Vec::new(),
        }
    }

    fn escape_error(&mut self, start: usize, end: usize, bad_escape: String) {
        let loc = self.make_file_loc(start, end);
        self.diags
            .push(BytestringError::InvalidEscape { loc, bad_escape });
    }

    fn char_error(&mut self, start: usize, end: usize, bad_char: char) {
        let loc = self.make_file_loc(start, end);
        self.diags
            .push(BytestringError::InvalidHexCharacter { loc, bad_char });
    }

    fn escape_seq_error(&mut self, start: usize, end: usize, bad_escape: char) {
        let loc = self.make_file_loc(start, end);
        self.diags
            .push(BytestringError::InvalidEscapeSeq { loc, bad_escape });
    }

    fn make_file_loc(&self, start: usize, end: usize) -> Loc {
        make_loc(
            self.file_hash,
            self.start_offset + 2 + start, // add 2 for the beginning of the string
            self.start_offset + 2 + end,
        )
    }

    fn has_diags(&self) -> bool {
        !self.diags.is_empty()
    }

    fn get_diags(self) -> Vec<BytestringError> {
        self.diags
    }
}

impl BytestringError {
    pub fn into_diagnostic(self) -> Diagnostic {
        match self {
            BytestringError::InvalidHexCharacter { loc, bad_char: c } => {
                diag!(
                    Syntax::InvalidByteString,
                    (loc, format!("Invalid hexadecimal character: '{c}'"))
                )
            }
            BytestringError::InvalidEscape {
                loc,
                bad_escape: err_text,
            } => {
                let err_text = format!(
                    "Invalid escape: '\\x{}'. Hex literals are represented by two \
             symbols: [\\x00-\\xFF].",
                    err_text
                );
                diag!(Syntax::InvalidByteString, (loc, err_text))
            }
            BytestringError::InvalidEscapeSeq { loc, bad_escape } => {
                let err_text = format!("Invalid escape sequence: '\\{bad_escape}'");
                diag!(Syntax::InvalidByteString, (loc, err_text))
            }
        }
    }
}

pub fn decode(loc: Loc, text: &str) -> Result<Vec<u8>, Vec<BytestringError>> {
    let file_hash = loc.file_hash();
    let start_offset = loc.start() as usize;
    let mut context = Context::new(file_hash, start_offset);
    let mut buffer = vec![];
    let chars: Vec<_> = text.chars().collect();
    decode_(&mut context, &mut buffer, chars);
    if !context.has_diags() {
        Ok(buffer)
    } else {
        Err(context.get_diags())
    }
}

fn decode_(context: &mut Context, buffer: &mut Vec<u8>, chars: Vec<char>) {
    let len = chars.len();
    let mut i = 0;
    macro_rules! next_char {
        () => {{
            let c = chars[i];
            i += 1;
            c
        }};
    }
    macro_rules! next_char_opt {
        () => {{ if i < len { Some(next_char!()) } else { None } }};
    }
    while i < len {
        let cur = i;
        let c = next_char!();
        if c != '\\' {
            push(buffer, c);
            continue;
        }

        match next_char!() {
            'n' => push(buffer, '\n'),
            'r' => push(buffer, '\r'),
            't' => push(buffer, '\t'),
            '\\' => push(buffer, '\\'),
            '0' => push(buffer, '\0'),
            '"' => push(buffer, '"'),
            'x' => {
                let d0_opt = next_char_opt!();
                let d1_opt = next_char_opt!();
                let hex = match (d0_opt, d1_opt) {
                    (Some(d0), Some(d1)) => {
                        let mut hex = String::new();
                        hex.push(d0);
                        hex.push(d1);
                        hex
                    }

                    // Unexpected end of text
                    (d0_opt @ Some(_), None) | (d0_opt @ None, None) => {
                        let err_text = match d0_opt {
                            Some(d0) => format!("{}", d0),
                            None => "".to_string(),
                        };
                        context.escape_error(cur, len, err_text);
                        return;
                    }

                    // There was a second digit but no first?
                    (None, Some(_)) => unreachable!(),
                };
                match hex::decode(hex) {
                    Ok(hex_buffer) => buffer.extend(hex_buffer),
                    Err(hex::FromHexError::InvalidHexCharacter { c, index }) => {
                        context.char_error(cur + 2 + index, cur + 2 + index, c);
                    }
                    Err(_) => unreachable!("ICE unexpected error parsing hex byte string value"),
                }
            }
            c => {
                context.escape_seq_error(cur, cur + 2, c);
            }
        }
    }
}

fn push(buffer: &mut Vec<u8>, ch: char) {
    let mut bytes = vec![0; ch.len_utf8()];
    ch.encode_utf8(&mut bytes);
    buffer.extend(bytes);
}
