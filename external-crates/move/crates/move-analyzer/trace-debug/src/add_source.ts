// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

/**
 * Support for adding source-level debugging artifacts to a trace that contains
 * external events (see `isExternalEventsTrace` in `extension.ts`).
 *
 * The output directory of such a trace contains one subdirectory per on-chain Move
 * package used by the trace, named after the package's id (a `0x`-prefixed hex
 * string), each with a `source` subdirectory that the debug adapter reads to provide
 * source-level (rather than disassembly-level) debugging. That `source` subdirectory
 * is empty unless populated by the user.
 *
 * The logic in this file (package directory discovery, matching, and copying)
 * populates the `source` subdirectory for a package the user has built locally: it
 * finds the matching package directory by comparing the built package's on-chain
 * published id against the package directories present in the trace output
 * directory, then copies the package's build output into that directory's `source`
 * subdirectory.
 *
 * The published id is searched for in the package's metadata files in the order in
 * which the package system introduced them: `Published.toml` (which, when present,
 * is the definitive record of the package's publications and ends the search), then
 * `Move.lock`, then the (legacy) `published-at` field in `Move.toml`. Only the id of
 * the latest published version is considered, as this is the version the locally
 * built artifacts can be assumed to describe. A package published to multiple
 * networks records one id per network and all of them become match candidates (of
 * which at most one can actually match), as the trace output directory itself is
 * network-agnostic.
 */

import * as fs from 'fs';
import * as path from 'path';
import * as vscode from 'vscode';
import toml from '@iarna/toml';

/**
 * Name of a package's manifest file.
 */
export const MANIFEST_FILE_NAME = 'Move.toml';

/**
 * Name of a package's lock file (records, among others, on-chain published ids).
 */
const LOCK_FILE_NAME = 'Move.lock';

/**
 * Name of a package's published file (modern tooling's authoritative record of
 * on-chain published ids, per environment).
 */
const PUBLISHED_FILE_NAME = 'Published.toml';

/**
 * Name of the build output subdirectory of a Move package.
 */
const BUILD_DIR = 'build';

/**
 * Subdirectories of a package's build output that hold Move source files and
 * their source maps (debug infos). The source maps live in `source_maps` (older
 * builds) or `debug_info` (newer builds).
 */
const BUILD_SOURCES_DIR = 'sources';
const BUILD_SOURCE_MAPS_DIR = 'source_maps';
const BUILD_DEBUG_INFO_DIR = 'debug_info';

/**
 * Subdirectory of a package directory (in a trace output directory) where the
 * user-provided Move source files and their debug infos are placed.
 */
const SOURCE_DIR = 'source';

/**
 * Pattern matching a package directory in a trace output directory. Such
 * directories are named after the on-chain package id (a `0x`-prefixed
 * hexadecimal string).
 */
const PKG_DIR_PATTERN = /^0x[0-9a-fA-F]+$/;

/**
 * Enumerates package directories in a trace output directory. Such directories
 * are named after the on-chain package id (a `0x`-prefixed hexadecimal string)
 * and contain the bytecode (and optionally source) debugging artifacts for a
 * single package.
 *
 * @param traceDir directory containing the trace file and per-package directories.
 * @returns names of the package directories found in `traceDir`.
 */
function enumeratePackageDirs(traceDir: string): string[] {
    return fs.readdirSync(traceDir).filter(dirOrFile => {
        const fullPath = path.join(traceDir, dirOrFile);
        return fs.statSync(fullPath).isDirectory() && PKG_DIR_PATTERN.test(dirOrFile);
    });
}

/**
 * Normalizes a package id (either a package directory name or a published id) to a
 * canonical lower-case 64-character hex string (without the `0x` prefix) so that ids
 * written in different forms (e.g. a short `0x2` in a lock file vs. a zero-padded
 * package directory name) compare equal.
 *
 * @param packageId package id string, with or without a `0x` prefix.
 * @returns normalized 64-character hex string.
 */
