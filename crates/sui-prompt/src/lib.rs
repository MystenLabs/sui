// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `sui prompt` — the entry point to expert Sui and Move knowledge, shipped as part of the Sui CLI.
//!
//! See `crates/sui-prompt/README.md` for usage with worked examples.
//!
//! ## Architecture
//!
//! - **Skills** and **categories** are embedded at **build time** (see
//!   `crates/sui-prompt/build.rs`). The generated `$OUT_DIR/embedded.rs` defines [`SKILL_FILES`]
//!   (one entry per `.md` file under `src/skills/<bundle>/`) and [`CATEGORIES`]
//!   (one entry per `src/categories/<name>/CATEGORY.md`). Both static slices'
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

/// The `sui prompt` no-args overview rendered to the agent. Source is
/// `prompt-output.md` next to this file, embedded at build time via `include_str!`.
const OVERVIEW: &str = include_str!("prompt-output.md");

/// One markdown file from a skill bundle, embedded in the binary at build time.
pub struct PromptSkillFile {
    /// Skill bundle name (top-level directory under `crates/sui-prompt/src/skills/`).
    pub bundle: &'static str,
    /// Path of the markdown file within the bundle, with `.md` extension.
    /// Example: `"SKILL.md"` or `"access-control.md"`.
    pub file: &'static str,
    /// Verbatim file content embedded via `include_str!`.
    pub content: &'static str,
}

/// One category, embedded in the binary at build time. The `content` is the full
/// `CATEGORY.md` (frontmatter included) — that is what `sui prompt category <name>`
/// prints, consistent with how skills are served.
pub struct PromptCategory {
    /// Category name — comes from the directory under `categories/<name>/`.
    pub name: &'static str,
    /// One-line description, extracted from the `description:` frontmatter line at
    /// build time. Used by `sui prompt categories` for the listing.
    pub description: &'static str,
    /// Full `CATEGORY.md` content (frontmatter + body).
    pub content: &'static str,
    /// Skill bundles this category references, in the order declared in its
    /// `CATEGORY.md` frontmatter `skills:` list. Used by `--list` (deep inventory)
    /// and `--all` (bulk load); note that the body's narrative workflow order may
    /// not match this load order. Populated from frontmatter at build time, and
    /// each entry is validated by `build.rs` to match an embedded bundle — so no
    /// runtime existence check is needed.
    pub skills: &'static [&'static str],
}

/// `sui prompt` — the entry point to expert Sui and Move knowledge, shipped as part of the Sui CLI.
#[derive(Parser)]
#[clap(
    name = "prompt",
    about = "Expert Sui and Move knowledge for AI agents (run `sui prompt` to start)"
)]
pub struct Prompt {
    /// The chosen subcommand; `None` means render the no-args [`OVERVIEW`].
    #[clap(subcommand)]
    cmd: Option<PromptCommand>,
}

/// Subcommands of `sui prompt`. Each variant maps 1:1 to a `print_*` helper below.
#[derive(Subcommand)]
pub enum PromptCommand {
    /// List embedded categories with one-line descriptions.
    Categories,

    /// Read a category's content (workflow, skills, references).
    Category {
        /// Category name (e.g. `audit`). See `sui prompt categories`.
        name: String,

        /// Deep inventory: list every bundle the category names plus each bundle's
        /// reference files. Use this to decide how to load the category's content.
        #[clap(long, conflicts_with = "all")]
        list: bool,

        /// Print every bundle's `SKILL.md` and every reference file in one call.
        /// Does NOT re-print `CATEGORY.md` (the agent has already read it). Each
        /// file is preceded by a `# === FILE: <bundle>/<filename> ===` separator.
        #[clap(long)]
        all: bool,
    },

    /// List bundled skill bundles.
    Skills,

