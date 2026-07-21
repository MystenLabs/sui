// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Version matrix: check that a package published with a historical `sui` binary verifies with the
//! current CLI.
//!
//! For each version: download that version's binary (via [`ensure_binary`]), publish a self-contained
//! fixture with it to a current-protocol localnet, then run the current `verify-source` against the
//! package directory — expecting success, and expecting failure once the source is tampered with. The
//! address and toolchain to verify against come from the metadata the publish wrote, so nothing but
//! the package path is passed.
//!
//! Modern vs legacy environments. A modern package (>= v1.63) declares `localnet` in its manifest, so
//! the localnet is a normal named environment and the whole flow — address lookup, `--build-env`, and
//! the on-chain fetch — resolves through the CLI under that one env. A legacy package cannot do this:
//! its only build environments are `testnet`/`mainnet`, pinned to their real chain ids, which never
//! match a test cluster's fresh chain id, so the CLI's `find_environment` can never place it on the
//! cluster. That is correct behaviour (verifying a package "as" an environment requires a named build
//! environment whose chain id matches the network being fetched from), not a bug to work around.
//!
//! Legacy packages therefore take a harness-only path: [`verify_legacy`] drives the library
//! `verify_source` in-process instead of the CLI, splitting the two environments the CLI deliberately
//! keeps unified —
//!   - `read_publication(localnet_env)` supplies the address (legacy publications are keyed by env
//!     *name*, so the localnet-published address is found under `"localnet"`), and the cluster is the
//!     fetch `client`;
//!   - `build_env = testnet` is passed for `--build-env`, a free choice here: legacy dependency
//!     resolution is environment-independent, and this fixture is framework-free, so the rebuild is
//!     identical under any build environment (no linkage entries whose storage ids could differ by
//!     network). `verify_source` already takes `publication` and `build_env` as independent
//!     arguments, so the seam exists; only the harness changes. This split stays test-only — a
//!     build-env-≠-verification-env knob in the CLI would let a user mis-verify a package.
//!
//! These tests are `#[ignore]`d: they download release binaries (network) and are exploratory (they
//! surface how well an old client interoperates with a current localnet). Run manually, and build
//! the current CLI first so `get_cargo_bin("sui")` resolves:
//!
//! ```text
//! cargo build -p sui
//! cargo test -p sui-source-verification --test version_matrix -- --ignored --nocapture
//! ```
//!
//! [`era_matrix`] asserts on a small curated set. [`historical_sweep`] reports a table over whatever
//! versions are passed via `SUI_MATRIX_VERSIONS`; it does not assert.

use std::path::{Path, PathBuf};
use std::process::Command;

use fs_extra::dir::CopyOptions;
use insta_cmd::get_cargo_bin;
use move_package_alt::read_publication;
use move_package_alt::schema::Environment;
use sui_config::SUI_CLIENT_CONFIG;
use sui_package_alt::SuiFlavor;
use sui_sdk::wallet_context::WalletContext;
use sui_source_verification::{ensure_binary, verify_source};
use test_cluster::TestClusterBuilder;

/// Curated versions for the nightly matrix, spanning both package-system eras. v1.63 is where
/// `Published.toml` was introduced; v1.23 to v1.62 use the legacy `Move.lock` workflow.
///
/// v1.23 is the floor for metadata-only verification: it is the earliest release whose `publish`
/// records a publication the package system can read back (an address keyed by environment name).
/// Earlier releases publish without recording a resolvable address — confirmed empirically, every
/// release from v1.10 through v1.22 leaves `read_publication` with nothing to find — so there is no
/// metadata to verify against, independent of the client version.
///
/// A full sweep of every mainnet release found further ranges to avoid. v1.8.1 and below ship no
/// binary for this platform. v1.25.1 to v1.29.2 cannot deserialise a current node's protocol config
/// (`unknown variant `bool`, expected one of `u16`, `u32`, `u64`, `f64``) and so cannot transact at
/// all. v1.64.1 is [`KNOWN_UNBUILDABLE`]. The curated set samples across the eras between those gaps.
const CURATED_VERSIONS: &[&str] = &["1.23.1", "1.46.3", "1.62.1", "1.63.3", "1.74.1"];

