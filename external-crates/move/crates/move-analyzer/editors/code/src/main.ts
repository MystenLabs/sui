// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Configuration } from './configuration';
import { Context } from './context';
import { Extension } from './extension';
import { log } from './log';

import * as childProcess from 'child_process';
import * as vscode from 'vscode';
import * as commands from './commands';


/**
 * An extension command that displays the version of the server that this extension
 * interfaces with.
 */
async function serverVersion(context: Readonly<Context>): Promise<void> {
    const version = childProcess.spawnSync(
        context.configuration.serverPath, ['--version'], { encoding: 'utf8' },
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
    const globalMoveVersionKey = 'move-version';
    const extension = new Extension();
    log.info(`${extension.identifier} version ${extension.version}`);

    const configuration = new Configuration();
    log.info(`configuration: ${configuration.toString()}`);

    // VSCode does not provide a hook for install/update extension, and we don't want to attempt
    // installation of move-analyzer binaries every time an extension is activated (e.g. after
    // VSCode restart).
    //
    // On a happy path (when user does not mock with user settings), we install move-analyzer
    // whenever the extension itself is installed or upgraded, and skip installation when
    // the globally stored extension version number does not change. However, even in this
    // case we want to run the move-analyzer installation procedure if the move-analyzer
    // is for some reason unavailable (e.g., because the user messed up user settings between
    // VSCode restarts).
    //
    // We also don't want to update the extension version in the global state until we know
    // that move-analyzer installation succeeded as the global state change is permanent.

    const lastMoveVersion = extensionContext.globalState.get(globalMoveVersionKey);
    let doInstallBinary: boolean;
    let updateGlobalExtVersion: boolean;
    if (lastMoveVersion === null) {
        // Installation (no global variable set).
        doInstallBinary = true;
        updateGlobalExtVersion = true;
    } else if (lastMoveVersion === extension.version) {
        // Not an installation or an update (same version as seen before).
        const serverPathExists = await vscode.workspace.fs.stat(vscode.Uri.file(configuration.serverPath)).then(
            () => true,
            () => false,
        );
        doInstallBinary = !serverPathExists;
        updateGlobalExtVersion = false;
    } else {
        // Update (different versions).
        doInstallBinary = true;
        updateGlobalExtVersion = true;
    }

    if (doInstallBinary) {
        const success = await configuration.installServerBinary(extensionContext);
        if (!success) {
            return;
        }
    }

    log.info('Creating extension context');
    const context = Context.create(extensionContext, configuration);
    // An error here -- for example, if the path to the `move-analyzer` binary that the user
    // specified in their settings is not valid -- prevents the extension from providing any
    // more utility, so return early.
    if (context instanceof Error) {
        void vscode.window.showErrorMessage(
            `Could not activate Move: ${context.message}.`,
        );
        return;
    }

    // Register handlers for VS Code commands that the user explicitly issues.
    context.registerCommand('serverVersion', serverVersion);

    // Configure other language features.
    context.configureLanguage();

    // All other utilities provided by this extension occur via the language server.
    await context.startClient();
    context.registerCommand('textDocumentDocumentSymbol', commands.textDocumentDocumentSymbol);
    context.registerCommand('textDocumentHover', commands.textDocumentHover);
    context.registerCommand('textDocumentCompletion', commands.textDocumentCompletion);

    context.registerOnDidChangeConfiguration();

    if (updateGlobalExtVersion) {
        await extensionContext.globalState.update(globalMoveVersionKey, extension.version);
    }
}
