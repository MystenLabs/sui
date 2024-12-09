// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use camino::Utf8Path;
use clap::Parser;
use nexlint::{prelude::*, NexLintContext};
use nexlint_lints::{
    content::*,
    package::*,
    project::{
        BannedDepConfig, BannedDepType, BannedDeps, BannedDepsConfig, DirectDepDups,
        DirectDepDupsConfig, DirectDuplicateGitDependencies,
    },
};
static EXTERNAL_CRATE_DIR: &str = "external-crates/";
static CREATE_DAPP_TEMPLATE_DIR: &str = "sdk/create-dapp/templates";
static LICENSE_HEADER: &str = "Copyright (c) Mysten Labs, Inc.\n\
                               SPDX-License-Identifier: Apache-2.0\n\
                               ";
#[derive(Debug, Parser)]
pub struct Args {
    #[clap(long)]
    fail_fast: bool,
}

pub fn run(args: Args) -> crate::Result<()> {
    let banned_deps_config = BannedDepsConfig(
        vec![
            (
                "lazy_static".to_owned(),
                BannedDepConfig {
                    message: "use once_cell::sync::Lazy instead".to_owned(),
                    type_: BannedDepType::Direct,
                },
            ),
            (
                "tracing-test".to_owned(),
                BannedDepConfig {
                    message: "you should not be testing against log lines".to_owned(),
                    type_: BannedDepType::Always,
                },
            ),
            (
                "openssl-sys".to_owned(),
                BannedDepConfig {
                    message: "use rustls for TLS".to_owned(),
                    type_: BannedDepType::Always,
                },
            ),
            (
                "actix-web".to_owned(),
                BannedDepConfig {
                    message: "use axum for a webframework instead".to_owned(),
                    type_: BannedDepType::Always,
                },
            ),
            (
                "warp".to_owned(),
                BannedDepConfig {
                    message: "use axum for a webframework instead".to_owned(),
                    type_: BannedDepType::Always,
                },
            ),
            (
                "pq-sys".to_owned(),
                BannedDepConfig {
                    message: "diesel_async asynchronous database connections instead".to_owned(),
                    type_: BannedDepType::Always,
                },
            ),
        ]
        .into_iter()
        .collect(),
    );

    let direct_dep_dups_config = DirectDepDupsConfig {
        allow: vec![
            // TODO spend the time to de-dup these direct dependencies
            "serde_yaml".to_owned(),
            "syn".to_owned(),
            // Our opentelemetry integration requires that we use the same version of these packages
            // as the opentelemetry crates.
            "prost".to_owned(),
            "tonic".to_owned(),
            // jsonrpsee uses an older version of http-body
            "http-body".to_owned(),
        ],
    };

    let project_linters: &[&dyn ProjectLinter] = &[
        &BannedDeps::new(&banned_deps_config),
        &DirectDepDups::new(&direct_dep_dups_config),
        &DirectDuplicateGitDependencies,
    ];

    let package_linters: &[&dyn PackageLinter] = &[
        &CrateNamesPaths,
        &IrrelevantBuildDeps,
        // This one seems to be broken
        // &UnpublishedPackagesOnlyUsePathDependencies::new(),
        &PublishedPackagesDontDependOnUnpublishedPackages,
        &OnlyPublishToCratesIo,
        &CratesInCratesDirectory,
        // There are crates under consensus/, external-crates/.
        // &CratesOnlyInCratesDirectory,
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

    handle_lint_results_exclude_external_crate_checks(results)
}

/// Define custom handler so we can skip certain lints on certain files. This is a temporary till we upstream this logic
pub fn handle_lint_results_exclude_external_crate_checks(
    results: LintResults,
) -> crate::Result<()> {
    // ignore_funcs is a slice of funcs to execute against lint sources and their path
    // if a func returns true, it means it will be ignored and not throw a lint error
    let ignore_funcs = [
        // legacy ignore checks
        |source: &LintSource, path: &Utf8Path| -> bool {
            (path.starts_with(EXTERNAL_CRATE_DIR)
                || path.starts_with(CREATE_DAPP_TEMPLATE_DIR)
                || path.to_string().contains("/generated/")
                || path.to_string().contains("/proto/"))
                && source.name() == "license-header"
        },
        // ignore check to skip buck related code paths, meta (fb) derived starlark, etc.
        |_source: &LintSource, path: &Utf8Path| -> bool {
            path.starts_with("buck/") || path.starts_with("third-party/")
        },
    ];

    // TODO: handle skipped results
    let mut errs = false;
    for (source, message) in &results.messages {
        if let LintKind::Content(path) = source.kind() {
            if ignore_funcs.iter().any(|func| func(source, path)) {
                continue;
            }
        }
        println!(
            "[{}] [{}] [{}]: {}\n",
            message.level(),
            source.name(),
            source.kind(),
            message.message()
        );
        errs = true;
    }

    if errs {
        Err(anyhow!("there were lint errors"))
    } else {
        Ok(())
    }
}