/// Releases that cannot rebuild any package depending on the framework, because the framework
/// revision they pin is not available from the sui repository (`upload-pack: not our ref`).
/// `crates/sui-framework-snapshot/manifest.json` records the revision, but the commit is not in the
/// public repo — protocol 108 for v1.64.x, and protocols 8/16 for early releases.
///
/// The matrix fixture is framework-free, so these versions would pass here while being unusable for
/// real packages. They are excluded rather than given false confidence. In practice a user hitting
/// this can pass `--toolchain-version` to rebuild with an adjacent release, since compiler output
/// rarely changes between them.
const KNOWN_UNBUILDABLE: &[&str] = &["1.64.1"];

/// Fixture for releases with the current package system (>= v1.63): `implicit-dependencies = false`
/// keeps the build from fetching the framework it does not use.
const FIXTURE: &str = "tests/version_fixtures/self_contained";

/// Fixture for legacy releases (< v1.63), whose manifests predate the `implicit-dependencies` key.
const LEGACY_FIXTURE: &str = "tests/version_fixtures/legacy";

#[derive(Debug)]
struct Outcome {
    version: String,
    downloaded: bool,
    published: bool,
    verified: bool,
    tamper_detected: bool,
    notes: String,
}

impl Outcome {
    fn passed(&self) -> bool {
        self.downloaded && self.published && self.verified && self.tamper_detected
    }
}

#[tokio::test]
#[ignore = "downloads historical sui release binaries and requires network; run manually"]
async fn era_matrix() {
    let mut versions: Vec<String> = CURATED_VERSIONS.iter().map(|s| s.to_string()).collect();

    // The nightly workflow passes the latest release here so that a new release which changes the
    // archive layout or breaks interoperability is caught, which a fixed curated set never would.
    if let Ok(extra) = std::env::var("SUI_MATRIX_EXTRA_VERSIONS") {
        versions.extend(
            extra
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from),
        );
    }

    let outcomes = run_matrix(&versions).await;
    print_table(&outcomes);
    let failed: Vec<_> = outcomes.iter().filter(|o| !o.passed()).collect();
    assert!(failed.is_empty(), "curated versions failed: {failed:#?}");
}

#[tokio::test]
#[ignore = "downloads historical sui release binaries and requires network; run manually"]
async fn historical_sweep() {
    // Fail rather than fall back to the curated set: silently sweeping only some versions when the
    // list is misconfigured would hide gaps in what was actually tested.
    let list = std::env::var("SUI_MATRIX_VERSIONS")
        .expect("set SUI_MATRIX_VERSIONS to a comma-separated list of versions to sweep");
    let versions: Vec<String> = list.split(',').map(|s| s.trim().to_string()).collect();
    let outcomes = run_matrix(&versions).await;
    print_table(&outcomes);
}

/// Build one localnet and run every version's fixture through it, returning an outcome per version.
async fn run_matrix(versions: &[String]) -> Vec<Outcome> {
    let cluster = TestClusterBuilder::new().build().await;
    let config = cluster.swarm.dir().join(SUI_CLIENT_CONFIG);
    let current_sui = get_cargo_bin("sui");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    let Some(chain_id) = chain_identifier(&current_sui, &config) else {
        panic!("could not read the localnet chain identifier");
    };

    let mut outcomes = Vec::new();
    for version in versions {
        outcomes.push(run_one(version, &root, &config, &current_sui, &chain_id).await);
    }
    outcomes
}

