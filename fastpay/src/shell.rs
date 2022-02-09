// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use colored::Colorize;
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::{Config, Context, Editor};
use rustyline_derive::Helper;
use std::fmt::Display;
use std::io;
use std::io::Write;
use structopt::clap::App;
use unescape::unescape;

/// A interactive command line shell with history and completion support
pub struct Shell<P: Display, S, H> {
    pub prompt: P,
    pub state: S,
    pub handler: H,
    pub description: String,
    pub command: CommandStructure,
}

impl<P: Display, S: Send, H: AsyncHandler<S>> Shell<P, S, H> {
    pub async fn run_async(&mut self) -> Result<(), anyhow::Error> {
        let config = Config::builder()
            .auto_add_history(true)
            .history_ignore_space(true)
            .history_ignore_dups(true)
            .build();

        let mut rl = Editor::with_config(config);

        let mut command = self.command.clone();
        let help = CommandStructure {
            name: "help".to_string(),
            completions: command.completions.clone(),
            children: vec![],
        };
        command.children.push(help);
        command.completions.extend([
            "help".to_string(),
            "exit".to_string(),
            "quit".to_string(),
            "clear".to_string(),
        ]);

        rl.set_helper(Some(ShellHelper { command }));

        let mut stdout = io::stdout();

        'shell: loop {
            print!("{}", self.prompt);
            stdout.flush()?;

            // Read a line
            let readline = rl.readline(&self.prompt.to_string());
            let line = match readline {
                Ok(rl_line) => rl_line,
                Err(ReadlineError::Interrupted | ReadlineError::Eof) => break 'shell,
                Err(err) => return Err(err.into()),
            };

            // Runs the line
            match Self::split_and_unescape(line.trim()) {
                Ok(line) => {
                    // do nothing if line is empty
                    if line.is_empty() {
                        continue 'shell;
                    };
                    // safe to unwrap with the above is_empty check.
                    if *line.first().unwrap() == "quit" || *line.first().unwrap() == "exit" {
                        println!("Bye!");
                        break 'shell;
                    };
                    if *line.first().unwrap() == "clear" {
                        // Clear screen and move cursor to top left
                        print!("\x1B[2J\x1B[1;1H");
                        continue 'shell;
                    };
                    if self
                        .handler
                        .handle_async(line, &mut self.state, &self.description)
                        .await
                    {
                        break 'shell;
                    };
                }
                Err(e) => eprintln!("{}", e.red()),
            }
        }
        Ok(())
    }

    fn split_and_unescape(line: &str) -> Result<Vec<String>, String> {
        let mut commands = Vec::new();
        for word in line.split_whitespace() {
            let command = match unescape(word) {
                Some(word) => word,
                None => return Err(format!("Error: Unhandled escape sequence {}", word)),
            };
            commands.push(command);
        }
        Ok(commands)
    }
}

#[derive(Helper)]
struct ShellHelper {
    pub command: CommandStructure,
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

        let candidates = command
            .completions
            .iter()
            .filter(|string| string.starts_with(&last_token) && !previous_tokens.contains(*string))
            .cloned()
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
            .map(|it| CommandStructure {
                name: it.get_name().to_string(),
                completions: it
                    .p
                    .opts
                    .iter()
                    .map(|it| format!("--{}", it.b.name))
                    .collect::<Vec<_>>(),
                children: vec![],
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
    async fn handle_async(&self, args: Vec<String>, state: &mut T, description: &str) -> bool;
}
