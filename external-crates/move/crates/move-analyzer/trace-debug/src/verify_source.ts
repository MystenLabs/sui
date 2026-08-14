// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

/**
 * Advisory verification that source-level debugging artifacts added to a trace
 * (see `./add_source`) actually correspond to the on-chain package used by the
 * trace. The locally built package is compared against on-chain bytecode via
 * `sui client verify-source <pkg> --verify-only <ON_CHAIN_ID>` (available since
 * sui v1.77.0), which compares the modules already compiled under the package's
 * `build` directory against the on-chain package, without rebuilding.
 *
 * The check is strictly advisory: it runs after the artifacts have been copied,
 * never modifies them, and is silent unless verification does not succeed, in
 * which case a single warning notification relays what the verification tool
 * reported.
 *
 * When the trace records that it was generated against a standard network
 * (see `STANDARD_NETWORKS`), verification is pinned to that network via the
 * correspondingly named client environment; otherwise it uses the client's
 * active environment.
 */

import * as cp from 'child_process';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import * as semver from 'semver';
import * as vscode from 'vscode';

/**
 * Name of the artifact file recording, among others,
 * the network the trace was generated against.
 */
const CACHE_SUMMARY_FILE_NAME = 'replay_cache_summary.json';

/**
 * How long to wait for the verification CLI invocation before giving up. The
 * check fetches the on-chain package over the network, which normally
 * completes in seconds.
 */
const VERIFY_TIMEOUT_MS = 60_000;

/**
 * Minimum Sui binary version supporting the verification check (the CLI's
 * `--verify-only` mode was introduced in sui v1.77.0).
 */
const MIN_SUI_VERSION = '1.77.0';

/**
 * How long to wait for the `sui --version` pre-flight check.
 */
const VERSION_CHECK_TIMEOUT_MS = 10_000;

/**
 * Maximum length of the verification tool's output relayed in a warning
 * notification (the full output is logged to the extension host log).
 */
const MAX_DETAIL_LENGTH = 500;

/**
 * Networks for which verification is pinned (via `--client.env`) to the client
 * environment named after the network. These are both the network names the
 * replay tool records in its cache summary and the environment aliases the Sui
 * CLI creates by default. Any other recorded network (a custom endpoint URL)
 * relies on the client's active environment instead, which is also the one
 * most likely to point at it.
 */
const STANDARD_NETWORKS = ['mainnet', 'testnet'];

/**
 * Verifies that the locally built package whose artifacts were just added to a
 * trace (see `addSourceToTrace` in `./add_source`) matches the on-chain
 * package used by the trace. Silent on success; never throws and never blocks
 * debugging: all failures (including inability to run the check at all) only
 * result in a warning message.
 *
 * @param pkgRoot root directory of the local package whose artifacts were added.
 * @param pkgDirName name of the matched package directory in the trace output
 * directory (the `0x`-prefixed id of the on-chain package used by the trace).
 * @param traceDir directory containing the trace file and per-package directories.
 */
export async function verifyAddedSource(
    pkgRoot: string,
    pkgDirName: string,
    traceDir: string
): Promise<void> {
    try {
        await vscode.window.withProgress({
            location: vscode.ProgressLocation.Notification,
            title: `Verifying package '${path.basename(pkgRoot)}' against on-chain bytecode...`,
            cancellable: true
        }, (_progress, token) => runVerification(pkgRoot, pkgDirName, traceDir, token));
    } catch (err) {
        // the check is advisory -- never let it propagate an error
        console.error('Source verification failed unexpectedly:', err);
    }
}

/**
 * Path to the Sui binary used for source verification. Reads the Move IDE
 * extension's `move.sui.path` setting (settings are a shared store, so the
 * value is readable here whether or not that extension is installed or active)
 * and defaults to a binary named `sui` on the system path. Since verficaction
 * only produces warnings, the consequence of this extension being installed
 * independently of the Move IDE extension (and thus having no `move.sui.path`
 * setting) is not fatal and simply means that verification will not be run.
 */
function resolveSuiPath(): string {
    const suiBin = process.platform === 'win32' ? 'sui.exe' : 'sui';
    const suiPath = vscode.workspace.getConfiguration('move').get<string | null>('sui.path') ?? suiBin;
    if (suiPath === suiBin) {
        // bare binary name, resolved via the system path by the spawn itself
        return suiPath;
    }
    if (suiPath.startsWith('~/')) {
        return os.homedir() + suiPath.slice('~'.length);
    }
    return path.resolve(suiPath);
}

/**
 * Network the trace was generated against, read from the replay tool's cache
 * summary artifact next to the trace file: `mainnet`, `testnet`, or the URL of
 * a custom endpoint. Returns `undefined` (silently) if the artifact is missing
 * or cannot be read -- the network is only used to pin verification to the
 * right client environment and to enrich the text of verification warnings.
 */
function readTraceNetwork(traceDir: string): string | undefined {
    try {
        const summary = JSON.parse(
            fs.readFileSync(path.join(traceDir, CACHE_SUMMARY_FILE_NAME), 'utf8')
        );
        const network = summary?.network;
        return typeof network === 'string' ? network : undefined;
    } catch {
        return undefined;
    }
}

/**
 * Runs the verification CLI command, showing a warning notification if it
 * does not succeed (nothing is reported on success, or when cancelled through
 * the progress notification).
 */
