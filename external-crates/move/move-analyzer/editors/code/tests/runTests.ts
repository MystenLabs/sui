// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

/**
 * This file contains what VS Code's documentation refers to as "the test script," which downloads,
 * unzips, launches a VS Code instance with our extension installed, and executes the "test runner."
 * For more information, see:
 * https://code.visualstudio.com/api/working-with-extensions/testing-extension#the-test-script
 */

import * as os from 'os';
import * as path from 'path';
import * as cp from 'child_process';
import * as fs from 'fs';
import * as fse from 'fs-extra';
import {
    runTests,
    downloadAndUnzipVSCode,
    resolveCliArgsFromVSCodeExecutablePath,
} from '@vscode/test-electron';

/**
 * Launches a VS Code instance to run tests.
 *
 * This is essentially a TypeScript program that executes the "VS Code Tokenizer Tests" launch
 * target defined in this repository's `.vscode/launch.json`.
 */
async function runVSCodeTest(vscodeVersion: string): Promise<void> {
    try {
        // The `--extensionDevelopmentPath` argument passed to VS Code. This should point to the
        // directory that contains the extension manifest file, `package.json`.
        const extensionDevelopmentPath = path.resolve(__dirname, '..', '..');

        // The `--extensionTestsPath` argument passed to VS Code. This should point to a JavaScript
        // program that is considered to be the "test suite" for the extension.
        const extensionTestsPath = path.resolve(__dirname, 'index.js');

        // The workspace
        let testWorkspacePath = path.resolve(__dirname, './lsp-demo/lsp-demo.code-workspace');
        if (process.platform === 'win32') {
            testWorkspacePath = path.resolve(__dirname, './lsp-demo/lsp-demo-win.code-workspace');
        }

        // Install vscode and depends extension
        const vscodeExecutablePath = await downloadAndUnzipVSCode(vscodeVersion);
        const [cli, ...args] = resolveCliArgsFromVSCodeExecutablePath(vscodeExecutablePath);
        const newCli = cli ?? 'code';
        cp.spawnSync(newCli, [...args, '--install-extension', 'damirka.move-syntax', '--force'], {
            encoding: 'utf-8',
            stdio: 'inherit',
        });

        // Because the default vscode userDataDir is too long,
        // v1.69.2 will report an error when running test.
        // So generate a short
        const userDataDir = path.join(os.tmpdir(), 'vscode-test', vscodeVersion);
        if (!fs.existsSync(userDataDir)) {
            fse.mkdirsSync(userDataDir);
        }

        // Download VS Code, unzip it, and run the "test suite" program.
        await runTests({
            vscodeExecutablePath: vscodeExecutablePath,
            extensionDevelopmentPath,
            extensionTestsPath,
            launchArgs: [testWorkspacePath, '--user-data-dir', userDataDir],
        });
    } catch (_err: unknown) {
        console.error('Failed to run tests');
        process.exit(1);
    }
}

async function main(): Promise<void> {
    await runVSCodeTest('1.64.0'); // Test with vscode v1.64.0
    await runVSCodeTest('1.69.2'); // Test with vscode v1.69.2
}

void main();