/// The chain identifier of the network `config` points at.
fn chain_identifier(sui: &Path, config: &Path) -> Option<String> {
    let out = Command::new(sui)
        .args(["client", "--client.config"])
        .arg(config)
        .arg("chain-identifier")
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Declare `localnet` in the fixture's manifest so it can be published to for real.
fn declare_localnet(sandbox: &Path, chain_id: &str) -> std::io::Result<()> {
    let manifest = sandbox.join("Move.toml");
    let contents = std::fs::read_to_string(&manifest)?;
    std::fs::write(
        manifest,
        format!("{contents}\n[environments]\nlocalnet = \"{chain_id}\"\n"),
    )
}

/// Whether `binary` uses the current package system (>= v1.63), detected by the presence of the
/// `test-publish` command. Such releases record publications in `Published.toml` and declare
/// environments in the manifest; older ones use the legacy `Move.lock` workflow.
fn uses_current_package_system(binary: &Path) -> bool {
    let Ok(out) = Command::new(binary).args(["client", "--help"]).output() else {
        return false;
    };
    let text =
        String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    text.contains("test-publish")
}

/// Stage a writable copy of `fixture` in a fresh temp dir, declaring the `localnet` environment for
/// current-package-system packages. Publishing writes lock/publish metadata and the source is later
/// tampered, so the fixture cannot be built in place. Returns the temp dir (which the caller must keep
/// alive) and the staged package path, or a note describing what went wrong.
fn stage_fixture(
    fixture: &Path,
    chain_id: &str,
    modern: bool,
) -> Result<(tempfile::TempDir, PathBuf), String> {
    let tmp = tempfile::tempdir().map_err(|_| "could not create temp dir".to_string())?;
    let sandbox = tmp.path().join("pkg");
    std::fs::create_dir_all(&sandbox)
        .and_then(|()| copy_dir(fixture, &sandbox).map_err(std::io::Error::other))
        .map_err(|e| format!("failed to stage fixture: {e}"))?;

    // Current-package-system releases publish to an environment the manifest declares (with a fresh
    // per-run chain id); legacy releases publish to the active environment directly.
    if modern {
        declare_localnet(&sandbox, chain_id)
            .map_err(|e| format!("could not declare localnet: {e}"))?;
    }
    Ok((tmp, sandbox))
}

/// Publish the staged package at `sandbox` with the old `binary`. This records the address and
/// toolchain that verify-source reads; any failure is the "old client -> current localnet" signal.
fn publish_fixture(
    binary: &Path,
    config: &Path,
    sandbox: &Path,
    modern: bool,
) -> Result<(), String> {
    let mut publish = Command::new(binary);
    publish
        .args(["client", "--client.config"])
        .arg(config)
        .args(["publish", "."]);
    if !modern {
        // Legacy releases require an explicit budget.
        publish.args(["--gas-budget", "500000000"]);
    }
    match publish.current_dir(sandbox).output() {
        Ok(out) if out.status.success() => Ok(()),
        Ok(out) => Err(format!("publish failed: {}", diagnostics(&out))),
        Err(e) => Err(format!("could not run publish: {e}")),
    }
}

/// Publish the fixture with the `version` binary and verify it with `current_sui`.
async fn run_one(
    version: &str,
    root: &Path,
    config: &Path,
    current_sui: &Path,
    chain_id: &str,
) -> Outcome {
    let mut outcome = Outcome {
        version: version.to_string(),
        downloaded: false,
        published: false,
        verified: false,
        tamper_detected: false,
        notes: String::new(),
    };

    let old_sui = match ensure_binary(version) {
        Ok(path) => path,
        Err(e) => {
            outcome.notes = format!("download failed: {e}");
            return outcome;
        }
    };
    outcome.downloaded = true;

    if KNOWN_UNBUILDABLE.contains(&version) {
        outcome.notes = "blacklisted: pinned framework revision is unavailable".into();
        return outcome;
    }

    let modern = uses_current_package_system(&old_sui);
    let fixture = root.join(if modern { FIXTURE } else { LEGACY_FIXTURE });

    // `_tmp` keeps the staging directory alive for the rest of the run.
    let (_tmp, sandbox) = match stage_fixture(&fixture, chain_id, modern) {
        Ok(staged) => staged,
        Err(note) => {
            outcome.notes = note;
            return outcome;
        }
    };

    if let Err(note) = publish_fixture(&old_sui, config, &sandbox, modern) {
        outcome.notes = note;
        return outcome;
    }
    outcome.published = true;

    // Verify with the current toolchain; the address and the toolchain to rebuild with come from the
    // metadata the publish wrote. Modern packages go through the CLI end-to-end; legacy packages
    // drive the library directly (see the module docs).
    match verify_one(modern, current_sui, config, &sandbox, chain_id).await {
        Ok(()) => outcome.verified = true,
        Err(e) => outcome.notes = format!("verify failed: {e}"),
    }

    // Tamper the source and confirm verification now fails. This only means anything if the pristine
    // source verified — otherwise verification is failing for an unrelated reason.
    if let Err(e) = tamper(&sandbox) {
        outcome.notes = format!("could not tamper source: {e}");
        return outcome;
    }
    outcome.tamper_detected = outcome.verified
        && verify_one(modern, current_sui, config, &sandbox, chain_id)
            .await
            .is_err();

    outcome
}

/// Verify the package at `sandbox`, dispatching by era. Modern packages go through the CLI
/// end-to-end; legacy packages cannot be resolved onto the cluster by the CLI, so they drive the
/// library directly (see the module docs).
async fn verify_one(
    modern: bool,
    current_sui: &Path,
    config: &Path,
    sandbox: &Path,
    chain_id: &str,
) -> Result<(), String> {
    if modern {
        verify(current_sui, config, sandbox)
    } else {
        verify_legacy(config, sandbox, chain_id).await
    }
}

/// Verify a legacy package in-process by calling the library `verify_source` directly, splitting the
/// verification and build environments the CLI keeps unified (see the module docs). The address and
/// on-chain fetch come from the `localnet` publication; the rebuild uses `testnet` as a formal
/// `--build-env`, which is sound only because the fixture is framework-free.
async fn verify_legacy(config: &Path, sandbox: &Path, chain_id: &str) -> Result<(), String> {
    let context = WalletContext::new(config).map_err(|e| e.to_string())?;
    let flavor = SuiFlavor::with_client(&context);
    let client = context.grpc_client().map_err(|e| e.to_string())?;

    let localnet = Environment::new("localnet".to_string(), chain_id.to_string());
    let publication = read_publication::<SuiFlavor>(sandbox, &localnet, &flavor)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "no localnet publication recorded".to_string())?;

    let build_env = Environment::new("testnet".to_string(), chain_id.to_string());
    verify_source(
        sandbox,
        &publication,
        None,
        &build_env,
        &client,
        Some(config),
    )
    .await
    .map(|_| ())
    .map_err(|e| e.to_string())
}

