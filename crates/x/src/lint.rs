// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use nexlint::{prelude::*, NexLintContext};
use nexlint_lints::{
    content::*,
    handle_lint_results,
    package::*,
    project::{BannedDeps, BannedDepsConfig, DirectDepDups, DirectDepDupsConfig},
};

static LICENSE_HEADER: &str = "Copyright (c) 2022, Mysten Labs, Inc.\n\
                               SPDX-License-Identifier: Apache-2.0\n\
                               ";
#[derive(Debug, Parser)]
pub struct Args {
    #[clap(long)]
    fail_fast: bool,
}

pub fn run(args: Args) -> crate::Result<()> {
    // rand can be removed if/when ed25519_dalek upgrades to rand 0.8
    let direct_dups_config = DirectDepDupsConfig {
        allow: vec!["rand".to_string()],
    };
    let banned_deps_config = BannedDepsConfig {
        direct: vec![
            (
                "lazy_static".to_owned(),
                "use once_cell::sync::Lazy instead".to_owned(),
            ),
            (
                "tracing-test".to_owned(),
                "you should not be testing against log lines".to_owned(),
            ),
        ]
        .into_iter()
        .collect(),
    };
    let project_linters: &[&dyn ProjectLinter] = &[
        &DirectDepDups::new(&direct_dups_config),
        &BannedDeps::new(&banned_deps_config),
    ];

    let package_linters: &[&dyn PackageLinter] = &[
        &CrateNamesPaths,
        &IrrelevantBuildDeps,
        // This one seems to be broken
        //&UnpublishedPackagesOnlyUsePathDependencies::new(),
        &PublishedPackagesDontDependOnUnpublishedPackages,
        &OnlyPublishToCratesIo,
        &CratesInCratesDirectory,
        &CratesOnlyInCratesDirectory,
    ];

    let file_path_linters: &[&dyn FilePathLinter] = &[
        // &AllowedPaths::new(DEFAULT_ALLOWED_PATHS_REGEX)?
        ];

    // allow whitespace exceptions for markdown files
    // let whitespace_exceptions = build_exceptions(&["*.md".to_owned()])?;
    let content_linters: &[&dyn ContentLinter] = &[
        &LicenseHeader::new(LICENSE_HEADER),
        &RootToml,
        // &EofNewline::new(&whitespace_exceptions),
        // &TrailingWhitespace::new(&whitespace_exceptions),
    ];

    let nexlint_context = NexLintContext::from_current_dir()?;
    let engine = LintEngineConfig::new(&nexlint_context)
        .with_project_linters(project_linters)
        .with_package_linters(package_linters)
        .with_file_path_linters(file_path_linters)
        .with_content_linters(content_linters)
        .fail_fast(args.fail_fast)
        .build();

    let results = engine.run()?;

    handle_lint_results(results)
}