async function runVerification(
    pkgRoot: string,
    pkgDirName: string,
    traceDir: string,
    token: vscode.CancellationToken
): Promise<void> {
    const suiPath = resolveSuiPath();
    // A binary predating `--verify-only` would fail with an obscure usage
    // error; check the version up front to report something meaningful
    // instead (an unknown version is let through to attempt verification).
    const suiVersion = semanticVersion(await version(suiPath, ['--version']));
    if (suiVersion !== null && semver.lt(suiVersion, MIN_SUI_VERSION)) {
        vscode.window.showWarningMessage(
            cannotVerifyPrefix(pkgRoot)
            + `the Sui binary is too old (v${suiVersion.version}) - `
            + `v${MIN_SUI_VERSION} or later is required.`
        );
        return;
    }
    // the user may have cancelled during the version check, before the
    // verification process (and its cancellation handler below) existed
    if (token.isCancellationRequested) {
        return;
    }
    return new Promise(resolve => {
        const network = readTraceNetwork(traceDir);
        // specify network on the commmand line only if it is one of the standard ones,
        // otherwise fall back on the client's active environment
        const envArgs = network !== undefined && STANDARD_NETWORKS.includes(network)
            ? ['--client.env', network]
            : [];
        const args = ['client', ...envArgs, 'verify-source', pkgRoot, '--verify-only', pkgDirName];
        let cancelled = false;
        let cancellation: vscode.Disposable | undefined;
        const child = cp.execFile(
            suiPath,
            args,
            { timeout: VERIFY_TIMEOUT_MS, windowsHide: true },
            (error, stdout, stderr) => {
                cancellation?.dispose();
                if (cancelled) {
                    resolve();
                    return;
                }
                // log full output to the console for debugging purposes
                console.log(
                    `Source verification ('${suiPath} ${args.join(' ')}') finished with `
                    + (error ? `error '${error.message}'` : 'success')
                    + `\n--- stdout ---\n${stdout}\n--- stderr ---\n${stderr}`
                );
                if (error) {
                    vscode.window.showWarningMessage(
                        // verification errors are reported on the CLI's stdout
                        // (only auxiliary warnings and usage errors go to
                        // stderr), so inspect both streams
                        verificationFailureMessage(error, stdout + '\n' + stderr, pkgRoot, network)
                    );
                }
                resolve();
            }
        );
        // Close the child's stdin. With a client config present this is a
        // no-op (the CLI never reads stdin). Without one, the CLI my prompt to
        // create a config and wait for input until the timeout expires
        // (closing stdin makes it proceed by accepting the prompt's default instead)
        child.stdin?.end();
        cancellation = token.onCancellationRequested(() => {
            cancelled = true;
            child.kill();
        });
    });
}

/**
 * Chooses the warning text for a failed verification. The only cases handled
 * specially are those detected via the spawn API and producing no tool output
 * (a missing binary and a timeout); any other failure relays what the
 * verification tool reported, rather than interpreting it (the tool's exact
 * messages may change from version to version).
 */
function verificationFailureMessage(
    error: cp.ExecFileException,
    output: string,
    pkgRoot: string,
    network: string | undefined
): string {
    const cannotVerify = cannotVerifyPrefix(pkgRoot);

    if (error.code === 'ENOENT') {
        return cannotVerify
            + `unable to locate the Sui binary (set 'move.sui.path' or add 'sui' `
            + `to the system path).`;
    }
    if (error.killed) {
        return cannotVerify + 'verification timed out.';
    }

    const detail = collapseOutput(output) ?? error.message;
    const networkNote = network !== undefined
        ? ` (trace was generated against '${network}')`
        : '';
    return cannotVerify + detail + networkNote;
}

/**
 * Collapses multi-line CLI output into a single line suitable for a
 * notification, truncated to `MAX_DETAIL_LENGTH` characters, or `undefined`
 * if the output has no non-whitespace content.
 */
function collapseOutput(text: string): string | undefined {
    const collapsed = text
        .split('\n')
        .map(l => l.trim())
        .filter(l => l.length > 0)
        .join(' ');
    if (collapsed.length === 0) {
        return undefined;
    }
    return collapsed.length > MAX_DETAIL_LENGTH
        ? collapsed.slice(0, MAX_DETAIL_LENGTH) + '...'
        : collapsed;
}

/**
 * Common lead-in for verification warnings: every message leads with the
 * consequence for the user, as the failure details that follow are not always
 * self-contained in this regard.
 */
function cannotVerifyPrefix(pkgRoot: string): string {
    return `Could not verify local debugging metadata for '${path.basename(pkgRoot)}' `
        + `against on-chain bytecode - source-level debugging may be unreliable: `;
}

/**
 * Raw output of invoking the binary at `binaryPath` with the given
 * (version-printing) arguments, or `null` if the binary cannot be run or
 * prints nothing.
 */
function version(binaryPath: string, args?: readonly string[]): Promise<string | null> {
    return new Promise(resolve => {
        cp.execFile(
            binaryPath,
            args,
            { timeout: VERSION_CHECK_TIMEOUT_MS, windowsHide: true },
            (_error, stdout, _stderr) => resolve(stdout || null)
        );
    });
}

/**
 * Semantic version parsed from a binary's version string.
 */
function semanticVersion(versionString: string | null): semver.SemVer | null {
    if (versionString !== null) {
        // Version string looks as follows: 'COMMAND_NAME SEMVER-SHA'
        const versionStringWords = versionString.split(' ', 2);
        if (versionStringWords.length < 2) {
            return null;
        }
        const versionParts = versionStringWords[1]?.split('-', 2);
        if (!versionParts) {
            return null;
        }
        if (versionParts.length < 2) {
            return null;
        }
        return semver.parse(versionParts[0]);
    }
    return null;
}
