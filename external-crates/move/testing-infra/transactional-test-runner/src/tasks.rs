// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use anyhow::{anyhow, bail, Result};
use clap::*;
use move_command_line_common::{
    address::ParsedAddress,
    files::{MOVE_EXTENSION, MOVE_IR_EXTENSION},
    types::{ParsedStructType, ParsedType},
    values::{ParsableValue, ParsedValue},
};
use move_compiler::shared::NumericalAddress;
use move_core_types::identifier::Identifier;
use std::{convert::TryInto, fmt::Debug, path::Path, str::FromStr};
use tempfile::NamedTempFile;

#[derive(Debug)]
pub struct TaskInput<Command> {
    pub command: Command,
    pub name: String,
    pub number: usize,
    pub start_line: usize,
    pub command_lines_stop: usize,
    pub stop_line: usize,
    pub data: Option<NamedTempFile>,
}

pub fn taskify<Command: Debug + Parser>(filename: &Path) -> Result<Vec<TaskInput<Command>>> {
    use regex::Regex;
    use std::{
        fs::File,
        io::{self, BufRead, Write},
    };
    // checks for lines that are entirely whitespace
    let re_whitespace = Regex::new(r"^\s*$").unwrap();
    // checks for lines that start with // comments
    // here the next character is whitespace or an ASCII character other than #
    let re_comment = Regex::new(r"^\s*//(\s|[\x20-\x22]|[[\x24-\x7E]])").unwrap();
    // checks for lines that start with //# commands
    // cutting leading/trailing whitespace
    // capturing the command text
    let re_command_text = Regex::new(r"^\s*//#\s*(.*)\s*$").unwrap();

    let file = File::open(filename).unwrap();
    let lines: Vec<String> = io::BufReader::new(file)
        .lines()
        .map(|ln| ln.expect("Could not parse line"))
        .collect();

    let lines_iter = lines.into_iter().enumerate().map(|(idx, l)| (idx + 1, l));
    let skipped_whitespace = lines_iter.skip_while(|(_line_number, line)| {
        re_whitespace.is_match(line) || re_comment.is_match(line)
    });
    let mut bucketed_lines = vec![];
    let mut cur_commands = vec![];
    let mut cur_text = vec![];
    let mut in_command = true;
    for (line_number, line) in skipped_whitespace {
        if let Some(captures) = re_command_text.captures(&line) {
            if !in_command {
                bucketed_lines.push((cur_commands, cur_text));
                cur_commands = vec![];
                cur_text = vec![];
                in_command = true;
            }
            let command_text = match captures.len() {
                1 => continue,
                2 => captures.get(1).unwrap().as_str().to_string(),
                n => panic!("COMMAND_TEXT captured {}. expected 1 or 2", n),
            };
            if command_text.is_empty() {
                continue;
            }
            cur_commands.push((line_number, command_text))
        } else if re_whitespace.is_match(&line) {
            in_command = false;
            continue;
        } else {
            in_command = false;
            cur_text.push((line_number, line))
        }
    }
    bucketed_lines.push((cur_commands, cur_text));

    if bucketed_lines.is_empty() {
        return Ok(vec![]);
    }

    let mut tasks = vec![];
    for (number, (commands, text)) in bucketed_lines.into_iter().enumerate() {
        if commands.is_empty() {
            assert!(number == 0);
            bail!("No initial command")
        }

        let start_line = commands.first().unwrap().0;
        let command_lines_stop = commands.last().unwrap().0;
        let mut command_text = "task ".to_string();
        for (line_number, text) in commands {
            assert!(!text.is_empty(), "{}: {}", line_number, text);
            command_text = format!("{} {}", command_text, text);
        }
        let command_split = command_text.split_ascii_whitespace().collect::<Vec<_>>();
        let name_opt = command_split.get(1).map(|s| (*s).to_owned());
        let command = match Command::try_parse_from(command_split) {
            Ok(command) => command,
            Err(e) => {
                let mut spit_iter = command_text.split_ascii_whitespace();
                // skip 'task'
                spit_iter.next();
                let help_command = match spit_iter.next() {
                    None => vec!["task", "--help"],
                    Some(c) => vec!["task", c, "--help"],
                };
                let help = match Command::try_parse_from(help_command) {
                    Ok(_) => panic!(),
                    Err(e) => e,
                };
                bail!(
                    "Invalid command. Got error {}\nLines {} - {}.\n{}",
                    e,
                    start_line,
                    command_lines_stop,
                    help
                )
            }
        };
        let name = name_opt.unwrap();

        let stop_line = if text.is_empty() {
            command_lines_stop
        } else {
            text[text.len() - 1].0
        };

        // Keep fucking this up somehow
        // let last_non_whitespace = text
        //     .iter()
        //     .rposition(|(_, l)| !WHITESPACE.is_match(l))
        //     .unwrap_or(0);
        // let initial_text = text
        //     .into_iter()
        //     .take_while(|(i, _)| *i < last_non_whitespace)
        //     .map(|(_, l)| l);
        let file_text_vec = (0..command_lines_stop)
            .map(|_| String::new())
            .chain(text.into_iter().map(|(_ln, l)| l))
            .collect::<Vec<String>>();
        let data = if file_text_vec.iter().all(|s| re_whitespace.is_match(s)) {
            None
        } else {
            let data = NamedTempFile::new()?;
            data.reopen()?
                .write_all(file_text_vec.join("\n").as_bytes())?;
            Some(data)
        };

        tasks.push(TaskInput {
            command,
            name,
            number,
            start_line,
            command_lines_stop,
            stop_line,
            data,
        })
    }
    Ok(tasks)
}