    /// Read a skill bundle's SKILL.md (or a specific reference file).
    Skill {
        /// Skill bundle name (e.g. `sui-move-security-review`).
        bundle: String,

        /// List reference files in the bundle instead of printing content.
        #[clap(long, conflicts_with_all = ["file", "all"])]
        list: bool,

        /// Print a specific reference file in the bundle. The `.md` extension is optional.
        #[clap(long, conflicts_with = "all")]
        file: Option<String>,

        /// Print `SKILL.md` and every reference file in the bundle in one call.
        /// Each file is preceded by a `# === FILE: <filename> ===` separator.
        #[clap(long)]
        all: bool,
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
            Some(PromptCommand::Category { name, list, all }) => {
                if all {
                    print_category_all(&name)
                } else if list {
                    print_category_list(&name)
                } else {
                    print_category(&name)
                }
            }
            Some(PromptCommand::Skills) => {
                print_skills();
                Ok(())
            }
            Some(PromptCommand::Skill {
                bundle,
                list,
                file,
                all,
            }) => print_skill(&bundle, list, file.as_deref(), all),
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
        println!("No skill bundles are embedded in this binary.");
        return;
    }
    println!("Embedded skill bundles ({}):", bundles.len());
    let max_name = bundles.iter().map(|b| b.len()).max().unwrap_or(0);
    for b in &bundles {
        let n = SKILL_FILES.iter().filter(|s| s.bundle == *b).count();
        let label = format!("{} file{}", n, if n == 1 { "" } else { "s" });
        println!("  {:<width$}  — {}", b, label, width = max_name);
    }
    println!();
    // Plain-text command listing — left-aligned commands padded to a common width so
    // the descriptions line up. No `#`/`##` markdown headings; this is CLI output, not
    // a rendered README.
    let commands: &[(&str, &str)] = &[
        (
            "sui prompt skill <bundle> --all",
            "read SKILL.md + every reference file",
        ),
        ("sui prompt skill <bundle>", "read SKILL.md"),
        (
            "sui prompt skill <bundle> --list",
            "list reference file names and sizes (no content)",
        ),
        (
            "sui prompt skill <bundle> --file <r>",
            "read a specific reference file",
        ),
    ];
    let max_cmd = commands.iter().map(|(c, _)| c.len()).max().unwrap_or(0);
    println!("Commands:");
    for (cmd, desc) in commands {
        println!("  {:<width$}  — {}", cmd, desc, width = max_cmd);
    }
}

/// Read from a named skill bundle. Behaviour depends on the flags (clap rejects
/// any pair of `list` / `file` / `all`, so at most one is set):
///
/// - `all = true`: print `SKILL.md` and every reference file, each preceded by a
///   `# === FILE: <filename> ===` separator. Files are ordered with `SKILL.md`
///   first, then the reference files in ASCII alphabetical order.
/// - `list = true`: print the bundle's reference files (alphabetical, `.md` stripped).
/// - `file = Some(name)`: print that reference file's content verbatim. The `.md`
///   extension is added if missing.
/// - Otherwise: print the bundle's `SKILL.md` content verbatim.
///
/// Returns an error if the bundle isn't embedded or the requested reference file
/// doesn't exist in it.
fn print_skill(bundle: &str, list: bool, file: Option<&str>, all: bool) -> anyhow::Result<()> {
    let bundle_exists = SKILL_FILES.iter().any(|s| s.bundle == bundle);
    if !bundle_exists {
        anyhow::bail!(
            "no embedded skill bundle named '{}'. Run `sui prompt skills` to list bundles.",
            bundle
        );
    }
    if all {
        print_bundle_all(bundle, None);
        return Ok(());
    }
    if list {
        // Same order as `--all` (SKILL.md first, then alphabetical) so an agent can
        // map a `--list` entry to its position in `--all` output.
        let files = bundle_files_sorted(bundle);
        let entries: Vec<(&str, usize)> =
            files.iter().map(|&f| (f, file_size(bundle, f))).collect();
        let total: usize = entries.iter().map(|(_, n)| n).sum();
        let max_name = entries
            .iter()
            .map(|(f, _)| f.strip_suffix(".md").unwrap_or(f).len())
            .max()
            .unwrap_or(0);
        println!(
            "Files in skill bundle '{}' ({} files, {} chars):",
            bundle,
            entries.len(),
            total
        );
        for (f, size) in &entries {
            // Display without `.md` extension so `--file <ref>` matches the printed form.
            let display = f.strip_suffix(".md").unwrap_or(f);
            println!("  {:<width$}  {:>7}", display, size, width = max_name);
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
            "no file '{}' in skill bundle '{}'. Run `sui prompt skill {} --list` to enumerate.",
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
        println!("No categories are embedded in this binary.");
        return;
    }
    println!("Embedded categories ({}):", entries.len());
    let max_name = entries.iter().map(|c| c.name.len()).max().unwrap_or(0);
    for c in &entries {
        println!(
            "  {:<width$}  — {}",
            c.name,
            c.description,
            width = max_name
        );
    }
    println!();
    // Plain-text command listing — see `print_skills` for the rationale.
    let commands: &[(&str, &str)] = &[
        (
            "sui prompt category <name> --all",
            "read every bundle's content in one call",
        ),
        ("sui prompt category <name>", "read the category's content"),
        (
            "sui prompt category <name> --list",
            "list bundle and reference file names and sizes (no content)",
        ),
        ("sui prompt skills", "list all skill bundles (flat)"),
        (
            "sui prompt skill <bundle> --all",
            "read SKILL.md + every reference file",
        ),
        (
            "sui prompt skill <bundle>",
            "read a skill bundle's SKILL.md",
        ),
    ];
    let max_cmd = commands.iter().map(|(c, _)| c.len()).max().unwrap_or(0);
    println!("Commands:");
    for (cmd, desc) in commands {
        println!("  {:<width$}  — {}", cmd, desc, width = max_cmd);
    }
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

/// Look up a category by name, returning `&PromptCategory` or the standard
/// `no embedded category named ...` error used by every category-level helper.
fn find_category(name: &str) -> anyhow::Result<&'static PromptCategory> {
    CATEGORIES.iter().find(|c| c.name == name).ok_or_else(|| {
        let valid = CATEGORIES
            .iter()
            .map(|c| c.name)
            .collect::<Vec<_>>()
            .join(", ");
        anyhow::anyhow!("no embedded category named '{name}'. Valid categories: {valid}")
    })
}

/// Return the bundle's files in print order: `SKILL.md` first, then the remaining
/// reference files in ASCII alphabetical order. Shared by both `--all` paths so
/// the file ordering is identical regardless of entry point.
fn bundle_files_sorted(bundle: &str) -> Vec<&'static str> {
    let mut files: Vec<&'static str> = SKILL_FILES
        .iter()
        .filter(|s| s.bundle == bundle)
        .map(|s| s.file)
        .collect();
    files.sort_by(|a, b| match (*a == "SKILL.md", *b == "SKILL.md") {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.cmp(b),
    });
    files
}

