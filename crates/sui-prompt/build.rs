// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::Deserialize;
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
struct CategoryFrontmatter {
    name: String,
    description: String,
    skills: Vec<String>,
}

fn main() {
    // Embed every skill markdown file under `src/skills/` and every category
    // under `src/categories/` into the binary so `sui prompt skill <bundle>` and
    // `sui prompt category <name>` can return their contents at runtime without touching
    // the filesystem. Walks both directories and emits `$OUT_DIR/embedded.rs`: a single
    // generated file holding the `SKILL_FILES` and `CATEGORIES` slices, whose textual
    // content is `include_str!`'d at compile time. The `cargo:rerun-if-changed`
    // directives ensure a rebuild whenever the underlying content changes.
    //
    // Both skills and categories are embedded raw — frontmatter is part of the content
    // an agent reads. Build-time frontmatter parsing exists only to validate the prompt
    // graph and extract category descriptions for `sui prompt categories`.

    let manifest = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set by cargo");
    let skills_dir = PathBuf::from(&manifest).join("src/skills");
    let categories_dir = PathBuf::from(&manifest).join("src/categories");
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set by cargo");
    let dst = PathBuf::from(&out_dir).join("embedded.rs");

    // Rebuild if any skill or category markdown changes (or is added/removed).
    println!("cargo:rerun-if-changed=src/skills");
    println!("cargo:rerun-if-changed=src/categories");
    println!("cargo:rerun-if-changed=build.rs");

    // 1. Collect skills. Walk `src/skills/<bundle>/` and turn every `.md` file
    //    (at any depth) into one `SKILL_FILES` entry. `collect_md` recurses so a bundle
    //    can group its reference files into subdirectories if its author wants. No file
    //    content is read at build time — each file is `include_str!`'d at compile time
    //    by the generated `embedded.rs`.
    let mut skill_entries: Vec<(String, String, PathBuf)> = Vec::new();
    let mut skill_bundles: BTreeSet<String> = BTreeSet::new();
    if skills_dir.exists() {
        for bundle_entry in fs::read_dir(&skills_dir)
            .expect("read src/skills/")
            .filter_map(Result::ok)
        {
            let bundle_path = bundle_entry.path();
            if !bundle_path.is_dir() {
                continue;
            }
            let bundle = bundle_path
                .file_name()
                .and_then(|s| s.to_str())
                .expect("bundle name is valid UTF-8")
                .to_owned();
            let skill_md = bundle_path.join("SKILL.md");
            if !skill_md.is_file() {
                panic!(
                    "skill bundle '{}' is missing required {}",
                    bundle,
                    skill_md.display()
                );
            }
            skill_bundles.insert(bundle.clone());
            collect_md(&bundle, &bundle_path, &bundle_path, &mut skill_entries);
        }
    }

    // 2. Collect categories. Each `src/categories/<name>/CATEGORY.md` becomes one
    //    `CATEGORIES` entry. The category `name` is taken from the directory; the
    //    frontmatter is parsed once to validate its name, description, and skill bundle
    //    references. Everything else in `CATEGORY.md` — the whole file, frontmatter
    //    included — is embedded raw and served verbatim by `sui prompt category <name>`,
    //    the same convention as skills.
    let mut category_entries: Vec<(String, String, PathBuf)> = Vec::new();
    if categories_dir.exists() {
        for entry in fs::read_dir(&categories_dir)
            .expect("read src/categories/")
            .filter_map(Result::ok)
        {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            let name = dir
                .file_name()
                .and_then(|s| s.to_str())
                .expect("category dir name is valid UTF-8")
                .to_owned();
            let category_md = dir.join("CATEGORY.md");
            if !category_md.is_file() {
                panic!(
                    "category directory '{}' is missing required CATEGORY.md",
                    dir.display()
                );
            }
            let frontmatter = read_category_frontmatter(&category_md);
            if frontmatter.name != name {
                panic!(
                    "{} has frontmatter name '{}' but directory name is '{}'",
                    category_md.display(),
                    frontmatter.name,
                    name
                );
            }
            if frontmatter.description.trim().is_empty() {
                panic!(
                    "{} has an empty `description` in its frontmatter",
                    category_md.display()
                );
            }
            if frontmatter.skills.is_empty() {
                panic!(
                    "{} must list at least one skill bundle in frontmatter `skills`",
                    category_md.display()
                );
            }
            for skill in &frontmatter.skills {
                if !skill_bundles.contains(skill) {
                    panic!(
                        "{} references unknown skill bundle '{}'",
                        category_md.display(),
                        skill
                    );
                }
            }
            category_entries.push((name, frontmatter.description, category_md));
        }
    }

    // 3. Emit `embedded.rs` with the two static slices, side by side. Both use
    //    `include_str!(r"<abs path>")` so the actual file content is pulled in by rustc
    //    at compile time — `build.rs` never sees the bytes itself for skills, and only
    //    sees category bytes once for frontmatter validation.
    let mut src = String::new();
    src.push_str("// Auto-generated by build.rs. Do not edit by hand.\n\n");

    src.push_str("pub static SKILL_FILES: &[PromptSkillFile] = &[\n");
    for (bundle, file, abs) in &skill_entries {
        src.push_str(&format!(
            "    PromptSkillFile {{ bundle: \"{}\", file: \"{}\", content: include_str!(r\"{}\") }},\n",
            escape(bundle),
            escape(file),
            abs.display()
        ));
    }
    src.push_str("];\n\n");

    src.push_str("pub static CATEGORIES: &[PromptCategory] = &[\n");
    for (name, description, abs) in &category_entries {
        src.push_str(&format!(
            "    PromptCategory {{ name: \"{}\", description: \"{}\", content: include_str!(r\"{}\") }},\n",
            escape(name),
            escape(description),
            abs.display()
        ));
    }
    src.push_str("];\n");

    fs::write(&dst, src).expect("write embedded.rs");
}

