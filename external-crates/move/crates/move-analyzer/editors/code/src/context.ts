// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import {
    MOVE_CONF_NAME, LINT_OPT, TYPE_HINTS_OPT, PARAM_HINTS_OPT,
    SUI_PATH_OPT, SERVER_PATH_OPT, Configuration,
} from './configuration';
import * as childProcess from 'child_process';
import * as vscode from 'vscode';
import * as lc from 'vscode-languageclient/node';
import * as semver from 'semver';
import { log } from './log';
import { assert } from 'console';
import { IndentAction } from 'vscode';

function version(path: string, args?: readonly string[]): string | null {
    const versionString = childProcess.spawnSync(
        path, args, { encoding: 'utf8' },
    );
    return versionString.stdout;
}

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

function shouldInstall(bundledVersionString: string | null,
                       bundledVersion: semver.SemVer | null,
                       highestVersionString: string | null,
                       highestVersion: semver.SemVer | null): boolean {
    if (bundledVersionString === null || bundledVersion === null) {
        log.info('No bundled binary');
        return false;
    }
    if (highestVersionString === null || highestVersion === null) {
        log.info(`Installing bundled move-analyzer as no existing version found: v${bundledVersion.version}`);
        return true;
    }
    if (semver.gt(bundledVersion, highestVersion)) {
        log.info(`Installing bundled move-analyzer as the highest version available: v${bundledVersion.version}`);
        return true;
    }
    if (semver.eq(bundledVersion, highestVersion) &&
        bundledVersionString !== highestVersionString) {
        // Bundled version is the same as the highest installed one,
        // but the entire version string including commit sha is different
        // in which case favor the bundled one as it may contain a patch
        log.info(`Installing bundled move-analyzer equal to the highest version available: v${bundledVersion.version}`);
        return true;
    }
    return false;
}

/** Information passed along to each VS Code command defined by this extension. */
export class Context {
    private client: lc.LanguageClient | undefined;

    configuration: Configuration;

    private lintLevel: string;

    private inlayHintsType: boolean;

    private inlayHintsParam: boolean;

    resolvedServerPath: string;

    resolvedServerArgs: string[];

    // The vscode-languageclient module reads a configuration option named
    // "<extension-name>.trace.server" to determine whether to log messages. If a trace output
    // channel is specified, these messages are printed there, otherwise they appear in the
    // output channel that it automatically created by the `LanguageClient` (in this extension,
    // that is 'Move Language Server'). For more information, see:
    // https://code.visualstudio.com/api/language-extensions/language-server-extension-guide#logging-support-for-language-server
    private readonly traceOutputChannel: vscode.OutputChannel;

    constructor(
        private readonly extensionContext: Readonly<vscode.ExtensionContext>,
        client: lc.LanguageClient | undefined = undefined,
    ) {
        this.client = client;
        this.configuration = new Configuration();
        log.info(`configuration: ${this.configuration.toString()}`);
        this.lintLevel = this.configuration.lint;
        this.inlayHintsType = this.configuration.inlayHintsForType;
        this.inlayHintsParam = this.configuration.inlayHintsForParam;
        // Default to configuration.serverPath but may change during server installation
        this.resolvedServerPath = this.configuration.serverPath;
        // Default to no additional args but may change during server installation
        this.resolvedServerArgs = [];
        this.traceOutputChannel = vscode.window.createOutputChannel(
            'Move Language Server Trace',
        );
    }

    /**
     * Registers the given command with VS Code.
     *
     * "Registering" the function means that the VS Code machinery will execute it when the command
     * with the given name is requested by the user. The command names themselves are specified in
     * this extension's `package.json` file, under the key `"contributes.commands"`.
     */
    registerCommand(
        name: Readonly<string>,
        command: (context: Readonly<Context>, ...args: Array<any>) => any,
    ): void {
        const disposable = vscode.commands.registerCommand(
            `move.${name}`,
            async (...args: Array<any>) : Promise<any> => {
                const ret = await command(this, ...args);
                return ret;
            },
        );

        this.extensionContext.subscriptions.push(disposable);
    }

    /**
     * Sets up additional language configuration that's impossible to do via a
     * separate language-configuration.json file. See [1] for more information.
     *
     * This code originates from [2](vscode-rust).
     *
     * [1]: https://github.com/Microsoft/vscode/issues/11514#issuecomment-244707076
     * [2]: https://github.com/rust-lang/vscode-rust/blob/660b412701fe2ea62fad180c40ee4f8a60571c61/src/extension.ts#L287:L287
     */
    configureLanguage(): void {
        const disposable = vscode.languages.setLanguageConfiguration('move', {
            onEnterRules: [
                {
                    // Doc single-line comment
                    // e.g. ///|
                    beforeText: /^\s*\/{3}.*$/,
                    action: { indentAction: IndentAction.None, appendText: '/// ' },
                },
                {
                    // Parent doc single-line comment
                    // e.g. //!|
                    beforeText: /^\s*\/{2}!.*$/,
                    action: { indentAction: IndentAction.None, appendText: '//! ' },
                },
            ],
        });
        this.extensionContext.subscriptions.push(disposable);
    }