/// Size in characters of a single embedded skill file. Reported by `--list` so an
/// agent can decide whether `--all` will fit in its context budget without guessing.
/// Returns 0 if the file isn't embedded (defensive; `build.rs` validates the table).
fn file_size(bundle: &str, file: &str) -> usize {
    SKILL_FILES
        .iter()
        .find(|s| s.bundle == bundle && s.file == file)
        .map(|s| s.content.len())
        .unwrap_or(0)
}

/// Print one bundle's content as a flat separator-per-file stream. With
/// `bundle_prefix = None`, each separator is `# === FILE: <filename> ===` (skill-
/// level `--all`); with `bundle_prefix = Some(bundle)`, each separator is
/// `# === FILE: <bundle>/<filename> ===` (category-level `--all`, where the agent
/// sees multiple bundles in one stream). The `=== FILE: … ===` sentinel cannot be
/// confused with `#` headings inside the file's own content — both humans and
/// agents can locate file boundaries unambiguously by grepping for `=== FILE:`.
fn print_bundle_all(bundle: &str, bundle_prefix: Option<&str>) {
    for file in bundle_files_sorted(bundle) {
        // `# === FILE: ... ===` rather than a bare `# <filename>` so the file boundary
        // can't be mistaken for a `#` heading inside the file's own content. The
        // `=== FILE: … ===` sentinel is greppable and visually distinct from any
        // plausible Markdown title.
        match bundle_prefix {
            Some(prefix) => println!("# === FILE: {prefix}/{file} ==="),
            None => println!("# === FILE: {file} ==="),
        }
        println!();
        let Some(s) = SKILL_FILES
            .iter()
            .find(|s| s.bundle == bundle && s.file == file)
        else {
            // build.rs guarantees the file exists; defensive in case the embedded
            // table ever drifts. Skip silently rather than panicking mid-stream.
            continue;
        };
        print!("{}", s.content);
        // Normalize to a blank line separating the file from the next `#` heading,
        // regardless of whether the source ends with `\n`, `\n\n`, or no newline.
        if !s.content.ends_with("\n\n") {
            if s.content.ends_with('\n') {
                println!();
            } else {
                println!();
                println!();
            }
        }
    }
}