impl<T> TaskInput<T> {
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> TaskInput<U> {
        let Self {
            command,
            name,
            number,
            start_line,
            command_lines_stop,
            stop_line,
            data,
        } = self;
        TaskInput {
            command: f(command),
            name,
            number,
            start_line,
            command_lines_stop,
            stop_line,
            data,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyntaxChoice {
    Source,
    IR,
}

/// When printing bytecode, the input program must either be a script or a module.
#[derive(Debug)]
pub enum PrintBytecodeInputChoice {
    Script,
    Module,
}

/// Translates the given Move IR module or script into bytecode, then prints a textual
/// representation of that bytecode.
#[derive(Debug, Parser)]
pub struct PrintBytecodeCommand {
    /// The kind of input: either a script, or a module.
    #[clap(long = "input", ignore_case = true, default_value = "script")]
    pub input: PrintBytecodeInputChoice,
}

#[derive(Debug, Parser)]
pub struct InitCommand {
    #[clap(
        long = "addresses",
        parse(try_from_str = move_compiler::shared::parse_named_address),
        takes_value(true),
        multiple_values(true),
        multiple_occurrences(true)
    )]
    pub named_addresses: Vec<(String, NumericalAddress)>,
}

#[derive(Debug, Parser)]
pub struct PublishCommand {
    #[clap(long = "gas-budget")]
    pub gas_budget: Option<u64>,
    #[clap(long = "syntax")]
    pub syntax: Option<SyntaxChoice>,
}

#[derive(Debug, Parser)]
pub struct RunCommand<ExtraValueArgs: ParsableValue> {
    #[clap(
        long = "signers",
        parse(try_from_str = ParsedAddress::parse),
        takes_value(true),
        multiple_values(true),
        multiple_occurrences(true)
    )]
    pub signers: Vec<ParsedAddress>,
    #[clap(
        long = "args",
        parse(try_from_str = ParsedValue::parse),
        takes_value(true),
        multiple_values(true),
        multiple_occurrences(true)
    )]
    pub args: Vec<ParsedValue<ExtraValueArgs>>,
    #[clap(
        long = "type-args",
        parse(try_from_str = ParsedType::parse),
        takes_value(true),
        multiple_values(true),
        multiple_occurrences(true)
    )]
    pub type_args: Vec<ParsedType>,
    #[clap(long = "gas-budget")]
    pub gas_budget: Option<u64>,
    #[clap(long = "syntax")]
    pub syntax: Option<SyntaxChoice>,
    #[clap(name = "NAME", parse(try_from_str = parse_qualified_module_access))]
    pub name: Option<(ParsedAddress, Identifier, Identifier)>,
}

#[derive(Debug, Parser)]
pub struct ViewCommand {
    #[clap(long = "address", parse(try_from_str = ParsedAddress::parse))]
    pub address: ParsedAddress,
    #[clap(long = "resource", parse(try_from_str = ParsedStructType::parse))]
    pub resource: ParsedStructType,
}

#[derive(Debug)]
pub enum TaskCommand<
    ExtraInitArgs: Parser,
    ExtraPublishArgs: Parser,
    ExtraValueArgs: ParsableValue,
    ExtraRunArgs: Parser,
    SubCommands: Parser,
> {
    Init(InitCommand, ExtraInitArgs),
    PrintBytecode(PrintBytecodeCommand),
    Publish(PublishCommand, ExtraPublishArgs),
    Run(RunCommand<ExtraValueArgs>, ExtraRunArgs),
    View(ViewCommand),
    Subcommand(SubCommands),
}

