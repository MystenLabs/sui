// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::bail;
use move_command_line_common::testing::insta_assert;

use codespan_reporting::{
    files::SimpleFiles,
    term::{self, Config, termcolor::Buffer},
};
use move_package_alt::{
    dependency::{self, DependencySet, UnpinnedDependencyInfo},
    flavor::Vanilla,
    package::{lockfile::Lockfile, manifest::Manifest},
};
use std::path::Path;
use tracing_subscriber::EnvFilter;

/// Resolve the package contained in the same directory as [path], and snapshot a value based
/// on the extension of [path]:
///  - ".parsed": the contents of the manifest
///  - ".locked": the contents of the lockfile
///  - ".pinned": the contents of the pinned dependencies
pub fn run_test(path: &Path) -> datatest_stable::Result<()> {
    let _ = tracing_subscriber::fmt::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .without_time()
        .with_target(false)
        .try_init();

    if path.iter().any(|part| part == "deps_only") {
        return Ok(());
    }

    let kind = path.extension().unwrap().to_string_lossy();
    let toml_path = path.with_extension("toml");
    let test = Test::from_path_with_kind(&toml_path, &kind)?;
    test.run()
}

#[derive(Debug)]
struct Test<'a> {
    kind: &'a str,
    toml_path: &'a Path,
}

impl Test<'_> {
    fn from_path_with_kind<'a>(
        toml_path: &'a Path,
        kind: &'a str,
    ) -> datatest_stable::Result<Test<'a>> {
        dbg!(&toml_path);
        Ok(Test { toml_path, kind })
    }

    fn run(&self) -> datatest_stable::Result<()> {
        let output = self.output().unwrap_or_else(|err| format!("{:#}\n", err));
        insta_assert! {
            input_path: self.toml_path,
            contents: output,
            suffix: self.kind,
        };

        Ok(())
    }

    /// Return the value to be snapshotted, based on `self.kind`, as described in [run_test]
    fn output(&self) -> anyhow::Result<String> {
        Ok(match self.kind {
            "parsed" => {
                let manifest = Manifest::<Vanilla>::read_from_file(self.toml_path);
                let contents = match manifest.as_ref() {
                    Ok(m) => format!("{:#?}", m),
                    Err(_) => {
                        let mut mapped_files = SimpleFiles::new();
                        mapped_files.add(
                            self.toml_path.to_str().unwrap(),
                            std::fs::read_to_string(self.toml_path).unwrap(),
                        );

                        if let Some(e) = manifest.as_ref().err() {
                            let diagnostic = e.to_diagnostic();
                            let mut writer = Buffer::no_color();
                            term::emit(&mut writer, &Config::default(), &mapped_files, &diagnostic)
                                .unwrap();
                            let inner = writer.into_inner();
                            String::from_utf8(inner).unwrap_or_default()
                        } else {
                            format!("{}", manifest.unwrap_err())
                        }
                    }
                };
                contents
            }
            "locked" => {
                let lockfile = Lockfile::<Vanilla>::read_from_dir(self.toml_path.parent().unwrap());
                match lockfile {
                    Ok(l) => l.render_as_toml().to_string(),
                    Err(e) => e.to_string(),
                }
            }
            "pinned" => run_pinning_wrapper(self.toml_path).unwrap(),
            ext => bail!("Unrecognised snapshot type: '{ext}'"),
        })
    }
}

async fn run_pinning_tests(input_path: &Path) -> datatest_stable::Result<String> {
    let manifest = Manifest::<Vanilla>::read_from_file(input_path).unwrap();

    let deps: DependencySet<UnpinnedDependencyInfo<Vanilla>> = manifest.dependencies();

    add_bindir();
    let pinned = dependency::pin(&Vanilla, deps, manifest.environments()).await;

    let output = match pinned {
        Ok(ref deps) => format!("{deps:?}"),
        Err(ref err) => err.to_string(),
    };

    Ok(output)
}

fn run_pinning_wrapper(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    let data = rt.block_on(run_pinning_tests(path))?;
    Ok(data)
}

/// Ensure that the directory containing mock-resolver is on the PATH
fn add_bindir() {
    let bindir = Path::new(std::env!("CARGO_BIN_EXE_mock-resolver"))
        .parent()
        .unwrap()
        .to_string_lossy();

    // TODO: replace this with different logic
    // SAFETY: this is safe because it's run under cargo nextest run. See:
    // `https://nexte.st/docs/configuration/env-vars/`
    unsafe {
        std::env::set_var(
            "PATH",
            format!("{}:{}", std::env::var("PATH").unwrap(), bindir),
        );
    }
}

datatest_stable::harness!(
    run_test,
    "tests/data",
    r".*\.parsed$",
    run_test,
    "tests/data",
    r".*\.locked$",
    run_test,
    "tests/data",
    r".*\.pinned$",
);
