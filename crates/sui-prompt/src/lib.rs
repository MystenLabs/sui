// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! `move prompt` — entry point to expert Move knowledge (agent-agnostic CLI).
//!
//! See `move-cli/src/prompt/README.md` for usage with worked examples.
//!
//! ## Architecture
//!
//! - **Skills** and **categories** are embedded at **build time** (see
//!   `move-cli/build.rs`). The generated `$OUT_DIR/embedded.rs` defines [`SKILL_FILES`]
//!   (one entry per `.md` file under `src/prompt/skills/<bundle>/`) and [`CATEGORIES`]
//!   (one entry per `src/prompt/categories/<name>/CATEGORY.md`). Both static slices'
//!   text is `include_str!`'d at compile time; no runtime filesystem access.
//! - The CLI surface routes between three concerns: the no-args overview
//!   (`prompt-output.md`), skill access (flat — `skills` / `skill <name>`), and
//!   category access (`categories` / `category <name>`). Categories reference skills by
//!   name; a skill can appear in multiple categories trivially.

use clap::{Parser, Subcommand};
use std::collections::BTreeSet;

// build.rs generates this file with `SKILL_FILES: &[PromptSkillFile]` and
// `CATEGORIES: &[PromptCategory]`.
include!(concat!(env!("OUT_DIR"), "/embedded.rs"));

/// The `move prompt` no-args overview rendered to the agent. Source is
/// `prompt-output.md` next to this file, embedded at build time via `include_str!`.
const OVERVIEW: &str = include_str!("prompt-output.md");

/// One markdown file from a skill bundle, embedded in the binary at build time.
pub struct PromptSkillFile {
    /// Skill bundle name (top-level directory under `move-cli/src/prompt/skills/`).
    pub bundle: &'static str,
    /// Path of the markdown file within the bundle, with `.md` extension.
    /// Example: `"SKILL.md"` or `"access-control.md"`.
    pub file: &'static str,
    /// Verbatim file content embedded via `include_str!`.
    pub content: &'static str,
}

/// One category, embedded in the binary at build time. The `content` is the full
/// `CATEGORY.md` (frontmatter included) — that is what `move prompt category <name>`
/// prints, consistent with how skills are served.
pub struct PromptCategory {
    /// Category name — comes from the directory under `categories/<name>/`.
    pub name: &'static str,
    /// One-line description, extracted from the `description:` frontmatter line at
    /// build time. Used by `move prompt categories` for the listing.
    pub description: &'static str,
    /// Full `CATEGORY.md` content (frontmatter + body).
    pub content: &'static str,
}

/// `move prompt` — entry point to expert Move knowledge (agent-agnostic CLI).
#[derive(Parser)]
#[clap(
    name = "prompt",
    about = "Entry point to expert Move knowledge (agent-agnostic). \
             Call with no subcommand to print the discoverability overview."
)]
pub struct Prompt {
    /// The chosen subcommand; `None` means render the no-args [`OVERVIEW`].
    #[clap(subcommand)]
    cmd: Option<PromptCommand>,
}

/// Subcommands of `move prompt`. Each variant maps 1:1 to a `print_*` helper below.
#[derive(Subcommand)]
pub enum PromptCommand {
    /// List embedded categories with one-line descriptions.
    Categories,

    /// Read a category's content (workflow, skills, references).
    Category {
        /// Category name (e.g. `audit`). See `move prompt categories`.
        name: String,
    },

    /// List bundled skill bundles.
    Skills,

    /// Read a skill bundle's SKILL.md (or a specific reference file).
    Skill {
        /// Skill bundle name (e.g. `sui-move-security-review`).
        bundle: String,

        /// List reference files in the bundle instead of printing content.
        #[clap(long, conflicts_with = "file")]
        list: bool,

        /// Print a specific reference file in the bundle. The `.md` extension is optional.
        #[clap(long)]
        file: Option<String>,
    },
}