function normalizePackageId(packageId: string): string {
    return packageId.toLowerCase().replace(/^0x/, '').padStart(64, '0');
}

/**
 * Reads the on-chain published package ids recorded in a package's published file.
 * The file records one publication per environment in its `published` table, with
 * the id of the latest published version in the `published-at` entry.
 *
 * @param pkgRoot root directory of the package (containing the published file).
 * @returns published ids found across all environments (empty if the file records
 * none), or `undefined` if the file does not exist.
 */
function readIdsFromPublishedFile(pkgRoot: string): string[] | undefined {
    const pubFilePath = path.join(pkgRoot, PUBLISHED_FILE_NAME);
    if (!fs.existsSync(pubFilePath)) {
        return undefined;
    }
    const parsedPub = toml.parse(fs.readFileSync(pubFilePath, 'utf8')) as any;
    const publishedTable = parsedPub.published;
    if (!publishedTable || typeof publishedTable !== 'object') {
        return [];
    }
    const publishedIds: string[] = [];
    for (const pub of Object.values(publishedTable) as any[]) {
        if (typeof pub?.['published-at'] === 'string') {
            publishedIds.push(pub['published-at']);
        }
    }
    return publishedIds;
}

/**
 * Reads the on-chain published package ids recorded in a package's lock file.
 * The file records one publication per environment in its `env` table, with the
 * id of the latest published version in the `latest-published-id` entry.
 *
 * @param pkgRoot root directory of the package (containing the lock file).
 * @returns published ids found across all environments (empty if the lock file is
 * missing or records no published ids).
 */
function readIdsFromLockFile(pkgRoot: string): string[] {
    const lockFilePath = path.join(pkgRoot, LOCK_FILE_NAME);
    if (!fs.existsSync(lockFilePath)) {
        return [];
    }
    const parsedLock = toml.parse(fs.readFileSync(lockFilePath, 'utf8')) as any;
    const envTable = parsedLock.env;
    if (!envTable || typeof envTable !== 'object') {
        return [];
    }
    const publishedIds: string[] = [];
    for (const env of Object.values(envTable) as any[]) {
        if (typeof env?.['latest-published-id'] === 'string') {
            publishedIds.push(env['latest-published-id']);
        }
    }
    return publishedIds;
}

/**
 * Reads the on-chain published package id recorded in a package's manifest via the
 * deprecated `published-at` field. This is a fallback for packages that do not
 * record published ids in their published or lock file (e.g. system packages such
 * as the standard library, or sources predating these files).
 *
 * @param pkgRoot root directory of the package (containing the manifest).
 * @returns the published id, or `undefined` if the manifest is missing or does not
 * record one.
 */
function readIdFromManifest(pkgRoot: string): string | undefined {
    const manifestPath = path.join(pkgRoot, MANIFEST_FILE_NAME);
    if (!fs.existsSync(manifestPath)) {
        return undefined;
    }
    const parsedManifest = toml.parse(fs.readFileSync(manifestPath, 'utf8')) as any;
    const publishedAt = parsedManifest?.package?.['published-at'];
    return typeof publishedAt === 'string' ? publishedAt : undefined;
}

/**
 * Reads the on-chain published package ids recorded for a package, searching its
 * metadata files in the order in which the package system introduced them. The
 * published file, when present, is the definitive record of the package's
 * publications and is used exclusively. Otherwise the ids are taken from the lock
 * file and, if it does not record any, from the manifest's deprecated
 * `published-at` field.
 *
 * A warning naming the searched file(s) is shown if no published id is recorded.
 *
 * @param pkgRoot root directory of the package.
 * @returns published ids recorded for the package, one per environment the package
 * was published to (empty if none are recorded, in which case a warning has been
 * shown).
 */
