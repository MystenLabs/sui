// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::env;
use std::fmt::Display;
use std::io::Write;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use colored::Colorize;
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::{Config, Context, Editor};
use rustyline_derive::Helper;
use structopt::clap::{App, SubCommand};
use unescape::unescape;

#[cfg(test)]
#[path = "unit_tests/shell_tests.rs"]
mod shell_tests;

/// A interactive command line shell with history and completion support
pub struct Shell<P: Display, S, H> {
    prompt: P,
    state: S,
    handler: H,
    command: CommandStructure,
}

impl<P: Display, S: Send, H: AsyncHandler<S>> Shell<P, S, H> {
    pub fn new(prompt: P, state: S, handler: H, mut command: CommandStructure) -> Self {
        // Add help to auto complete
        let help = CommandStructure {
            name: "help".to_string(),
            completions: command.completions.clone(),
            children: vec![],
        };
        command.children.push(help);
        command.completions.extend(["help".to_string()]);

        Self {
            prompt,
            state,
            handler,
            command,
        }
    }

    pub async fn run_async(
        &mut self,
        out: &mut dyn Write,
        err: &mut dyn Write,
    ) -> Result<(), anyhow::Error> {
        let config = Config::builder()
            .auto_add_history(true)
            .history_ignore_space(true)
            .history_ignore_dups(true)
            .build();

        let mut rl = Editor::with_config(config);

        let completion_cache = Arc::new(RwLock::new(BTreeMap::new()));

        rl.set_helper(Some(ShellHelper {
            command: self.command.clone(),
            completion_cache: completion_cache.clone(),
        }));

        'shell: loop {
            write!(out, "{}", self.prompt)?;
            out.flush()?;

            // Read a line
            let readline = rl.readline(&self.prompt.to_string());
            let line = match readline {
                Ok(rl_line) => rl_line,
                Err(ReadlineError::Interrupted | ReadlineError::Eof) => break 'shell,
                Err(err) => return Err(err.into()),
            };

            let line = Self::substitution_env_variables(line);

            // Runs the line
            match Self::split_and_unescape(line.trim()) {
                Ok(line) => {
                    if let Some(s) = line.first() {
                        // These are shell only commands.
                        match s.as_str() {
                            "quit" | "exit" => {
                                writeln!(out, "Bye!")?;
                                break 'shell;
                            }
                            "clear" => {
                                // Clear screen and move cursor to top left
                                write!(out, "\x1B[2J\x1B[1;1H")?;
                                continue 'shell;
                            }
                            "echo" => {
                                let line = line.as_slice()[1..line.len()].join(" ");
                                writeln!(out, "{}", line)?;
                                continue 'shell;
                            }
                            "env" => {
                                for (key, var) in env::vars() {
                                    writeln!(out, "{}={}", key, var)?;
                                }
                                continue 'shell;
                            }
                            _ => {}
                        }
                    } else {
                        // do nothing if line is empty
                        continue 'shell;
                    }

                    if self
                        .handler
                        .handle_async(line, &mut self.state, completion_cache.clone())
                        .await
                    {
                        break 'shell;
                    };
                }
                Err(e) => writeln!(err, "{}", e.red())?,
            }
        }
        Ok(())
    }

    fn substitution_env_variables(s: String) -> String {
        if !s.contains('$') {
            return s;
        }
        let mut env = env::vars().collect::<Vec<_>>();
        // Sort variable name by the length in descending order, to prevent wrong substitution by variable with partial same name.
        env.sort_by(|(k1, _), (k2, _)| Ord::cmp(&k2.len(), &k1.len()));

        for (key, value) in env {
            let var = format!("${}", key);
            if s.contains(&var) {
                let result = s.replace(var.as_str(), value.as_str());
                return if result.contains('$') {
                    Self::substitution_env_variables(result)
                } else {
                    result
                };
            }
        }
        s
    }

    fn split_and_unescape(line: &str) -> Result<Vec<String>, String> {
        let mut commands = Vec::new();
        for word in line.split_whitespace() {
            let command = unescape(word)
                .ok_or_else(|| format!("Error: Unhandled escape sequence {}", word))?;
            commands.push(command);
        }
        Ok(commands)
    }
}

pub fn install_shell_plugins<'a>(clap: App<'a, 'a>) -> App<'a, 'a> {
    clap.subcommand(
        SubCommand::with_name("exit")
            .alias("quit")
            .about("Exit the interactive shell"),
    )
    .subcommand(SubCommand::with_name("clear").about("Clear screen"))
    .subcommand(SubCommand::with_name("echo").about("Write arguments to the console output"))
    .subcommand(SubCommand::with_name("env").about("Print environment"))
}