impl Prompt {
    /// Dispatch the parsed subcommand. Prints to stdout; never writes to the filesystem.
    /// Returns an error only for user-input failures (unknown bundle / unknown reference
    /// file); successful renders return `Ok(())`.
    pub fn execute(self) -> anyhow::Result<()> {
        match self.cmd {
            None => {
                print!("{}", OVERVIEW);
                Ok(())
            }
            Some(PromptCommand::Categories) => {
                print_categories();
                Ok(())
            }
            Some(PromptCommand::Category { name }) => print_category(&name),
            Some(PromptCommand::Skills) => {
                print_skills();
                Ok(())
            }
            Some(PromptCommand::Skill { bundle, list, file }) => {
                print_skill(&bundle, list, file.as_deref())
            }
        }
    }
}

/// Dispatch `sui prompt` (called from `crates/sui/src/sui_commands.rs`). Mirrors the
/// shape of `sui_move::execute_move_command` for consistency with the `sui move`
/// delegation pattern.
pub fn execute_prompt_command(prompt: Prompt) -> anyhow::Result<()> {
    prompt.execute()
}

/// Print every embedded skill bundle's name and file count to stdout, alphabetically.
/// Bundle ordering is derived from a `BTreeSet`, so it is stable regardless of the order
/// in which [`SKILL_FILES`] entries were generated by `build.rs`.
fn print_skills() {
    let bundles: BTreeSet<&str> = SKILL_FILES.iter().map(|s| s.bundle).collect();
    if bundles.is_empty() {
        println!("# Embedded skill bundles");
        println!();
        println!("No skill bundles are embedded in this binary.");
        return;
    }
    println!("# Embedded skill bundles ({})", bundles.len());
    println!();
    for b in &bundles {
        let n = SKILL_FILES.iter().filter(|s| s.bundle == *b).count();
        println!("- `{}` — {} file{}", b, n, if n == 1 { "" } else { "s" });
    }
    println!();
    println!("## Commands");
    println!();
    println!("- `move prompt skill <bundle>` — read `SKILL.md`");
    println!("- `move prompt skill <bundle> --list` — list reference files");
    println!("- `move prompt skill <bundle> --file <r>` — read a specific reference file");
}

/// Read from a named skill bundle. Behaviour depends on the flags:
///
/// - `list = true`: print the bundle's reference files (alphabetical, `.md` stripped).
/// - `file = Some(name)`: print that reference file's content verbatim. The `.md`
///   extension is added if missing.
/// - Otherwise: print the bundle's `SKILL.md` content verbatim.
///
/// Returns an error if the bundle isn't embedded or the requested reference file
/// doesn't exist in it.
fn print_skill(bundle: &str, list: bool, file: Option<&str>) -> anyhow::Result<()> {
    let bundle_exists = SKILL_FILES.iter().any(|s| s.bundle == bundle);
    if !bundle_exists {
        anyhow::bail!(
            "no embedded skill bundle named '{}'. Run `move prompt skills` to list bundles.",
            bundle
        );
    }
    if list {
        let mut files: Vec<&str> = SKILL_FILES
            .iter()
            .filter(|s| s.bundle == bundle)
            .map(|s| s.file)
            .collect();
        files.sort();
        println!("Files in skill bundle '{}' ({}):", bundle, files.len());
        for f in files {
            // Display without `.md` extension so `--file <ref>` matches the printed form.
            let display = f.strip_suffix(".md").unwrap_or(f);
            println!("  {}", display);
        }
        return Ok(());
    }
    let target_file = match file {
        Some(name) => normalize_skill_file(name),
        None => "SKILL.md".to_string(),
    };
    match SKILL_FILES
        .iter()
        .find(|s| s.bundle == bundle && s.file == target_file)
    {
        Some(s) => {
            print!("{}", s.content);
            Ok(())
        }
        None => anyhow::bail!(
            "no file '{}' in skill bundle '{}'. Run `move prompt skill {} --list` to enumerate.",
            target_file,
            bundle,
            bundle
        ),
    }
}

