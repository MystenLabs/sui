// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use parking_lot::Mutex;
use rhai::{Dynamic, Engine, Scope};
use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;
use std::path::PathBuf;
use std::sync::Arc;
use tideconsole::engine::{ConsoleContext, create_engine, init_scope_with_db, is_complete};

pub fn run(db: Option<PathBuf>, exec: Option<String>, script: Option<PathBuf>) -> Result<()> {
    let ctx = Arc::new(Mutex::new(ConsoleContext::default()));
    let engine = create_engine(ctx.clone());
    let mut scope = Scope::new();

    if let Some(path) = db {
        init_scope_with_db(&engine, &mut scope, &ctx, &path.display().to_string())?;
    }

    if let Some(code) = exec {
        let result = engine
            .eval_with_scope::<Dynamic>(&mut scope, &code)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        if !result.is_unit() {
            println!("{result}");
        }
        return Ok(());
    }

    if let Some(path) = script {
        let code = std::fs::read_to_string(&path)
            .map_err(|e| anyhow::anyhow!("Failed to read {}: {e}", path.display()))?;
        let result = engine
            .eval_with_scope::<Dynamic>(&mut scope, &code)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        if !result.is_unit() {
            println!("{result}");
        }
        return Ok(());
    }

    repl(&engine, &mut scope)
}

fn repl(engine: &Engine, scope: &mut Scope<'_>) -> Result<()> {
    let mut rl = DefaultEditor::new()?;

    println!("TideConsole — TideHunter Interactive Shell");
    println!("Type help() for available methods, Ctrl+D to exit.");
    println!("Use `let db = open(\"/path/to/db\")` to open a database.\n");

    let mut buf = String::new();

    loop {
        let prompt = if buf.is_empty() { ">> " } else { ".. " };

        match rl.readline(prompt) {
            Ok(line) => {
                if !buf.is_empty() {
                    buf.push('\n');
                }
                buf.push_str(&line);

                if !is_complete(&buf) {
                    continue;
                }

                let input = buf.trim().to_string();
                buf.clear();

                if input.is_empty() {
                    continue;
                }

                match engine.eval_with_scope::<Dynamic>(scope, &input) {
                    Ok(v) if !v.is_unit() => println!("{v}"),
                    Ok(_) => {}
                    Err(e) => eprintln!("Error: {e}"),
                }
            }
            Err(ReadlineError::Interrupted) => {
                if !buf.is_empty() {
                    buf.clear();
                    println!("(input cleared)");
                }
            }
            Err(ReadlineError::Eof) => {
                println!("Goodbye!");
                break;
            }
            Err(e) => {
                eprintln!("Readline error: {e}");
                break;
            }
        }
    }

    Ok(())
}