function readPublishedIds(pkgRoot: string): string[] {
    // The published file, when present, is authoritative and ends the search
    // even if it records no ids (the package is then simply not published).
    // `undefined` means the file is absent and fallbacks should be tried.
    const publishedFileIds = readIdsFromPublishedFile(pkgRoot);
    if (publishedFileIds !== undefined) {
        if (publishedFileIds.length === 0) {
            vscode.window.showWarningMessage(
                `Package at '${pkgRoot}' does not have a published id recorded in `
                + `'${PUBLISHED_FILE_NAME}'. `
                + `Debugging this package will remain at the disassembly level.`
            );
        }
        return publishedFileIds;
    }
    const lockIds = readIdsFromLockFile(pkgRoot);
    if (lockIds.length > 0) {
        return lockIds;
    }
    const manifestId = readIdFromManifest(pkgRoot);
    if (manifestId) {
        return [manifestId];
    }
    vscode.window.showWarningMessage(
        `Package at '${pkgRoot}' does not have a published id recorded in `
        + `'${LOCK_FILE_NAME}' or '${MANIFEST_FILE_NAME}'. `
        + `Debugging this package will remain at the disassembly level.`
    );
    return [];
}

/**
 * Finds the package directory in a trace output directory whose package id matches
 * one of the provided published ids. The ids record the package's publications on
 * different networks, so at most one of them can correspond to a package directory
 * used by the trace.
 *
 * @param publishedIds published package ids to look for.
 * @param packageDirs names of the package directories present in the trace output
 * directory.
 * @returns the matching package directory name, or `undefined` if none matches.
 */
function matchPackageDir(publishedIds: string[], packageDirs: string[]): string | undefined {
    const normalizedDirs = new Map(packageDirs.map(dir => [normalizePackageId(dir), dir]));
    for (const publishedId of publishedIds) {
        const match = normalizedDirs.get(normalizePackageId(publishedId));
        if (match) {
            return match;
        }
    }
    return undefined;
}

/**
 * Locates the build output directory of a package and validates that it contains
 * the artifacts required for source-level debugging (Move source files and their
 * source maps / debug infos). A warning is shown if the directory cannot be
 * located or is missing these artifacts.
 *
 * @param pkgRoot root directory of the package (containing the manifest and the
 * `build` directory).
 * @returns path to the package's build output directory, or `undefined` if it
 * cannot be located or is missing debugging artifacts.
 */
function resolveBuildDir(pkgRoot: string): string | undefined {
    const manifestPath = path.join(pkgRoot, MANIFEST_FILE_NAME);
    if (!fs.existsSync(manifestPath)) {
        vscode.window.showWarningMessage(`No '${MANIFEST_FILE_NAME}' found in '${pkgRoot}'.`);
        return undefined;
    }
    const parsedManifest = toml.parse(fs.readFileSync(manifestPath, 'utf8')) as any;
    const pkgName = parsedManifest?.package?.name;
    if (typeof pkgName !== 'string') {
        vscode.window.showWarningMessage(`Cannot determine package name from '${manifestPath}'.`);
        return undefined;
    }
    const buildDir = path.join(pkgRoot, BUILD_DIR, pkgName);
    // source maps live in `source_maps` (older builds) or `debug_info` (newer builds)
    const hasDebugInfo = fs.existsSync(path.join(buildDir, BUILD_SOURCE_MAPS_DIR))
        || fs.existsSync(path.join(buildDir, BUILD_DEBUG_INFO_DIR));
    const hasSources = fs.existsSync(path.join(buildDir, BUILD_SOURCES_DIR));
    if (!hasDebugInfo || !hasSources) {
        vscode.window.showWarningMessage(
            `Package '${pkgName}' at '${pkgRoot}' is not built with debugging artifacts. `
            + `Build it before adding its source to the trace.`
        );
        return undefined;
    }
    return buildDir;
}