    /**
     * Configures and starts the client that interacts with the language server.
     *
     * The "client" is an object that sends messages to the language server, which in Move's case is
     * the `move-analyzer` executable. Unlike registered extension commands such as
     * `move-analyzer.serverVersion`, which are manually executed by a VS Code user via the command
     * palette or menu, this client sends many of its messages on its own (for example, when it
     * starts, it sends the "initialize" request).
     *
     * To read more about the messages sent and responses received by this client, such as
     * "initialize," read [the Language Server Protocol specification](https://microsoft.github.io/language-server-protocol/specifications/specification-current/#initialize).
     *
     * In order to synchronously wait for the client to be completely ready,
     * we need to mark the function as asynchronous
     **/
    async startClient(): Promise<void> {
        const executable: lc.Executable = {
            command: this.resolvedServerPath,
            args: this.resolvedServerArgs,
        };
        const serverOptions: lc.ServerOptions = {
            run: executable,
            debug: executable,
        };

        this.traceOutputChannel.clear();
        const clientOptions: lc.LanguageClientOptions = {
            documentSelector: [{ scheme: 'file', language: 'move' }],
            traceOutputChannel: this.traceOutputChannel,
            initializationOptions: {
                lintLevel: this.lintLevel,
                inlayHintsType: this.inlayHintsType,
                inlayHintsParam: this.inlayHintsParam,
            },
        };

        const client = new lc.LanguageClient(
            'move',
            'Move',
            serverOptions,
            clientOptions,
        );
        log.info('Starting client...');
        const res = client.start();
        this.extensionContext.subscriptions.push({ dispose: async () => client.stop() });
        this.client = client;

        // Wait for the Move Language Server initialization to complete,
        // especially the first symbol table parsing is completed
        return res;
    }

    /**
     * Returns the client that this extension interacts with.
     *
     * @returns lc.LanguageClient
     */
    getClient(): lc.LanguageClient | undefined {
        return this.client;
    }

    /**
     * Deactivates the client interacting with the language server.
     */
    async stopClient(): Promise<void> {
        log.info('Stopping client...');
        if (this.client) {
            await this.client.stop();
        }
    }

    /**
     * Registers a handler to be executed when user/workspace configuration gets changed.
     */
    registerOnDidChangeConfiguration(): void {
        vscode.workspace.onDidChangeConfiguration(async event => {

            const server_path_conf = MOVE_CONF_NAME.concat('.').concat(SERVER_PATH_OPT);
            const sui_path_conf = MOVE_CONF_NAME.concat('.').concat(SUI_PATH_OPT);
            const lint_conf = MOVE_CONF_NAME.concat('.').concat(LINT_OPT);
            const type_hints_conf = MOVE_CONF_NAME.concat('.').concat(TYPE_HINTS_OPT);
            const param_hints_conf = MOVE_CONF_NAME.concat('.').concat(PARAM_HINTS_OPT);

            const optionsChanged = event.affectsConfiguration(lint_conf) ||
                event.affectsConfiguration(type_hints_conf) ||
                event.affectsConfiguration(param_hints_conf);
            const pathsChanged = event.affectsConfiguration(server_path_conf) ||
                event.affectsConfiguration(sui_path_conf);

            if (optionsChanged || pathsChanged) {
                this.configuration = new Configuration();
                log.info(`configuration: ${this.configuration.toString()}`);

                this.lintLevel = this.configuration.lint;
                this.inlayHintsType = this.configuration.inlayHintsForType;
                this.inlayHintsParam = this.configuration.inlayHintsForParam;
                try {
                    await this.stopClient();
                        if (pathsChanged) {
                            await this.installServerBinary(this.extensionContext);
                        }
                        await this.startClient();
                } catch (err) {
                    // Handle error
                    log.info(String(err));
                }
            }
        });

    }

    async installBundledBinary(bundledServerPath: vscode.Uri): Promise<void> {
        // Check if directory to store server binary exists (create it if necessary).
        const serverDirExists = await vscode.workspace.fs.stat(this.configuration.defaultServerDir).then(
            () => true,
            () => false,
        );
        if (serverDirExists) {
            const serverPathExists = await vscode.workspace.fs.stat(this.configuration.defaultServerPath).then(
                () => true,
                () => false,
            );
            if (serverPathExists) {
                log.info(`Deleting existing move-analyzer binary at '${this.configuration.defaultServerPath}'`);
                await vscode.workspace.fs.delete(this.configuration.defaultServerPath);
            }
         } else {
            log.info(`Creating directory for move-analyzer binary at '${this.configuration.defaultServerDir}'`);
            await vscode.workspace.fs.createDirectory(this.configuration.defaultServerDir);
         }

         log.info(`Copying move-analyzer binary to '${this.configuration.defaultServerPath}'`);
         await vscode.workspace.fs.copy(bundledServerPath, this.configuration.defaultServerPath);
    }

