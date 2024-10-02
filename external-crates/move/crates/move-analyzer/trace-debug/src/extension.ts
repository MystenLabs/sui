// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import * as fs from 'fs';
import * as vscode from 'vscode';
import * as path from 'path';
import { StackFrame } from '@vscode/debugadapter';
import {
    WorkspaceFolder,
    DebugConfiguration,
    CancellationToken,
    TextDocument,
    Position
} from 'vscode';

/**
 * Log level for the debug adapter.
 */
const LOG_LEVEL = 'log';

/**
 * Describes debugger configuration name defined in package.json
 */
const DEBUGGER_TYPE = 'move-debug';

/**
 * Provider of on-hover information during debug session.
 */
class MoveEvaluatableExpressionProvider {
    // TODO: implement a more sophisticated provider that actually provides correct on-hover information,
    // at least for variable definitions whose locations are readily available in the source map
    // (user can always use go-to-def to see the definition and the value)
    provideEvaluatableExpression(_document: TextDocument, _position: Position, _token: CancellationToken) {
        // suppress debug-time on hover information for now
        return null;
    }
}

/**
 * Called when the extension is activated.
*/
export function activate(context: vscode.ExtensionContext) {

    // register a configuration provider for 'move-debug' debug type
    const provider = new MoveConfigurationProvider();
    context.subscriptions.push(vscode.debug.registerDebugConfigurationProvider('move-debug', provider));
    context.subscriptions.push(
        vscode.debug.registerDebugAdapterDescriptorFactory(DEBUGGER_TYPE, {
            createDebugAdapterDescriptor: (session: vscode.DebugSession) => {
                return new vscode.DebugAdapterExecutable(
                    process.execPath,  // This uses the Node.js executable that runs VS Code itself
                    [path.join(context.extensionPath, './out/server.js')]
                );
            }
        })
    );

    let previousSourcePath: string | undefined;
    const decorationType = vscode.window.createTextEditorDecorationType({
        color: 'grey',
        backgroundColor: 'rgba(220, 220, 220, 0.5)' // grey with 50% opacity
    });
    context.subscriptions.push(
        vscode.debug.onDidChangeActiveStackItem(async stackItem => {
            if (stackItem instanceof vscode.DebugStackFrame) {
                const session = vscode.debug.activeDebugSession;
                if (session) {
                    // Request the stack frame details from the debug adapter
                    const stackTraceResponse = await session.customRequest('stackTrace', {
                        threadId: stackItem.threadId,
                        startFrame: stackItem.frameId,
                        levels: 1
                    });

                    const stackFrame: StackFrame = stackTraceResponse.stackFrames[0];
                    if (stackFrame && stackFrame.source && stackFrame.source.path !== previousSourcePath) {
                        previousSourcePath = stackFrame.source.path;
                        const source = stackFrame.source;
                        const line = stackFrame.line;
                        console.log(`Frame details: ${source?.name} at line ${line}`);

                        const editor = vscode.window.activeTextEditor;
                        if (editor) {
                            const optimized_lines = stackTraceResponse.optimized_lines;
                            const document = editor.document;
                            let decorationsArray: vscode.DecorationOptions[] = [];

                            optimized_lines.forEach((lineNumber: number) => {
                                const line = document.lineAt(lineNumber);
                                const lineLength = line.text.length;
                                const lineText = line.text.trim();
                                if (lineText.length !== 0 // ignore empty lines
                                    && !lineText.startsWith("const") // ignore constant declarations (not in the source map)
                                    && !lineText.startsWith("}")) { // ignore closing braces with nothing else on the same line
                                    const decoration = {
                                        range: new vscode.Range(lineNumber, 0, lineNumber, lineLength),
                                    };
                                    decorationsArray.push(decoration);
                                }
                            });

                            editor.setDecorations(decorationType, decorationsArray);
                        }
                    }
                }
            }
        })
    );

    // register a provider of on-hover information during debug session
    const langSelector = { scheme: 'file', language: 'move' };
    context.subscriptions.push(
        vscode.languages.registerEvaluatableExpressionProvider(
            langSelector,
            new MoveEvaluatableExpressionProvider()
        )
    );

    context.subscriptions.push(vscode.debug.onDidTerminateDebugSession(() => {
        // reset all decorations when the debug session is terminated
        // to avoid showing lines for code that was optimized away
        const editor = vscode.window.activeTextEditor;
        if (editor) {
            editor.setDecorations(decorationType, []);
        }
    }));
}

/**
 * Called when the extension is deactivated.
 */
export function deactivate() { }

/**
 * Custom configuration provider for Move debug configurations.
 */
class MoveConfigurationProvider implements vscode.DebugConfigurationProvider {

