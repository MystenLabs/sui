// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use colored::Colorize;
use crossterm::event::{KeyCode, KeyModifiers};
use reedline::{
    default_emacs_keybindings, ComplationActionHandler, Completer, DefaultCompletionActionHandler,
    Emacs, Prompt, PromptEditMode, PromptHistorySearch, PromptHistorySearchStatus, Reedline,
    ReedlineEvent, Signal, Span,
};
use std::borrow::Cow;
use std::fmt::Display;
use std::io;
use std::io::Write;
use structopt::clap::App;

/// A interactive command line shell with history and completion support
pub struct Shell<P: Display, S, H> {
    pub prompt: P,
    pub state: S,
    pub handler: H,
    pub description: String,
    pub commands: CommandStructure,
}

impl<P: Display, S: Send, H: AsyncHandler<S>> Shell<P, S, H> {
    pub async fn run_async(&mut self) -> Result<(), anyhow::Error> {
        let mut keybindings = default_emacs_keybindings();
        keybindings.add_binding(
            KeyModifiers::CONTROL,
            KeyCode::Char('d'),
            ReedlineEvent::CtrlD,
        );
        keybindings.add_binding(
            KeyModifiers::CONTROL,
            KeyCode::Char('c'),
            ReedlineEvent::CtrlC,
        );

        let mut line_editor = Reedline::create()?
            .with_edit_mode(Box::new(Emacs::new(keybindings)))
            .with_completion_action_handler(self.get_competition_handler());

        let mut stdout = io::stdout();
        let prompt = FastXPrompt {
            prompt_indicator: "fastx>-$ ".to_string(),
        };

        'shell: loop {
            stdout.flush()?;
            // Read a line
            let sig = line_editor.read_line(&prompt)?;
            let line = match sig {
                Signal::CtrlD | Signal::CtrlC => {
                    let _ = line_editor.print_crlf();
                    println!("Bye!");
                    break 'shell;
                }
                Signal::CtrlL => {
                    line_editor.clear_screen()?;
                    continue 'shell;
                }
                Signal::Success(buffer) => buffer,
            };

            // Runs the line
            match Self::unescape(line.trim()) {
                Ok(line) => {
                    // do nothing if line is empty
                    if line.is_empty() {
                        continue 'shell;
                    };
                    // safe to unwrap with the above is_empty check.
                    if *line.first().unwrap() == "clear" {
                        line_editor.clear_screen()?;
                        continue 'shell;
                    };
                    if *line.first().unwrap() == "quit" || *line.first().unwrap() == "exit" {
                        println!("Bye!");
                        break 'shell;
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

    fn unescape(command: &str) -> Result<Vec<String>, String> {
        // Create a vec to store the split int.
        let mut vec = vec![String::new()];

        // Are we in an escape sequence?
        let mut escape = false;

        // Are we in a string?
        let mut string = false;

        // Go through each char in the string
        for c in command.chars() {
            let segment = vec.last_mut().unwrap();
            if escape {
                match c {
                    '\\' => segment.push('\\'),
                    ' ' if !string => segment.push(' '),
                    'n' => segment.push('\n'),
                    'r' => segment.push('\r'),
                    't' => segment.push('\t'),
                    '"' => segment.push('"'),
                    _ => return Err(format!("Error: Unhandled escape sequence \\{}", c)),
                }
                escape = false;
            } else {
                match c {
                    '\\' => escape = true,
                    '"' => string = !string,
                    ' ' if string => segment.push(c),
                    ' ' if !string => vec.push(String::new()),
                    _ => segment.push(c),
                }
            }
        }

        if vec.len() == 1 && vec[0].is_empty() {
            vec.clear();
        }
        Ok(vec)
    }

    fn get_competition_handler(&self) -> Box<dyn ComplationActionHandler> {
        let mut command = self.commands.clone();
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

        Box::new(
            DefaultCompletionActionHandler::default()
                .with_completer(Box::new(FastXCompleter { command })),
        )
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

pub struct FastXPrompt {
    prompt_indicator: String,
}

impl Prompt for FastXPrompt {
    fn render_prompt(&self, _: usize) -> Cow<str> {
        "".into()
    }

    fn render_prompt_indicator(&self, _: PromptEditMode) -> Cow<str> {
        let prompt = &*self.prompt_indicator;
        prompt.into()
    }

    fn render_prompt_multiline_indicator(&self) -> Cow<str> {
        Cow::Borrowed(">")
    }

    fn render_prompt_history_search_indicator(
        &self,
        history_search: PromptHistorySearch,
    ) -> Cow<str> {
        let prefix = match history_search.status {
            PromptHistorySearchStatus::Passing => "",
            PromptHistorySearchStatus::Failing => "failing ",
        };
        Cow::Owned(format!(
            "({}reverse-search: {})",
            prefix, history_search.term
        ))
    }
}

pub struct FastXCompleter {
    pub command: CommandStructure,
}

impl Completer for FastXCompleter {
    fn complete(&self, line: &str, pos: usize) -> Vec<(Span, String)> {
        let line = format!("{}_", line);
        // split line
        let mut tokens = line.split_whitespace();
        let mut last_token = tokens.next_back().unwrap().to_string();
        last_token.pop();

        let mut command = &self.command;

        let mut previous_tokens = Vec::new();
        for token in tokens {
            if let Some(next_command) = command.get_child(token) {
                command = next_command;
            }
            previous_tokens.push(token.to_string());
        }

        let mut candidates = command
            .completions
            .iter()
            .filter(|string| string.starts_with(&last_token) && !previous_tokens.contains(*string))
            .cloned()
            .collect::<Vec<_>>();

        candidates.sort();

        let start = line.len() - last_token.len() - 1;

        candidates
            .iter()
            .map(|cmd| (Span::new(start, pos), cmd.to_string()))
            .collect::<Vec<_>>()
    }
}