    /**
     * Installs language server binary in the default location if needed.
     * On a happy path we just compare versions of installed and bundled
     * binaries and don't do any actual file operations at all.
     *
     * The actual algorithm deciding on how this works as follows.
     *
     * - if the user set an explicit path for the binary, this path is used
     *   even if the binary at that path does not work (which is reported)
     * - otherwise try to establish the highest available version of the binary
     *   - if standalone move-analyzer binary exists, it is now the highest
     *     version available
     *   - if CLI binary is available and its version is higher than the currently
     *     highest one, it is now the highest version
     *   - if there is a binary bundled with the extension and its version
     *     is higher than the currently highest one, it is now the highest version,
     *     and the bundled binary should be installed
     * - if there is no binary available at this point, report an error
     *   to the user
     *
     * @returns `true` if server binary installation succeeded, `false` otherwise.
     */
    async installServerBinary(extensionContext: vscode.ExtensionContext): Promise<boolean> {
        log.info('Installing language server');
        // Args to run move-analyzer by invoking server binary
        const serverArgs: string[] = [];
        // Args to run move-analyzer by invoking CLI binary
        const cliArgs = ['analyzer'];

        // Args to get version out of the server binary
        const serverVersionArgs = serverArgs.concat(['--version']);
        // Args to get version out of CLI binary
        const cliVersionArgs = cliArgs.concat(['--version']);

        // Check if server binary is bundled with the extension
        const bundledServerPath = vscode.Uri.joinPath(extensionContext.extensionUri,
                                                    'language-server',
                                                    this.configuration.serverName);
        const bundledVersionString = version(bundledServerPath.fsPath, serverVersionArgs);
        const bundledVersion = semanticVersion(bundledVersionString);
        log.info(`Bundled version: ${bundledVersion}`);

        const standaloneVersionString = version(this.configuration.serverPath, serverVersionArgs);
        const standaloneVersion = semanticVersion(standaloneVersionString);
        log.info(`Standalone version: ${standaloneVersion}`);

        const cliVersionString = version(this.configuration.suiPath, cliVersionArgs);
        const cliVersion = semanticVersion(cliVersionString);
        log.info(`CLI version: ${cliVersion}`);

        if (this.configuration.serverPath !== this.configuration.defaultServerPath.fsPath) {
            // User has overwritten default server path (need to compare paths as comparing URIs fails
            // for some reason even if one is initialized from another).
            if (standaloneVersion === null) {
                // The server binary on user-overwritten path does not return version number.
                // Need to use modal messages, otherwise the promise returned from is not resolved
                // (and the extension blocks), which can confusing.
                const items: vscode.MessageItem = { title: 'OK', isCloseAffordance: true };
                await vscode.window.showInformationMessage(
                    `The move-analyzer binary at the user-specified path ('${this.configuration.serverPath}') ` +
                    'is not working. See troubleshooting instructions in the README file accompanying ' +
                    'Move VSCode extension by Mysten in the VSCode marketplace',
                    { modal: true },
                    items,
                );
                return false;
            } // Otherwise simply return and use the existing user-specified binary.
            log.info(`Using move-analyzer binary at user-specified path '${this.configuration.serverPath}'`);
            return true;
        }

        assert(this.configuration.serverPath === this.configuration.defaultServerPath.fsPath);

        // What's the highest version installed? Also track path and arguments to run analyzer
        // with the highest version
        let highestVersionString = null;
        let highestVersion = null;
        if (standaloneVersion !== null) {
            highestVersionString = standaloneVersionString;
            highestVersion = standaloneVersion;
            this.resolvedServerPath = this.configuration.serverPath;
            this.resolvedServerArgs = serverArgs;
            log.info(`Setting v${standaloneVersion.version} of installed standalone move-analyzer ` +
                    ` at '${this.resolvedServerPath}' as the highest one`);
        }

        if (cliVersion !== null && (highestVersion === null || semver.gt(cliVersion, highestVersion))) {
            highestVersionString = cliVersionString;
            highestVersion = cliVersion;
            this.resolvedServerPath = this.configuration.suiPath;
            this.resolvedServerArgs = cliArgs;
            log.info(`Setting v${cliVersion.version} of installed CLI move-analyzer ` +
                    ` at '${this.resolvedServerPath}' as the highest one`);
        }

        if (shouldInstall(bundledVersionString, bundledVersion, highestVersionString, highestVersion)) {
            highestVersion = bundledVersion;
            this.resolvedServerPath = this.configuration.defaultServerPath.fsPath;
            this.resolvedServerArgs = serverArgs;
            await this.installBundledBinary(bundledServerPath);
            log.info('Successfuly installed move-analyzer');
        }

        if (highestVersion === null) {
            // There is no installed binary and there is no bundled binary.
            // See a comment earlier in this function for why we need to use modal messages. In this
            // particular case,  the extension would never activate and its settings that could be
            // used to override location of the server binary would not be available.
            const items: vscode.MessageItem = { title: 'OK', isCloseAffordance: true };
            await vscode.window.showErrorMessage(
                'Pre-built move-analyzer binary is not available for this platform. ' +
                'Follow the instructions to manually install the language server in the README ' +
                'file accompanying Move VSCode extension by Mysten in the VSCode marketplace',
                { modal: true },
                items,
            );
            return false;
        }
        return true;

    }

} // Context