/// Recursively collect every `.md` file under `dir` into `out` as
/// `(bundle, relative-path-within-bundle, absolute-path)` triples. `root` is the bundle's
/// top-level directory, used to compute the relative path that the runtime sees as
/// `PromptSkillFile.file`.
fn collect_md(bundle: &str, root: &Path, dir: &Path, out: &mut Vec<(String, String, PathBuf)>) {
    for e in fs::read_dir(dir)
        .expect("read skill bundle directory")
        .filter_map(Result::ok)
    {
        let p = e.path();
        if p.is_dir() {
            collect_md(bundle, root, &p, out);
        } else if p.extension().and_then(|s| s.to_str()) == Some("md") {
            let rel = p
                .strip_prefix(root)
                .expect("path under bundle root")
                .to_string_lossy()
                .into_owned();
            out.push((bundle.to_owned(), rel, p));
        }
    }
}

/// Parse `CATEGORY.md` frontmatter. YAML parsing is delegated to `serde_yaml`; the only
/// manual work here is finding the conventional leading `---` frontmatter block.
fn read_category_frontmatter(path: &Path) -> CategoryFrontmatter {
    let content = fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
    let yaml = frontmatter_block(&content)
        .unwrap_or_else(|| panic!("{} is missing leading YAML frontmatter", path.display()));
    serde_yaml::from_str(yaml)
        .unwrap_or_else(|err| panic!("failed to parse frontmatter in {}: {err}", path.display()))
}

/// Return the leading YAML frontmatter block, excluding the `---` delimiters.
fn frontmatter_block(content: &str) -> Option<&str> {
    let mut lines = content.split_inclusive('\n');
    let first = lines.next()?;
    if first.trim_end_matches(['\r', '\n']) != "---" {
        return None;
    }
    let start = first.len();
    let mut offset = start;
    for line in lines {
        if line.trim_end_matches(['\r', '\n']) == "---" {
            return Some(&content[start..offset]);
        }
        offset += line.len();
    }
    None
}

/// Escape a string for embedding inside a Rust double-quoted string literal in the
/// generated `embedded.rs`.
fn escape(s: &str) -> String {
    s.escape_default().to_string()
}
