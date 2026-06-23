// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Interactive shell loop for the db-shell command.

use anyhow::bail;
use base64::Engine;
use rustyline::Editor;
use rustyline::error::ReadlineError;
use rustyline::history::FileHistory;
use std::io::Write as _;
use std::sync::Arc;

use crate::db_shell::{
    backend::Backend,
    completion::ShellHelper,
    vfs::{VfsPath, resolve_path},
};

const DEFAULT_LIMIT: usize = 30;

pub fn run_shell(backend: Arc<dyn Backend>, initial_path: VfsPath) -> anyhow::Result<()> {
    let helper = ShellHelper {
        backend: backend.clone(),
        cwd: initial_path.clone(),
    };

    let mut rl: Editor<ShellHelper, FileHistory> = Editor::new()?;
    rl.set_helper(Some(helper));

    let mut cwd = initial_path;

    loop {
        let prompt = format!("sui-db:{}> ", cwd);
        // Keep the helper's CWD in sync so tab completion reflects the current directory.
        if let Some(h) = rl.helper_mut() {
            h.cwd = cwd.clone();
        }
        match rl.readline(&prompt) {
            Ok(line_raw) => {
                let line = line_raw.trim().to_string();
                if line.is_empty() {
                    continue;
                }
                let _ = rl.add_history_entry(&line);

                match dispatch(&line, &mut cwd, &backend) {
                    Ok(true) => break,
                    Ok(false) => {}
                    Err(e) => eprintln!("error: {e}"),
                }
            }
            Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => break,
            Err(e) => {
                eprintln!("readline error: {e}");
                break;
            }
        }
    }

    Ok(())
}

/// Returns Ok(true) to signal exit.
fn dispatch(line: &str, cwd: &mut VfsPath, backend: &Arc<dyn Backend>) -> anyhow::Result<bool> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.is_empty() {
        return Ok(false);
    }

    match tokens[0] {
        "exit" | "quit" | "q" => return Ok(true),
        "pwd" => println!("{cwd}"),
        "cd" => cmd_cd(tokens.get(1).copied(), cwd)?,
        "ls" => cmd_ls(&tokens[1..], cwd, backend)?,
        "cat" => cmd_cat(&tokens[1..], cwd, backend)?,
        "dbg" => cmd_dbg(&tokens[1..], cwd, backend)?,
        "bcs" => cmd_bcs(&tokens[1..], cwd, backend)?,
        "rm" => cmd_rm(&tokens[1..], cwd, backend)?,
        "help" => cmd_help(tokens.get(1).copied()),
        _ => bail!(
            "unknown command '{}' — type 'help' for a list of commands",
            tokens[0]
        ),
    }

    Ok(false)
}

fn cmd_cd(path_arg: Option<&str>, cwd: &mut VfsPath) -> anyhow::Result<()> {
    let target = match path_arg {
        None | Some("/") => VfsPath::Root,
        Some("..") => cwd.parent().unwrap_or(VfsPath::Root),
        Some(p) => resolve_path(cwd, p)?,
    };
    if !target.is_dir() {
        bail!("'{}': not a directory", target);
    }
    *cwd = target;
    Ok(())
}

fn cmd_ls(args: &[&str], cwd: &VfsPath, backend: &Arc<dyn Backend>) -> anyhow::Result<()> {
    let mut limit = DEFAULT_LIMIT;
    let mut path_arg: Option<&str> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i] {
            "--limit" | "-l" => {
                i += 1;
                limit = args
                    .get(i)
                    .ok_or_else(|| anyhow::anyhow!("--limit requires a value"))?
                    .parse()
                    .map_err(|_| anyhow::anyhow!("--limit requires a number"))?;
            }
            arg if arg.starts_with('-') => bail!("unknown flag: {arg}"),
            arg => path_arg = Some(arg),
        }
        i += 1;
    }

    let (target, use_cursor) = match path_arg {
        None | Some(".") => (cwd.clone(), false),
        Some(p) => {
            let resolved = resolve_path(cwd, p)?;
            let cursor = resolved.is_ls_cursor();
            (resolved, cursor)
        }
    };

    let entries = if use_cursor {
        backend.ls_cursor(&target, limit)?
    } else {
        backend.ls_children(&target, limit)?
    };

    for e in &entries {
        let display = if e.is_dir {
            format!("{}/", e.name)
        } else {
            e.name.clone()
        };
        println!("{display}");
    }

    if entries.len() == limit {
        println!("(limit of {limit} reached — use --limit N to show more)");
    }

    Ok(())
}

fn resolve_file_path(args: &[&str], cwd: &VfsPath) -> anyhow::Result<VfsPath> {
    let path_str = args
        .first()
        .ok_or_else(|| anyhow::anyhow!("path required"))?;
    resolve_path(cwd, path_str)
}