/// Normalize the `--file` value to the embedded markdown filename. The list command
/// displays references without `.md`, so direct reads accept either form.
fn normalize_skill_file(name: &str) -> String {
    if name.ends_with(".md") {
        name.to_string()
    } else {
        format!("{name}.md")
    }
}

/// Print every embedded category's name + description to stdout, alphabetically.
/// Categories are sorted at print time, so the order of entries in [`CATEGORIES`] does
/// not affect what the agent sees.
fn print_categories() {
    let mut entries: Vec<&PromptCategory> = CATEGORIES.iter().collect();
    entries.sort_by_key(|c| c.name);
    if entries.is_empty() {
        println!("# Embedded categories");
        println!();
        println!("No categories are embedded in this binary.");
        return;
    }
    println!("# Embedded categories ({})", entries.len());
    println!();
    for c in &entries {
        println!("- `{}` — {}", c.name, c.description);
    }
    println!();
    println!("## Commands");
    println!();
    println!("- `move prompt category <name>` — read the category's content");
    println!("- `move prompt skills` — list all skill bundles (flat)");
    println!("- `move prompt skill <bundle>` — read a skill bundle's `SKILL.md`");
}

/// Print the named category's content (frontmatter + body) verbatim — the same convention
/// as skills. Returns an error if the category isn't embedded; the error message includes
/// the valid category names so an agent can recover with a single retry.
fn print_category(name: &str) -> anyhow::Result<()> {
    match CATEGORIES.iter().find(|c| c.name == name) {
        Some(c) => {
            print!("{}", c.content);
            Ok(())
        }
        None => {
            let valid = CATEGORIES
                .iter()
                .map(|c| c.name)
                .collect::<Vec<_>>()
                .join(", ");
            anyhow::bail!("no embedded category named '{name}'. Valid categories: {valid}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_adds_md() {
        // `--list` prints refs without `.md`; direct reads accept that printed form.
        assert_eq!(normalize_skill_file("access-control"), "access-control.md");
    }

    #[test]
    fn normalize_keeps_md() {
        // Passing the full embedded filename should not append a duplicate extension.
        assert_eq!(
            normalize_skill_file("access-control.md"),
            "access-control.md"
        );
    }

    #[test]
    fn normalize_nested_path() {
        // build.rs supports nested skill references; normalization should preserve paths.
        assert_eq!(normalize_skill_file("refs/foo"), "refs/foo.md");
        assert_eq!(normalize_skill_file("refs/foo.md"), "refs/foo.md");
    }

    #[test]
    fn list_file_conflict() {
        // Clap should reject ambiguous intent before execution, independent of bundle content.
        let result = Prompt::try_parse_from([
            "prompt",
            "skill",
            "any-bundle",
            "--list",
            "--file",
            "any-ref",
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn file_parses() {
        // A lone `--file` flag is valid; only combining it with `--list` is rejected.
        let result = Prompt::try_parse_from(["prompt", "skill", "any-bundle", "--file", "any-ref"]);
        assert!(result.is_ok());
    }

    #[test]
    fn list_parses() {
        // A lone `--list` flag is valid; only combining it with `--file` is rejected.
        let result = Prompt::try_parse_from(["prompt", "skill", "any-bundle", "--list"]);
        assert!(result.is_ok());
    }

    #[test]
    fn missing_skill_error() {
        // Unknown bundles should tell agents how to recover without guessing command syntax.
        let err = print_skill("__missing_skill_for_prompt_test__", false, None)
            .unwrap_err()
            .to_string();
        assert!(err.contains("move prompt skills"));
    }

    #[test]
    fn missing_category_error() {
        // Unknown categories should include the valid-category hint for a single retry.
        let err = print_category("__missing_category_for_prompt_test__")
            .unwrap_err()
            .to_string();
        assert!(err.contains("Valid categories:"));
    }
}