    /**
     * Massage a debug configuration just before a debug session is being launched,
     * e.g. add all missing attributes to the debug configuration.
     */
    async resolveDebugConfiguration(folder: WorkspaceFolder | undefined, config: DebugConfiguration, token?: CancellationToken): Promise<DebugConfiguration | undefined | null> {

        // if launch.json is missing or empty
        if (!config.type && !config.request && !config.name) {
            const editor = vscode.window.activeTextEditor;
            if (editor && editor.document.languageId === 'move') {

                try {
                    let traceInfo = await findTraceInfo(editor);
                    config.type = DEBUGGER_TYPE;
                    config.name = 'Launch';
                    config.request = 'launch';
                    config.source = '${file}';
                    config.traceInfo = traceInfo;
                    config.stopOnEntry = true;
                    config.logLevel = LOG_LEVEL;
                } catch (err) {
                    const msg = err instanceof Error ? err.message : String(err);
                    return vscode.window.showErrorMessage(msg).then(_ => {
                        return undefined;	// abort launch
                    });
                }
            }
        }

        if (!config.source) {
            const msg = "Unknown error when trying to start the trace viewer";
            return vscode.window.showErrorMessage(msg).then(_ => {
                return undefined;	// abort launch
            });
        }

        return config;
    }
}

/**
 * Finds the trace information for the current active editor.
 *
 * @param editor active text editor.
 * @returns trace information of the form `<package>::<module>::<function>`.
 * @throws Error with a descriptive error message if the trace information cannot be found.
 */
async function findTraceInfo(editor: vscode.TextEditor): Promise<string> {
    const pkgRoot = await findPkgRoot(editor.document.uri.fsPath);
    if (!pkgRoot) {
        throw new Error("Cannot find package root for file: " + editor.document.uri.fsPath);
    }

    const pkgModules = findModules(editor.document.getText());
    if (pkgModules.length === 0) {
        throw new Error("Cannot find any modules in file: " + editor.document.uri.fsPath);
    }

    const tracedFunctions = findTracedFunctions(pkgRoot, pkgModules);

    if (tracedFunctions.length === 0) {
        throw new Error("No traced functions found for package at: " + pkgRoot);
    }

    const fun = tracedFunctions.length === 1
        ? tracedFunctions[0]
        : await pickFunctionToDebug(tracedFunctions);

    if (!fun) {
        throw new Error("No function to be debugged selected from\n" + tracedFunctions.join('\n'));
    }

    return fun;
}

/**
 * Finds the root directory of the package containing the active file.
 * TODO: once `trace-adapter` is in npm registry, we can use the implementation of this function
 * from `trace-adapter`.
 *
 * @param active_file_path path to a file active in the editor.
 * @returns root directory of the package containing the active file.
 */
async function findPkgRoot(active_file_path: string): Promise<string | undefined> {
    const containsManifest = (dir: string): boolean => {
        const filesInDir = fs.readdirSync(dir);
        return filesInDir.includes('Move.toml');
    };

    const activeFileDir = path.dirname(active_file_path);
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

    return undefined;
}

/**
 * Finds modules by searching the content of the file to look for
 * module declarations of the form `module <package>::<module>`.
 * We cannot rely on the directory structure to find modules because
 * trace info is generated based on module names in the source files.
 *
 * @param file_content content of the file.
 * @returns modules in the file content of the form `<package>::<module>`.
 */
function findModules(file_content: string): string[] {
    const modulePattern = /\bmodule\s+\w+::\w+\b/g;
    const moduleSequences = file_content.match(modulePattern);
    return moduleSequences
        ? moduleSequences.map(str => str.substring('module'.length).trim())
        : [];
}

/**
 * Find all functions that have a corresponding trace file.
 *
 * @param pkg_root root directory of the package.
 * @param pkg_modules modules in the package of the form `<package>::<module>`.
 * @returns list of functions of the form `<package>::<module>::<function>`.
 */
function findTracedFunctions(pkg_root: string, pkg_modules: string[]): string[] {
    try {
        const traces_dir = path.join(pkg_root, 'traces');
        const files = fs.readdirSync(traces_dir);
        const result: [string, string[]][] = [];

        pkg_modules.forEach((module) => {
            const prefix = module.replace(/:/g, '_') + '__';
            const prefixFiles = files.filter((file) => file.startsWith(prefix));
            const suffixes = prefixFiles.map((file) => {
                const suffix = file.substring(module.length);
                if (suffix.startsWith('__') && suffix.endsWith('.json')) {
                    return suffix.substring(2, suffix.length - 5);
                }
                return suffix;
            });
            result.push([module, suffixes]);
        });

        return result.map(([module, functionName]) => {
            return functionName.map((func) => module + "::" + func);
        }).flat();
    } catch (err) {
        return [];
    }
}

/**
 * Prompts the user to select a function to debug from a list of traced functions.
 *
 * @param tracedFunctions list of traced functions of the form `<package>::<module>::<function>`.
 * @returns single function to debug of the form `<package>::<module>::<function>`.
 */
async function pickFunctionToDebug(tracedFunctions: string[]): Promise<string | undefined> {
    const selectedFunction = await vscode.window.showQuickPick(tracedFunctions.map(pkgFun => {
        const [pkg, mod, fun] = pkgFun.split('::');
        const modFun = mod + '::' + fun;
        return {
            label: modFun,
            pkg: pkg
        };
    }), {
        canPickMany: false,
        placeHolder: 'Select a function to debug'
    });

    return selectedFunction ? selectedFunction.pkg + '::' + selectedFunction.label : undefined;
}