/// Run `verify-source .` with the current CLI. Returns `Ok` on a zero exit.
fn verify(current_sui: &Path, config: &Path, sandbox: &Path) -> Result<(), String> {
    let out = Command::new(current_sui)
        .args(["client", "--client.config"])
        .arg(config)
        .args(["verify-source", "."])
        .current_dir(sandbox)
        .output()
        .map_err(|e| e.to_string())?;

    if out.status.success() {
        Ok(())
    } else {
        Err(diagnostics(&out))
    }
}

/// The sui CLI prints some errors (notably dependency resolution) to stdout rather than stderr, so
/// collect both when reporting a failed command. Newlines are collapsed to keep the table readable.
fn diagnostics(out: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let joined = [stderr.trim(), stdout.trim()]
        .iter()
        .filter(|s| !s.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join(" | ");
    joined.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Change the fixture's source so that its bytecode no longer matches what was published.
fn tamper(sandbox: &Path) -> std::io::Result<()> {
    let source = sandbox.join("sources").join("m.move");
    let contents = std::fs::read_to_string(&source)?;
    std::fs::write(source, contents.replace("7", "8"))
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), fs_extra::error::Error> {
    fs_extra::dir::copy(from, to, &CopyOptions::new().content_only(true)).map(|_| ())
}

fn print_table(outcomes: &[Outcome]) {
    println!("\nversion    | dl | pub | ver | tamper | notes");
    println!("-----------|----|-----|-----|--------|------");
    for o in outcomes {
        let flag = |b| if b { " y" } else { " n" };
        println!(
            "{:<10} | {} | {}  | {}  | {}     | {}",
            o.version,
            flag(o.downloaded),
            flag(o.published),
            flag(o.verified),
            flag(o.tamper_detected),
            o.notes,
        );
    }
    println!();
}
