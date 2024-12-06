// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Context } from './context';
import { Extension } from './extension';
import { log } from './log';

import * as childProcess from 'child_process';
import * as fs from 'fs';
import * as path from 'path';
import * as vscode from 'vscode';
import * as commands from './commands';


/**
 * An extension command that displays the version of the server that this extension
 * interfaces with.
 */
async function serverVersion(context: Readonly<Context>): Promise<void> {
    const version = childProcess.spawnSync(
        context.resolvedServerPath,
        context.resolvedServerArgs.concat(['--version']),
        { encoding: 'utf8' },
    );
    if (version.stdout) {
        await vscode.window.showInformationMessage(version.stdout);
    } else if (version.error) {
        await vscode.window.showErrorMessage(
            `Could not execute move-analyzer: ${version.error.message}.`,
        );
    } else {
        await vscode.window.showErrorMessage(
            `A problem occurred when executing '${context.configuration.serverPath}'.`,
        );
    }
}

async function findPkgRoot(): Promise<string | undefined> {
    const activeEditor = vscode.window.activeTextEditor;
    if (!activeEditor) {
        await vscode.window.showErrorMessage('Cannot find package manifest (no active editor window)');
        return undefined;
    }

    const containsManifest = (dir: string): boolean => {
        const filesInDir = fs.readdirSync(dir);
        return filesInDir.includes('Move.toml');
    };

    const activeFileDir = path.dirname(activeEditor.document.uri.fsPath);
    let currentDir = activeFileDir;
    while (currentDir !== path.parse(currentDir).root) {
        if (containsManifest(currentDir)) {
            return currentDir;
        }
        currentDir = path.resolve(currentDir, '..');
    }

    if (containsManifest(currentDir)) {
        return currentDir;
    }

    await vscode.window.showErrorMessage(`Cannot find package manifest for file in '${activeFileDir}' directory`);
    return undefined;
}

async function suiMoveCmd(context: Readonly<Context>, cmd: string): Promise<void> {
    const version = childProcess.spawnSync(
        context.configuration.suiPath, ['--version'], { encoding: 'utf8' },
    );
    if (version.stdout) {
        const pkgRoot = await findPkgRoot();
        if (pkgRoot !== undefined) {
            const terminalName = 'sui move';
            let terminal = vscode.window.terminals.find(t => t.name === terminalName);
            if (!terminal) {
                terminal = vscode.window.createTerminal(terminalName);
            }
            terminal.show(true);
            terminal.sendText('cd ' + pkgRoot, true);
            terminal.sendText(`${context.configuration.suiPath} move ${cmd}`, true);
        }
    } else {
        await vscode.window.showErrorMessage(
            `A problem occurred when executing the Sui command: '${context.configuration.suiPath}'`
            + 'Make sure that Sui CLI is installed and available, either in your global PATH, '
            + 'or on a path set via `move.sui.path` configuration option.',
        );
    }
}

/**
 * An extension command that that builds the current Move project.
 */
async function buildProject(context: Readonly<Context>): Promise<void> {
    return suiMoveCmd(context, 'build');
}

/**
 * An extension command that that tests the current Move project.
 */
async function testProject(context: Readonly<Context>): Promise<void> {
    const filter = await vscode.window.showInputBox({
        title: 'Testing Move package',
        prompt: 'Enter filter string to only run tests whose names contain the string'
            + '(leave empty to run all tests)',
        ignoreFocusOut: true, // Keeps the input box open when it loses focus
    });
    if (filter !== undefined) {
        const cmd = filter.length > 0 ? `test ${filter}` : 'test';
        return suiMoveCmd(context, cmd);
    }
    return Promise.resolve();
}

/**
 * An extension command that that traces the current Move project.
 */
async function traceProject(context: Readonly<Context>): Promise<void> {
    const filter = await vscode.window.showInputBox({
        title: 'Tracing Move package',
        prompt: 'Enter filter string to only trace tests whose names contain the string'
            + '(leave empty to trace all tests)',
        ignoreFocusOut: true, // Keeps the input box open when it loses focus
    });
    if (filter !== undefined) {
        const cmd = filter.length > 0 ? `test ${filter} --trace-execution` : 'test --trace-execution';
        return suiMoveCmd(context, cmd);
    }
    return Promise.resolve();
}

/**
 * The entry point to this VS Code extension.
 *
 * As per [the VS Code documentation on activation
 * events](https://code.visualstudio.com/api/references/activation-events), "an extension must
 * export an `activate()` function from its main module and it will be invoked only once by
 * VS Code when any of the specified activation events [are] emitted."
 *
 * Activation events for this extension are listed in its `package.json` file, under the key
 * `"activationEvents"`.
 *
 * In order to achieve synchronous activation, mark the function as an asynchronous function,
 * so that you can wait for the activation to complete by await
 */
export async function activate(extensionContext: Readonly<vscode.ExtensionContext>): Promise<void> {
    const extension = new Extension();
    log.info(`${extension.identifier} version ${extension.version}`);

    log.info('Creating extension context');
    const context = new Context(extensionContext);
    const success = await context.installServerBinary(extensionContext);
    if (!success) {
        // Return early (errors have already been reported)
        return;
    }

    // Register handlers for VS Code commands that the user explicitly issues.
    context.registerCommand('serverVersion', serverVersion);
    context.registerCommand('build', buildProject);
    context.registerCommand('test', testProject);
    context.registerCommand('trace', traceProject);

    // Configure other language features.
    context.configureLanguage();

    // All other utilities provided by this extension occur via the language server.
    await context.startClient();
    context.registerCommand('textDocumentDocumentSymbol', commands.textDocumentDocumentSymbol);
    context.registerCommand('textDocumentHover', commands.textDocumentHover);
    context.registerCommand('textDocumentCompletion', commands.textDocumentCompletion);

    context.registerOnDidChangeConfiguration();
}