fn cmd_cat(args: &[&str], cwd: &VfsPath, backend: &Arc<dyn Backend>) -> anyhow::Result<()> {
    let target = resolve_file_path(args, cwd)?;
    let value = backend.read_json(&target)?;
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

fn cmd_dbg(args: &[&str], cwd: &VfsPath, backend: &Arc<dyn Backend>) -> anyhow::Result<()> {
    let target = resolve_file_path(args, cwd)?;
    let text = backend.read_debug(&target)?;
    println!("{text}");
    Ok(())
}

fn cmd_bcs(args: &[&str], cwd: &VfsPath, backend: &Arc<dyn Backend>) -> anyhow::Result<()> {
    let mut raw = false;
    let mut path_arg: Option<&str> = None;

    for arg in args {
        match *arg {
            "--raw" => raw = true,
            a if a.starts_with('-') => bail!("unknown flag: {a}"),
            a => path_arg = Some(a),
        }
    }

    let target = match path_arg {
        Some(p) => resolve_path(cwd, p)?,
        None => bail!("path required"),
    };

    let bytes = backend.read_bcs(&target)?;

    if raw {
        std::io::stdout()
            .write_all(&bytes)
            .map_err(|e| anyhow::anyhow!("write error: {e}"))?;
    } else {
        println!(
            "{}",
            base64::engine::general_purpose::STANDARD.encode(&bytes)
        );
    }

    Ok(())
}

fn cmd_rm(args: &[&str], cwd: &VfsPath, backend: &Arc<dyn Backend>) -> anyhow::Result<()> {
    let target = resolve_file_path(args, cwd)?;
    print!("Remove '{target}'? This is permanent. [y/N] ");
    std::io::stdout().flush()?;

    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    if answer.trim().eq_ignore_ascii_case("y") {
        backend.delete(&target)?;
        println!("deleted.");
    } else {
        println!("cancelled.");
    }

    Ok(())
}

fn cmd_help(topic: Option<&str>) {
    match topic {
        None => print_help_overview(),
        Some("ls") => print!("{}", HELP_LS),
        Some("cd") => print!("{}", HELP_CD),
        Some("cat") => print!("{}", HELP_CAT),
        Some("dbg") => print!("{}", HELP_DBG),
        Some("bcs") => print!("{}", HELP_BCS),
        Some("rm") => print!("{}", HELP_RM),
        Some("pwd") => println!("pwd\n\n  Print the current working directory."),
        Some(other) => println!("No help for '{other}'. Type 'help' for a command list."),
    }
}

fn print_help_overview() {
    println!(
        r#"
Available commands:

  ls [path] [--limit N]   List directory contents
  cd [path]               Change directory
  cat <path>              Print JSON representation
  dbg <path>              Print Rust debug representation
  bcs [--raw] <path>      Print BCS (base64 by default, raw bytes with --raw)
  rm <path>               Remove an entry (permanent!)
  pwd                     Print current directory
  help [command]          Show this help or command-specific help
  exit | quit             Exit the shell

Virtual filesystem structure:

  /epochs/<epoch>/first-checkpoint         First checkpoint of the epoch
  /epochs/<epoch>/last-checkpoint          Last checkpoint of the epoch
  /epochs/<epoch>/committee                Validator committee for the epoch
  /epochs/<epoch>/checkpoints/<seq>        Individual checkpoint in the epoch

  /checkpoints/seq/<seq>/summary           Checkpoint summary by sequence number
  /checkpoints/seq/<seq>/contents          Checkpoint contents by sequence number
  /checkpoints/seq/<seq>/contents-short    tx/fx digest pairs, one per line
  /checkpoints/digest/<digest>/summary     Checkpoint summary by digest
  /checkpoints/digest/<digest>/contents    Checkpoint contents by digest
  /checkpoints/digest/<digest>/contents-short  tx/fx digest pairs, one per line

  /checkpoint-contents/<digest>            Raw checkpoint contents by contents digest

  /transactions/<txdigest>                 A transaction
  /transactions/<txdigest>.fx-<fxdigest>   Its effects

  /consensus/commits/<index>/summary       Consensus commit summary with transaction keys

Listing behaviour:

  ls /checkpoints/seq             First 30 entries
  ls /checkpoints/seq/1000        30 entries starting at seq 1000
  ls /checkpoints/digest/Abc123   Digests matching that prefix (up to 30)

  ls --limit 100 /epochs          Show up to 100 epochs
"#
    );
}

const HELP_LS: &str = r#"ls [path] [--limit N]

  List the contents of a directory.

  When `path` ends in a sequence number or digest (inside a paginated namespace
  like /checkpoints/seq or /checkpoints/digest), it acts as a start cursor:
  the listing begins there rather than listing that specific checkpoint's children.

  To see the children of a specific checkpoint directory, cd into it first:
    cd /checkpoints/seq/1234
    ls           # shows: summary  contents

Options:
  --limit N    Maximum entries to show (default: 30)

Examples:
  ls                           List current directory
  ls /epochs                   List known epochs
  ls /checkpoints/seq          First 30 sequence-numbered checkpoints
  ls /checkpoints/seq/5000     30 checkpoints from seq 5000
  ls --limit 100 /epochs       Up to 100 epochs
"#;

const HELP_CD: &str = r#"cd [path]

  Change the current working directory.

  Supports absolute paths (/checkpoints/seq/1234), relative paths (../digest),
  and .. to go up one level. 'cd' with no argument returns to root.
"#;

const HELP_CAT: &str = r#"cat <path>

  Print the JSON representation of the entry at <path>.

Examples:
  cat /checkpoints/seq/1234/summary
  cat /epochs/5/last-checkpoint
  cat /epochs/5/committee
"#;

const HELP_DBG: &str = r#"dbg <path>

  Print the Rust debug representation ({:#?}) of the entry at <path>.
  Useful for inspecting raw field values not visible in the JSON view.
"#;

const HELP_BCS: &str = r#"bcs [--raw] <path>

  Print the BCS serialization of the entry at <path>.
  By default, prints base64-encoded bytes.
  With --raw, writes raw binary bytes to stdout.

Examples:
  bcs /checkpoints/seq/1234/summary
  bcs --raw /checkpoints/seq/1234/summary | xxd | head
"#;

const HELP_RM: &str = r#"rm <path>

  Permanently delete the entry at <path>.
  You will be prompted to confirm before deletion occurs.
  THIS IS IRREVERSIBLE. Use only when the node is not running.
"#;
