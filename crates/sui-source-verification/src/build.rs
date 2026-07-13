// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::process::Command;

use fastcrypto::encoding::{Base64, Encoding};
use move_binary_format::CompiledModule;
use serde::Deserialize;
use sui_types::base_types::ObjectID;

use crate::error::Error;

/// The result of rebuilding the source package with the resolved toolchain binary: the compiled
/// modules (root package, self-address `0x0`) and the storage ids of its linkage dependencies.
pub struct GeneratedPackage {
    pub modules: Vec<CompiledModule>,
    pub dependencies: Vec<ObjectID>,
}

/// The JSON printed by `sui move build --dump-bytecode-as-base64`. `digest` is intentionally
/// ignored — module-by-module comparison localizes mismatches better than a whole-package digest.
#[derive(Deserialize)]
struct DumpOutput {
    modules: Vec<String>,
    dependencies: Vec<ObjectID>,
}

/// Rebuild the package at `source_path` with `binary` and return its compiled modules and linkage
/// dependencies via `--dump-bytecode-as-base64`.
///
/// The flag set is adapted to what `binary` supports (probed from its `--help`): older binaries lack
/// `--no-tree-shaking` (their output is whatever they shake) and the `--build-env` selector.
///
/// `--with-unpublished-dependencies` is never passed. Up to v1.66 it means "emit only the modules
/// still at `0x0`", which is empty for any package that has been published — precisely the packages
/// being verified. From v1.70 it is a no-op unless the package bundles unpublished dependencies, a
/// discouraged feature. Omitting it yields the package's modules on every release.
///
/// The edition and flavor recorded in the publication are deliberately *not* forced. They only apply
/// when the manifest omits them, in which case the original build used this very binary's built-in
/// defaults — which it will use again. Forcing the recorded value instead breaks packages whose
/// record has since drifted: Pyth's `Move.lock` claims edition 2024 while its sources are legacy
/// edition, so passing `--default-move-edition 2024` fails to compile a package that builds fine.
pub fn dump(
    binary: &Path,
    source_path: &Path,
    build_env: &str,
    client_config: Option<&Path>,
) -> Result<GeneratedPackage, Error> {
    let help = help_text(binary);
    if !help.contains("dump-bytecode-as-base64") {
        return Err(Error::BuildSubprocess {
            command: format!("{} move build --help", binary.display()),
            stderr: "this sui version does not support --dump-bytecode-as-base64; \
                     it predates source verification support"
                .to_string(),
        });
    }

    let mut args: Vec<String> = vec![
        "move".into(),
        "build".into(),
        "--dump-bytecode-as-base64".into(),
        "-p".into(),
        source_path.display().to_string(),
    ];

    if help.contains("no-tree-shaking") {
        args.push("--no-tree-shaking".into());
    }
    // The environment selector picks which environment's dependencies to resolve; without it the
    // build cannot decide between e.g. testnet and mainnet. Only releases that support it need it:
    // earlier ones have a single dependency set.
    if help.contains("build-env") {
        args.push("--build-env".into());
        args.push(build_env.to_string());
    }
    let mut command = Command::new(binary);
    command.args(&args);

    // Releases that cannot disable tree-shaking must reach the network during the dump, and they
    // locate the wallet through `SUI_CONFIG_DIR`. Without this the subprocess would consult the
    // user's default configuration rather than the network being verified against.
    if let Some(dir) = client_config.and_then(|path| path.parent()) {
        command.env("SUI_CONFIG_DIR", dir);
    }

    let output = command.output().map_err(|e| Error::BuildSubprocess {
        command: display_command(binary, &args),
        stderr: format!("could not run the build: {e}"),
    })?;

    if !output.status.success() {
        // Build diagnostics are not consistently written to stderr — dependency-resolution errors,
        // for instance, are printed to stdout — so surface both.
        let mut diagnostics = String::from_utf8_lossy(&output.stderr).into_owned();
        diagnostics.push_str(&String::from_utf8_lossy(&output.stdout));
        return Err(Error::BuildSubprocess {
            command: display_command(binary, &args),
            stderr: diagnostics,
        });
    }

    parse_dump(&output.stdout)
}

/// Deserialize and decode the dump JSON into a [`GeneratedPackage`].
fn parse_dump(stdout: &[u8]) -> Result<GeneratedPackage, Error> {
    let parse_err = |message: String| Error::BuildOutputParse { message };

    let dump: DumpOutput = serde_json::from_slice(stdout)
        .map_err(|e| parse_err(format!("invalid build output JSON: {e}")))?;

    let modules = dump
        .modules
        .iter()
        .map(|b64| {
            let bytes = Base64::decode(b64)
                .map_err(|e| parse_err(format!("module is not valid base64: {e}")))?;
            CompiledModule::deserialize_with_defaults(&bytes)
                .map_err(|e| parse_err(format!("could not deserialize module bytecode: {e}")))
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(GeneratedPackage {
        modules,
        dependencies: dump.dependencies,
    })
}

/// Capture `binary move build --help` (stdout and stderr combined) for flag probing.
fn help_text(binary: &Path) -> String {
    let Ok(output) = Command::new(binary)
        .args(["move", "build", "--help"])
        .output()
    else {
        return String::new();
    };
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    text
}

fn display_command(binary: &Path, args: &[String]) -> String {
    format!("{} {}", binary.display(), args.join(" "))
}