/// Deep inventory: per bundle the category names, list every reference file with
/// its size in characters. Bundles are iterated in CATEGORY.md frontmatter `skills:`
/// order; files within each bundle use the same `SKILL` + alphabetical order as
/// `--all` so an agent can map a `--list` entry to the position it will appear in
/// `--all` output. Per-file size + per-bundle subtotal + category grand total are
/// reported so the agent can decide whether `--all` fits its context budget.
fn print_category_list(name: &str) -> anyhow::Result<()> {
    let c = find_category(name)?;

    // Pre-compute per-bundle file lists + sizes so we can emit the grand total in the
    // header before the per-bundle breakdowns.
    let bundles: Vec<(&str, Vec<(&str, usize)>)> = c
        .skills
        .iter()
        .map(|&bundle| {
            let entries: Vec<(&str, usize)> = bundle_files_sorted(bundle)
                .into_iter()
                .map(|f| (f, file_size(bundle, f)))
                .collect();
            (bundle, entries)
        })
        .collect();
    let grand_total: usize = bundles
        .iter()
        .flat_map(|(_, files)| files.iter().map(|(_, n)| *n))
        .sum();

    println!(
        "Bundles in category '{}' ({} bundles, {} chars):",
        c.name,
        c.skills.len(),
        grand_total
    );
    for (bundle, files) in &bundles {
        let bundle_total: usize = files.iter().map(|(_, n)| *n).sum();
        let max_name = files
            .iter()
            .map(|(f, _)| f.strip_suffix(".md").unwrap_or(f).len())
            .max()
            .unwrap_or(0);
        println!();
        println!(
            "{} ({} files, {} chars):",
            bundle,
            files.len(),
            bundle_total
        );
        for (f, size) in files {
            // `.md` stripped to match the `skill --list` convention, so `--file <ref>`
            // can be invoked with the printed form unchanged.
            let display = f.strip_suffix(".md").unwrap_or(f);
            println!("  {:<width$}  {:>7}", display, size, width = max_name);
        }
    }
    Ok(())
}

/// Print every bundle's content for the named category in one call. Each file is
/// preceded by a `# <bundle>/<filename>` heading so source attribution survives
/// the flat stream. Does NOT prepend `CATEGORY.md` — the agent has already read
/// it via plain `category <name>`.
fn print_category_all(name: &str) -> anyhow::Result<()> {
    let c = find_category(name)?;
    for bundle in c.skills {
        print_bundle_all(bundle, Some(bundle));
    }
    Ok(())
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
        let err = print_skill("__missing_skill_for_prompt_test__", false, None, false)
            .unwrap_err()
            .to_string();
        assert!(err.contains("sui prompt skills"));
    }

    #[test]
    fn missing_category_error() {
        // Unknown categories should include the valid-category hint for a single retry.
        let err = print_category("__missing_category_for_prompt_test__")
            .unwrap_err()
            .to_string();
        assert!(err.contains("Valid categories:"));
    }

    #[test]
    fn category_all_parses() {
        // `--all` at the category level is valid on its own.
        let result = Prompt::try_parse_from(["prompt", "category", "any-name", "--all"]);
        assert!(result.is_ok());
    }

    #[test]
    fn category_list_parses() {
        // `--list` at the category level is valid on its own.
        let result = Prompt::try_parse_from(["prompt", "category", "any-name", "--list"]);
        assert!(result.is_ok());
    }

    #[test]
    fn category_all_list_conflict() {
        // `--all` and `--list` are mutually exclusive at the category level.
        let result = Prompt::try_parse_from(["prompt", "category", "any-name", "--all", "--list"]);
        assert!(result.is_err());
    }

    #[test]
    fn skill_all_parses() {
        // `--all` at the skill level is valid on its own.
        let result = Prompt::try_parse_from(["prompt", "skill", "any-bundle", "--all"]);
        assert!(result.is_ok());
    }

    #[test]
    fn skill_all_file_conflict() {
        // `--all` and `--file` are mutually exclusive at the skill level.
        let result = Prompt::try_parse_from([
            "prompt",
            "skill",
            "any-bundle",
            "--all",
            "--file",
            "any-ref",
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn skill_all_list_conflict() {
        // `--all` and `--list` are mutually exclusive at the skill level.
        let result = Prompt::try_parse_from(["prompt", "skill", "any-bundle", "--all", "--list"]);
        assert!(result.is_err());
    }

    #[test]
    fn category_all_missing_error() {
        // Same recovery hint as `print_category` on an unknown name.
        let err = print_category_all("__missing_category_for_prompt_test__")
            .unwrap_err()
            .to_string();
        assert!(err.contains("Valid categories:"));
    }

    #[test]
    fn category_list_missing_error() {
        let err = print_category_list("__missing_category_for_prompt_test__")
            .unwrap_err()
            .to_string();
        assert!(err.contains("Valid categories:"));
    }

    #[test]
    fn bundle_files_sorted_skill_first() {
        // SKILL.md must lead the order so `--all` always opens with the bundle's index.
        // We pick the first embedded bundle (build.rs guarantees it has SKILL.md).
        if let Some(first) = SKILL_FILES.first() {
            let files = bundle_files_sorted(first.bundle);
            assert_eq!(files.first().copied(), Some("SKILL.md"));
            // Remaining files are alphabetical (already-strict order means a sort is no-op).
            let mut tail = files[1..].to_vec();
            let original_tail = tail.clone();
            tail.sort();
            assert_eq!(original_tail, tail);
        }
    }
}
