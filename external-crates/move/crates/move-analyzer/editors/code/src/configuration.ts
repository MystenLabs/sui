// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import * as os from 'os';
import * as vscode from 'vscode';
import * as Path from 'path';
import { log } from './log';
import { assert } from 'console';


/**
 * User-defined configuration values, such as those specified in VS Code settings.
 *
 * This provides a more strongly typed interface to the configuration values specified in this
 * extension's `package.json`, under the key `"contributes.configuration.properties"`.
 */
export class Configuration {
    private readonly configuration: vscode.WorkspaceConfiguration;

    /** Default directory for the location of the language server binary */
    private readonly defaultServerDir: vscode.Uri;

    /** Name of the language server binary */
    private readonly serverName: string;

    /** Default path to the language server binary */
    readonly defaultServerPath: vscode.Uri;

    constructor() {
        this.configuration = vscode.workspace.getConfiguration('move');
        this.defaultServerDir = vscode.Uri.joinPath(vscode.Uri.file(os.homedir()), '.sui', 'bin');
        if (process.platform === 'win32') {
            this.serverName = 'move-analyzer.exe';
        } else {
            this.serverName = 'move-analyzer';
        }
        this.defaultServerPath = vscode.Uri.joinPath(this.defaultServerDir, this.serverName);
    }

    /** A string representation of the configured values, for logging purposes. */
    toString(): string {
        return JSON.stringify(this.configuration);
    }

    /** The path to the move-analyzer executable. */
    get serverPath(): string {
        const serverPath = this.configuration.get<string | null>('server.path') ?? this.defaultServerPath.fsPath;
        if (serverPath.startsWith('~/')) {
            return os.homedir() + serverPath.slice('~'.length);
        }
        return Path.resolve(serverPath);
    }

    /**
     * Installs language server binary in the default location unless a user-specified
     * (but not default) server location already contains a server binary.
     *
     * @returns `true` if server binary installation succeeded, `false` otherwise.
     */
    async installServerBinary(extensionContext: vscode.ExtensionContext): Promise<boolean> {
        log.info('Installing language server binary');

        // Check if server binary is bundled with the extension
        const bundledServerPath = vscode.Uri.joinPath(extensionContext.extensionUri,
                                                    'language-server',
                                                    this.serverName);
        const bundledServerExists = await vscode.workspace.fs.stat(bundledServerPath).then(
            () => true,
            () => false,
        );

        const serverPath = vscode.Uri.file(this.serverPath);
        const serverPathExists = await vscode.workspace.fs.stat(serverPath).then(
            () => true,
            () => false,
        );

        if (serverPath.fsPath !== this.defaultServerPath.fsPath) {
            // User has overwritten default server path (need to compare paths as comparing URIs fails
            // for some reason even if one is initialized from another).
            if (!serverPathExists) {
                // The server binary on user-overwritten path does not exist so warn the user about it.
                // Need to use modal messages, otherwise the promise returned from is not resolved
                // (and the extension blocks), which can confusing.
                const items: vscode.MessageItem = { title: 'OK', isCloseAffordance: true };
                await vscode.window.showInformationMessage(
                    `The move-analyzer binary at the user-specified path ('${this.serverPath}') ` +
                    'is not available. Put the binary in the path or reset user settings and ' +
                    'reinstall the extension to use the bundled binary.',
                    { modal: true },
                    items,
                );
                return false;
            } // Otherwise simply return and use the existing user-specified binary.
            return true;
        }

        assert(serverPath === this.defaultServerPath);
        if (serverPathExists) {
            // If a binary is bundled with the extension, delete existing binary in the default location.
            // If the user wants to use another binary, they must specify an alternative location
            // (and the docs reflect this). If there is no bundled binary, used the existing one.
            if (bundledServerExists) {
                await vscode.workspace.fs.delete(this.defaultServerPath);
            } else {
                // Since there is no bundled binary, let's use the one that's in the (default) path.
                return true;
            }
        }

        if (!bundledServerExists) {
            // See a comment earlier in this function for why we need to use modal messages. In this
            // particular case,  the extension would never activate and its settings that could be
            // used to override location of the server binary would not be available.
            const items: vscode.MessageItem = { title: 'OK', isCloseAffordance: true };
            await vscode.window.showErrorMessage(
                'Pre-built move-analyzer binary is not available for this platform. ' +
                'Follow the instructions in the move-analyzer Visual Studio Code extension ' +
                'README to manually install the language server.',
                { modal: true },
                items,
            );
            return false;
        }

        // Check if directory to store server binary exists (create it if necessary).
        const serverDirExists = await vscode.workspace.fs.stat(this.defaultServerDir).then(
            () => true,
            () => false,
        );
        if (!serverDirExists){
            await vscode.workspace.fs.createDirectory(this.defaultServerDir);
        }

        await vscode.workspace.fs.copy(bundledServerPath, this.defaultServerPath);

        return true;
    }

}

export function lint(): boolean {
    return vscode.workspace.getConfiguration('move').get('lint') ?? true;
}