/**
 * Prepares a package directory's `source` subdirectory to receive the build output,
 * prompting for confirmation (and clearing it) if it already contains files added
 * previously.
 *
 * @param sourceDir the `source` subdirectory inside the package directory.
 * @param pkgDirName name of the package directory (for display).
 * @returns `true` if the directory is ready to be populated, `false` if the user
 * declined to overwrite existing content.
 */
async function prepareSourceDir(sourceDir: string, pkgDirName: string): Promise<boolean> {
    if (fs.existsSync(sourceDir) && fs.readdirSync(sourceDir).length > 0) {
        const overwrite = 'Overwrite';
        const choice = await vscode.window.showWarningMessage(
            `Source for package '${pkgDirName}' has already been added. Overwrite it?`,
            { modal: true },
            overwrite
        );
        if (choice !== overwrite) {
            return false;
        }
        fs.rmSync(sourceDir, { recursive: true, force: true });
    }
    fs.mkdirSync(sourceDir, { recursive: true });
    return true;
}

/**
 * Adds source-level debugging artifacts of a built Move package to a trace,
 * enabling source-level (rather than disassembly-level) debugging. The matching
 * package directory for the package selected by the user is identified by comparing
 * the package's published id (read from its `Published.toml`/`Move.lock`/
 * `Move.toml`) against the package directories present in the trace output
 * directory, and the package's build output is copied into that directory's
 * `source` subdirectory. If no match is found (or the package is not built with
 * debugging artifacts), a warning is shown and debugging remains at the disassembly
 * level for the package.
 *
 * This is meant to be used from the trace viewer before a debug session is started,
 * so that the artifacts are picked up when the session is launched (no restart is
 * needed). The caller is expected to have verified that `traceFilePath` is a trace
 * containing external events.
 *
 * @param traceFilePath path to the trace file shown in the trace viewer.
 */
export async function addSourceToTrace(traceFilePath: string): Promise<void> {
    const traceDir = path.dirname(traceFilePath);
    const packageDirs = enumeratePackageDirs(traceDir);
    if (packageDirs.length === 0) {
        vscode.window.showErrorMessage(`No package directories found in '${traceDir}'.`);
        return;
    }

    const selectedFolders = await vscode.window.showOpenDialog({
        canSelectFolders: true,
        canSelectFiles: false,
        canSelectMany: false,
        openLabel: 'Add Source',
        title: 'Select built Move package folder'
    });
    if (!selectedFolders || selectedFolders.length === 0) {
        return;
    }
    const pkgRoot = selectedFolders[0].fsPath;

    const publishedIds = readPublishedIds(pkgRoot);
    if (publishedIds.length === 0) {
        // readPublishedIds has already reported the problem to the user
        return;
    }
    const pkgDirName = matchPackageDir(publishedIds, packageDirs);
    if (!pkgDirName) {
        vscode.window.showWarningMessage(
            `Could not match package at '${pkgRoot}' to any package in the trace: `
            + `none of its published ids (${publishedIds.join(', ') || '[]'}) is used by `
            + `this trace (possibly because it used a different version of the `
            + `package). `
            + `Debugging this package will remain at the disassembly level.`
        );
        return;
    }
    const buildDir = resolveBuildDir(pkgRoot);
    if (!buildDir) {
        // resolveBuildDir has already reported the problem to the user
        return;
    }
    const sourceDir = path.join(traceDir, pkgDirName, SOURCE_DIR);
    if (!await prepareSourceDir(sourceDir, pkgDirName)) {
        return;
    }
    try {
        fs.cpSync(buildDir, sourceDir, { recursive: true });
    } catch (err) {
        // remove partially copied artifacts so that the next debug session
        // does not load an incomplete source view (debugging for this package
        // cleanly remains at the disassembly level instead)
        fs.rmSync(sourceDir, { recursive: true, force: true });
        throw err;
    }
    vscode.window.showInformationMessage(
        `Added source for '${path.basename(pkgRoot)}' (${pkgDirName}). `
        + `Start debugging to use the source-level view.`
    );
}
