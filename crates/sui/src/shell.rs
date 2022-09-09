// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use std::borrow::Cow;
use std::borrow::Cow::Owned;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::env;
use std::fmt::Display;
use std::io::Write;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use clap::*;
use colored::Colorize;
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::{Config, Context, Editor};
use rustyline_derive::Helper;
use unescape::unescape;

#[path = "unit_tests/shell_tests.rs"]
#[cfg(test)]
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
        out: &mut (dyn Write + Send),
        err: &mut (dyn Write + Send),
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

        loop {
            // Read a line
            let readline = rl.readline(&self.prompt.to_string());
            let line = match readline {
                Ok(rl_line) => rl_line,
                Err(ReadlineError::Interrupted | ReadlineError::Eof) => break,
                Err(err) => return Err(err.into()),
            };

            let line = substitute_env_variables(line);

            // Runs the line
            match split_and_unescape(line.trim()) {
                Ok(line) => {
                    if let Some(s) = line.first() {
                        // These are shell only commands.
                        match s.as_str() {
                            "quit" | "exit" => {
                                writeln!(out, "Bye!")?;
                                break;
                            }
                            "clear" => {
                                // Clear screen and move cursor to top left
                                write!(out, "\x1B[2J\x1B[1;1H")?;
                                continue;
                            }
                            "echo" => {
                                let line = line.as_slice()[1..line.len()].join(" ");
                                writeln!(out, "{}", line)?;
                                continue;
                            }
                            "env" => {
                                for (key, var) in env::vars() {
                                    writeln!(out, "{}={}", key, var)?;
                                }
                                continue;
                            }
                            "history" => {
                                for (pos, history) in rl.history().iter().enumerate() {
                                    println!(" {} {}", pos + 1, history);
                                }
                                continue;
                            }
                            _ => {}
                        }
                    } else {
                        // do nothing if line is empty
                        continue;
                    }

                    if self
                        .handler
                        .handle_async(line, &mut self.state, completion_cache.clone())
                        .await
                    {
                        break;
                    };
                }
                Err(e) => writeln!(err, "{}", e.to_string().red())?,
            }
        }
        Ok(())
    }
}

fn split_and_unescape(line: &str) -> Result<Vec<String>, anyhow::Error> {
    let mut commands = Vec::new();
    let split: Vec<String> = shell_words::split(line)?;

    for word in split {
        let command =
            unescape(&word).ok_or_else(|| anyhow!("Error: Unhandled escape sequence {word}"))?;
        commands.push(command);
    }
    Ok(commands)
}

fn substitute_env_variables(s: String) -> String {
    if !s.contains('$') {
        return s;
    }
    let mut env = env::vars().collect::<Vec<_>>();
    // Sort variable name by the length in descending order, to prevent wrong substitution by variable with partial same name.
    env.sort_by(|(k1, _), (k2, _)| Ord::cmp(&k2.len(), &k1.len()));

    for (key, value) in env {
        let var = format!("${key}");
        if s.contains(&var) {
            let result = s.replace(var.as_str(), value.as_str());
            return if result.contains('$') {
                substitute_env_variables(result)
            } else {
                result
            };
        }
    }
    s
}

pub fn install_shell_plugins(clap: Command) -> Command {
    clap.subcommand(
        Command::new("exit")
            .alias("quit")
            .about("Exit the interactive shell"),
    )
    .subcommand(Command::new("clear").about("Clear screen"))
    .subcommand(Command::new("echo").about("Write arguments to the console output"))
    .subcommand(Command::new("env").about("Print environment"))
    .subcommand(Command::new("history").about("Print history"))
}

#[derive(Helper)]
struct ShellHelper {
    pub command: CommandStructure,
    pub completion_cache: CompletionCache,
}

impl Hinter for ShellHelper {
    type Hint = String;
}

impl Highlighter for ShellHelper {
    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(
        &'s self,
        prompt: &'p str,
        _default: bool,
    ) -> Cow<'b, str> {
        Owned(prompt.bold().green().to_string())
    }
}

impl Validator for ShellHelper {}

impl Completer for ShellHelper {
    type Candidate = Pair;
    fn complete(
        &self,
        line: &str,
        _pos: usize,
        _ctx: &Context<'_>,
    ) -> Result<(usize, Vec<Self::Candidate>), rustyline::error::ReadlineError> {
        let line = format!("{line}_");
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
        let cache_key = CacheKey {
            command: Some(command.name.clone()),
            flag: previous_tokens.last().cloned().unwrap_or_default(),
        };
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
    /// Create CommandStructure using clap::Command, currently only support 1 level of subcommands
    pub fn from_clap(app: &Command) -> Self {
        let subcommands = app
            .get_subcommands()
            .map(|it| {
                let name = it.get_name();
                CommandStructure {
                    name: name.to_string(),
                    completions: it
                        .get_opts()
                        .map(|it| match it.get_long() {
                            Some(long) => format!("--{}", long),
                            None => format!("--{}", name),
                        })
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
        self.children
            .iter()
            .find(|&subcommand| subcommand.name == name)
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
/// A special key for `CompletionCache` which will perform wildcard key matching.
/// Command field is optional and it will be treated as wildcard if `None`
pub struct CacheKey {
    command: Option<String>,
    flag: String,
}
impl CacheKey {
    pub fn new(command: &str, flag: &str) -> Self {
        Self {
            command: Some(command.to_string()),
            flag: flag.to_string(),
        }
    }
    pub fn flag(flag: &str) -> Self {
        Self {
            command: None,
            flag: flag.to_string(),
        }
    }
}
impl Eq for CacheKey {}

impl PartialOrd<Self> for CacheKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
/// This custom ordering for `CacheKey` enable wildcard matching,
/// the command field for `CacheKey` is optional and can be used as a wildcard when equal `None`
/// # Examples
/// ```
/// use std::cmp::Ordering;
/// use std::collections::BTreeMap;
/// use sui::shell::CacheKey;
///
/// assert_eq!(Ordering::Equal, CacheKey::flag("--flag").cmp(&CacheKey::new("any command", "--flag")));
///
/// let mut data = BTreeMap::new();
/// data.insert(CacheKey::flag("--flag"), "Some Data");
///
/// assert_eq!(Some(&"Some Data"), data.get(&CacheKey::new("This can be anything", "--flag")));
/// assert_eq!(Some(&"Some Data"), data.get(&CacheKey::flag("--flag")));
/// ```
impl Ord for CacheKey {
    fn cmp(&self, other: &Self) -> Ordering {
        let cmd_eq = if self.command.is_none() || other.command.is_none() {
            Ordering::Equal
        } else {
            self.command.cmp(&other.command)
        };

        if cmd_eq != Ordering::Equal {
            cmd_eq
        } else {
            self.flag.cmp(&other.flag)
        }
    }
}