impl<
        ExtraInitArgs: Parser,
        ExtraPublishArgs: Parser,
        ExtraValueArgs: ParsableValue,
        ExtraRunArgs: Parser,
        SubCommands: Parser,
    > FromArgMatches
    for TaskCommand<ExtraInitArgs, ExtraPublishArgs, ExtraValueArgs, ExtraRunArgs, SubCommands>
{
    fn from_arg_matches(matches: &ArgMatches) -> Result<Self, Error> {
        Ok(match matches.subcommand() {
            Some(("init", matches)) => TaskCommand::Init(
                FromArgMatches::from_arg_matches(matches)?,
                FromArgMatches::from_arg_matches(matches)?,
            ),
            Some(("print-bytecode", matches)) => {
                TaskCommand::PrintBytecode(FromArgMatches::from_arg_matches(matches)?)
            }
            Some(("publish", matches)) => TaskCommand::Publish(
                FromArgMatches::from_arg_matches(matches)?,
                FromArgMatches::from_arg_matches(matches)?,
            ),
            Some(("run", matches)) => TaskCommand::Run(
                FromArgMatches::from_arg_matches(matches)?,
                FromArgMatches::from_arg_matches(matches)?,
            ),
            Some(("view", matches)) => {
                TaskCommand::View(FromArgMatches::from_arg_matches(matches)?)
            }
            _ => TaskCommand::Subcommand(SubCommands::from_arg_matches(matches)?),
        })
    }

    fn update_from_arg_matches(&mut self, matches: &ArgMatches) -> Result<(), Error> {
        *self = Self::from_arg_matches(matches)?;
        Ok(())
    }
}

impl<
        ExtraInitArgs: Parser,
        ExtraPublishArgs: Parser,
        ExtraValueArgs: ParsableValue,
        ExtraRunArgs: Parser,
        SubCommands: Parser,
    > CommandFactory
    for TaskCommand<ExtraInitArgs, ExtraPublishArgs, ExtraValueArgs, ExtraRunArgs, SubCommands>
{
    fn into_app<'help>() -> Command<'help> {
        SubCommands::command()
            .name("Task Command")
            .subcommand(InitCommand::augment_args(ExtraInitArgs::command()).name("init"))
            .subcommand(PrintBytecodeCommand::command().name("print-bytecode"))
            .subcommand(PublishCommand::augment_args(ExtraPublishArgs::command()).name("publish"))
            .subcommand(
                RunCommand::<ExtraValueArgs>::augment_args(ExtraRunArgs::command()).name("run"),
            )
            .subcommand(ViewCommand::command().name("view"))
    }

    fn into_app_for_update<'help>() -> Command<'help> {
        todo!()
    }
}
// Note: this needs to be manually implemented because clap cannot handle generic tuples
// with more than 1 element currently.
//
// The code is a simplified version of what `#[derive(Parser)` would generate had it worked.
// (`cargo expand` is useful in printing out the derived code.)
//
impl<
        ExtraInitArgs: Parser,
        ExtraPublishArgs: Parser,
        ExtraValueArgs: ParsableValue,
        ExtraRunArgs: Parser,
        SubCommands: Parser,
    > Parser
    for TaskCommand<ExtraInitArgs, ExtraPublishArgs, ExtraValueArgs, ExtraRunArgs, SubCommands>
{
}

#[derive(Debug, Parser)]
pub struct EmptyCommand {}

fn parse_qualified_module_access(s: &str) -> Result<(ParsedAddress, Identifier, Identifier)> {
    let [addr_str, module_str, struct_str]: [&str; 3] =
        s.split("::").collect::<Vec<_>>().try_into().map_err(|e| {
            anyhow!(
                "Invalid module access. \
                 Expected 3 distinct parts, address, module, and struct. Got error {:?}",
                e
            )
        })?;
    let addr = ParsedAddress::parse(addr_str)?;
    let module = Identifier::new(module_str)?;
    let struct_ = Identifier::new(struct_str)?;
    Ok((addr, module, struct_))
}

impl FromStr for SyntaxChoice {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            MOVE_EXTENSION => Ok(SyntaxChoice::Source),
            MOVE_IR_EXTENSION => Ok(SyntaxChoice::IR),
            _ => Err(anyhow!(
                "Invalid syntax choice. Expected '{}' or '{}'",
                MOVE_EXTENSION,
                MOVE_IR_EXTENSION
            )),
        }
    }
}

impl FromStr for PrintBytecodeInputChoice {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "script" => Ok(PrintBytecodeInputChoice::Script),
            "module" => Ok(PrintBytecodeInputChoice::Module),
            _ => Err(anyhow!(
                "Invalid input choice. Expected 'script' or 'module'"
            )),
        }
    }
}