#[derive(Helper)]
struct ShellHelper {
    pub command: CommandStructure,
    pub completion_cache: CompletionCache,
}

impl Hinter for ShellHelper {
    type Hint = String;
}

impl Highlighter for ShellHelper {}

impl Validator for ShellHelper {}

impl Completer for ShellHelper {
    type Candidate = Pair;
    fn complete(
        &self,
        line: &str,
        _pos: usize,
        _ctx: &Context<'_>,
    ) -> Result<(usize, Vec<Self::Candidate>), rustyline::error::ReadlineError> {
        let line = format!("{}_", line);
        // split line
        let mut tokens = line.split_whitespace();
        let mut last_token = tokens.next_back().unwrap().to_string();
        last_token.pop();

        let mut command = &self.command;
        let mut previous_tokens = Vec::new();
        for tok in tokens {
            let next_cmd = command.get_child(tok);
            if let Some(next_command) = next_cmd {
                command = next_command;
            }
            previous_tokens.push(tok.to_string());
        }

        let completions = command.completions.clone();
        let cache_key = CacheKey::new(
            &command.name,
            &previous_tokens.last().cloned().unwrap_or_default(),
        );
        let mut completion_from_cache = self
            .completion_cache
            .read()
            .map(|cache| cache.get(&cache_key).cloned().unwrap_or_default())
            .unwrap_or_default();

        completion_from_cache.extend(completions);

        let candidates = completion_from_cache
            .into_iter()
            .filter(|string| string.starts_with(&last_token) && !previous_tokens.contains(string))
            .collect::<Vec<_>>();

        Ok((
            line.len() - last_token.len() - 1,
            candidates
                .iter()
                .map(|cmd| Pair {
                    display: cmd.to_string(),
                    replacement: cmd.to_string(),
                })
                .collect(),
        ))
    }
}

#[derive(Clone)]
pub struct CommandStructure {
    pub name: String,
    pub completions: Vec<String>,
    pub children: Vec<CommandStructure>,
}

impl CommandStructure {
    /// Create CommandStructure using structopt::clap::App, currently only support 1 level of subcommands
    pub fn from_clap(app: &App) -> Self {
        let subcommands = app
            .p
            .subcommands
            .iter()
            .map(|it| {
                let name = it.get_name();
                CommandStructure {
                    name: name.to_string(),
                    completions: it
                        .p
                        .opts
                        .iter()
                        .map(|it| format!("--{}", it.b.name))
                        .collect::<Vec<_>>(),
                    children: vec![],
                }
            })
            .collect::<Vec<_>>();

        Self::from_children("", subcommands)
    }

    fn from_children(name: &str, children: Vec<CommandStructure>) -> Self {
        let completions = children
            .iter()
            .map(|child| child.name.to_string())
            .collect();
        Self {
            name: name.to_string(),
            completions,
            children,
        }
    }

    fn get_child(&self, name: &str) -> Option<&CommandStructure> {
        for subcommand in self.children.iter() {
            if subcommand.name == name {
                return Some(subcommand);
            }
        }
        None
    }
}

#[async_trait]
pub trait AsyncHandler<T: Send> {
    async fn handle_async(
        &self,
        args: Vec<String>,
        state: &mut T,
        completion_cache: CompletionCache,
    ) -> bool;
}

pub type CompletionCache = Arc<RwLock<BTreeMap<CacheKey, Vec<String>>>>;

#[derive(PartialEq)]
pub struct CacheKey {
    command: String,
    flag: String,
}
impl CacheKey {
    pub fn new(command: &str, flag: &str) -> Self {
        Self {
            command: command.to_string(),
            flag: flag.to_string(),
        }
    }
}
impl Eq for CacheKey {}

impl PartialOrd<Self> for CacheKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let cmd_eq = if self.command == "*" || other.command == "*" {
            Some(Ordering::Equal)
        } else {
            self.command.partial_cmp(&other.command)
        };

        if cmd_eq != Some(Ordering::Equal) {
            return cmd_eq;
        }
        if self.flag == "*" || other.flag == "*" {
            Some(Ordering::Equal)
        } else {
            self.flag.partial_cmp(&other.flag)
        }
    }
}

impl Ord for CacheKey {
    fn cmp(&self, other: &Self) -> Ordering {
        let cmd_eq = if self.command == "*" || other.command == "*" {
            Ordering::Equal
        } else {
            self.command.cmp(&other.command)
        };

        if cmd_eq != Ordering::Equal {
            return cmd_eq;
        }
        if self.flag == "*" || other.flag == "*" {
            Ordering::Equal
        } else {
            self.flag.cmp(&other.flag)
        }
    }
}
