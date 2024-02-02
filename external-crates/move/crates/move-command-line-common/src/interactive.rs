// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    fmt::Display,
    io::{BufRead, Write},
};

use once_cell::sync::Lazy;

pub struct Terminal<'a, W: Write, R: BufRead> {
    pub writer: &'a mut W,
    pub reader: &'a mut R,
}

static NEWLINE: Lazy<&[u8]> = Lazy::new(|| "\n".as_bytes());
static POS_YES_NO_PROMPT: Lazy<&[u8]> = Lazy::new(|| "(Y/n) ".as_bytes());
static NEG_YES_NO_PROMPT: Lazy<&[u8]> = Lazy::new(|| "(y/N) ".as_bytes());

impl<'a, W: Write, R: BufRead> Terminal<'a, W, R> {
    pub fn new<'new>(writer: &'new mut W, reader: &'new mut R) -> Terminal<'new, W, R> {
        Terminal { writer, reader }
    }

    fn write_bytes(&mut self, bytes: &[u8]) -> anyhow::Result<()> {
        self.writer.write_all(bytes)?;
        Ok(())
    }

    pub fn write(&mut self, msg: &str) -> anyhow::Result<()> {
        self.write_bytes(msg.as_bytes())
    }

    pub fn flush(&mut self) -> anyhow::Result<()> {
        self.writer.flush()?;
        Ok(())
    }

    pub fn newline(&mut self) -> anyhow::Result<()> {
        self.write_bytes(*NEWLINE)
    }

    pub fn writeln(&mut self, msg: &str) -> anyhow::Result<()> {
        self.write(msg)?;
        self.newline()
    }

    pub fn option_prompt<Value: Clone + Display>(
        &mut self,
        prompt: &str,
        options: &BTreeMap<String, Value>,
    ) -> anyhow::Result<Value> {
        self.writeln(prompt)?;
        self.newline()?;
        for (key, opt) in options {
            self.write(format!("{}) {}\n", key, opt).as_str())?;
        }
        self.newline()?;

        let default = options.keys().next().unwrap();
        loop {
            self.write(format!("Selection (default={}): ", default).as_str())?;
            self.flush()?;
            let mut input = String::new();
            self.reader.read_line(&mut input)?;
            let input = input.trim().to_string();
            if input.is_empty() {
                break Ok(options.get(default).unwrap().clone());
            }
            if let Some(selection) = options.get(&input.trim().to_string()) {
                break Ok(selection.clone());
            } else {
                self.write("\nInvalid selection. Please try again (or use Ctrl+C to quit).\n")?;
            }
        }
    }

    pub fn yes_no_prompt(&mut self, prompt: &str, default: bool) -> anyhow::Result<bool> {
        loop {
            self.write(prompt)?;
            self.write_bytes(" ".as_bytes())?;
            if default {
                self.write_bytes(*POS_YES_NO_PROMPT)?;
            } else {
                self.write_bytes(*NEG_YES_NO_PROMPT)?;
            }
            self.flush()?;
            let mut input = String::new();
            self.reader.read_line(&mut input)?;
            let input = input.trim().to_lowercase().to_string();
            if input.is_empty() {
                break Ok(default);
            } else if input.starts_with('y') {
                break Ok(true);
            } else if input.starts_with('n') {
                break Ok(false);
            }
        }
    }
}
