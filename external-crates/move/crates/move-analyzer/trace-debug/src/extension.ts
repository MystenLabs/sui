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
import { decompress } from 'fzstd';

/**
 * Log level for the debug adapter.
 */
const LOG_LEVEL = 'log';

/**
 * Describes debugger configuration name defined in package.json
 */
const DEBUGGER_TYPE = 'move-debug';

/**
 * The name of the Move language.
 */
const MOVE_LANGUAGE_ID = 'move';

/**
 * File extension for Move source files.
 */
const MOVE_FILE_EXT = '.' + MOVE_LANGUAGE_ID;

/**
 * The extension for the trace files.
 */
const TRACE_FILE_EXT = ".json.zst";

/**
 * Name of the trace file containing external events.
 */
const EXT_EVENTS_TRACE_FILE_NAME = 'trace';

/**
 * The URI scheme for the trace files.
 */
const TRACE_FILE_URI_SCHEME = 'mtrace';

/**
 * Language identifier for the trace files.
 */
const TRACE_FILE_LANGUAGE_ID = TRACE_FILE_URI_SCHEME;

/**
 * Idengifier for the custom editor to view move trace files.
 */
const TRACE_CUSTOM_EDITOR_ID = 'mtrace.viewer';

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
 * Information about a traced function.
 */
interface TracedFunctionInfo {
    pkgAddr: number;
    module: string;
    function: string;
}

/**
 * Called when the extension is activated.
*/
export function activate(context: vscode.ExtensionContext) {
    // register a configuration provider for DEBUGGER_TYPE
    const provider = new MoveConfigurationProvider();
    context.subscriptions.push(vscode.debug.registerDebugConfigurationProvider(DEBUGGER_TYPE, provider));
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
        backgroundColor: 'rgba(220, 220, 220, 0.3)' // grey with 30% opacity
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

                    const editor = vscode.window.activeTextEditor;
                    if (!editor) {
                        return;
                    }
                    const optimized_lines = stackTraceResponse.optimizedLines;
                    let decorationsArray: vscode.DecorationOptions[] = [];
                    if (optimized_lines && optimized_lines.length > 0) {
                        const stackFrame: StackFrame = stackTraceResponse.stackFrames[0];
                        if (stackFrame && stackFrame.source) {
                            if (stackFrame.source.path === previousSourcePath) {
                                // don't do anything (neither reset nor set decorations)
                                // if the source path is the same as the previous one
                                return;
                            }
                            previousSourcePath = stackFrame.source.path;
                            const document = editor.document;

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

                        }
                    }
                    editor.setDecorations(decorationType, decorationsArray);
                }
            }
        })
    );

    // register a provider of on-hover information during debug session
    const langSelector = { scheme: 'file', language: MOVE_LANGUAGE_ID };
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

    // register custom command to toggle disassembly view
    context.subscriptions.push(vscode.commands.registerCommand('move.toggleDisassembly', () => {
        const session = vscode.debug.activeDebugSession;
        if (session) {
            session.customRequest('toggleDisassembly');
        }
    }));

    // register custom command to toggle source view (when in disassembly view)
    context.subscriptions.push(vscode.commands.registerCommand('move.toggleSource', () => {
        const session = vscode.debug.activeDebugSession;
        if (session) {
            session.customRequest('toggleSource');
        }
    }));

    // send custom request to the debug adapter when the active text editor changes
    context.subscriptions.push(vscode.window.onDidChangeActiveTextEditor(async editor => {
        if (editor) {
            const session = vscode.debug.activeDebugSession;
            if (session) {
                await session.customRequest('fileChanged', editor.document.uri.fsPath);
            }
        }
    }));

    // Create and register custom content provider for compressed trace files,
    // as well as custom editor for Move trace files.
    // TODO: for now it's OK to decompress the whole trace here because the debugger
    // does not handle streaming traces at the moment anyway, but it will have to change
    // once it does.
    const trace_content_provider: vscode.TextDocumentContentProvider = {
        async provideTextDocumentContent(uri: vscode.Uri): Promise<string> {
            try {
                return trimTraceFileContent(await decompressTraceFile(uri.path));
            } catch (err) {
                const msg = err instanceof Error ? err.message : String(err);
                return `Failed to decode trace:\n${msg}`;
            }
        }
    };
    context.subscriptions.push(
        vscode.workspace.registerTextDocumentContentProvider(TRACE_FILE_URI_SCHEME, trace_content_provider)
    );
    context.subscriptions.push(
        vscode.window.registerCustomEditorProvider(
            TRACE_CUSTOM_EDITOR_ID,
            new MoveTraceViewProvider(),
            {
                supportsMultipleEditorsPerDocument: false
            }
        )
    );

    // When opening compressed trace file in the "default" editor,
    // close the editor and open another one showing decompressed
    // content.
    vscode.workspace.onDidOpenTextDocument(async doc => {
        if (doc.uri.scheme === 'file' && doc.uri.fsPath.endsWith('.json.zst')) {
            // Close binary trace file after it was opened
            await vscode.commands.executeCommand('workbench.action.closeActiveEditor');

            // Open editor showing decompressed trace file
            const mtraceUri = vscode.Uri.parse(`${TRACE_FILE_URI_SCHEME}:${doc.uri.fsPath}`);
            const mtraceDoc = await vscode.workspace.openTextDocument(mtraceUri);
            await vscode.window.showTextDocument(mtraceDoc, { preview: false });
            vscode.commands.executeCommand('vscode.open', mtraceUri);
        }
    });
}

/**
 * Called when the extension is deactivated.
 */
export function deactivate() { }

/**
 * Custom editor provider for Move trace files.
 */
class MoveTraceViewProvider implements vscode.CustomReadonlyEditorProvider {
    async openCustomDocument(uri: vscode.Uri): Promise<vscode.CustomDocument> {
        return { uri, dispose: () => { } };
    }

    async resolveCustomEditor(
        document: vscode.CustomDocument,
        webviewPanel: vscode.WebviewPanel,
        _token: vscode.CancellationToken
    ) {
        if (document.uri.scheme === TRACE_FILE_URI_SCHEME) {
            // Do not fire custom editor for trace files already displayed
            // correctly via mtrace scheme.
            vscode.commands.executeCommand('workbench.action.closeActiveEditor');
        }

        webviewPanel.webview.options = { enableScripts: true };

        try {
            const traceContent = trimTraceFileContent(await decompressTraceFile(document.uri.fsPath));
            webviewPanel.webview.html = this.renderHtml(traceContent);
        } catch (err) {
            const msg = err instanceof Error ? err.message : String(err);
            webviewPanel.webview.html = `<pre style="color:red;">Failed to load trace: ${msg}</pre>`;
        }
    }

    private renderHtml(decodedText: string): string {
        return `
        <html>
          <body>
            <pre>${decodedText.replace(/</g, '&lt;')}</pre>
          </body>
        </html>
      `;
    }
}

/**
 * Custom configuration provider for Move debug configurations.
 */
class MoveConfigurationProvider implements vscode.DebugConfigurationProvider {

    /**
     * Massage a debug configuration just before a debug session is being launched,
     * e.g. add all missing attributes to the debug configuration.
     */
    async resolveDebugConfiguration(
        folder: WorkspaceFolder | undefined,
        config: DebugConfiguration,
        token?: CancellationToken
    ): Promise<DebugConfiguration | undefined | null> {
        // if launch.json is missing or empty
        if (!config.request && !config.name) {
            try {
                const editor = vscode.window.activeTextEditor;
                if (editor && (editor.document.languageId === MOVE_LANGUAGE_ID
                    || editor.document.languageId === TRACE_FILE_LANGUAGE_ID)) {
                    config.traceInfo = await findTraceInfo(editor);
                    config.source = '${file}';
                } else {
                    const traceViewUri = traceViewTabUri();
                    if (traceViewUri) {
                        config.traceInfo = await constructTraceInfo(traceViewUri.fsPath);
                        config.source = traceViewUri.fsPath;
                    } else if (folder) {
                        const traceFilesPattern = new vscode.RelativePattern(folder.uri.fsPath, '**/*.json.zst');
                        const traceFilePaths = await vscode.workspace.findFiles(traceFilesPattern);
                        if (traceFilePaths.length === 0) {
                            return vscode.window.showErrorMessage('No trace files found in the workspace.').then(_ => {
                                return undefined;
                            });
                        }
                        const tracePath = traceFilePaths.length === 1
                            ? traceFilePaths[0].fsPath
                            : await pickTraceFileToDebug(folder.uri.fsPath, traceFilePaths.map(file => file.fsPath));

                        if (!tracePath) {
                            return vscode.window.showErrorMessage('No trace file selected.').then(_ => {
                                return undefined;
                            });
                        }
                        config.traceInfo = await constructTraceInfo(tracePath);
                        config.source = tracePath;
                    } else {
                        throw new Error('No active editor or folder');
                    }
                }
            } catch (err) {
                const msg = err instanceof Error ? err.message : String(err);
                return vscode.window.showErrorMessage(msg).then(_ => {
                    return undefined;
                });
            }
        }

        config.type = DEBUGGER_TYPE;
        config.name = 'Launch';
        config.request = 'launch';
        config.stopOnEntry = true;
        config.logLevel = LOG_LEVEL;

        return config;
    }
}

/**
 * Finds the trace information for the current active editor. If the trace
 * was generated from a unit test, it contains a trace of a single Move
 * function execution and trace infor reflects this. If the trace
 * contains external events, it will return `undefined` as there
 * is no single function to debug (and the runtime ignores trace info
 * in this case).
 *
 * @param editor active text editor.
 * @returns trace information of the form `<package>::<module>::<function_name>`.
 * @throws Error with a descriptive error message if the trace information cannot be found.
 */
async function findTraceInfo(editor: vscode.TextEditor): Promise<string | undefined> {
    const openedFilePath = editor.document.uri.fsPath;
    let tracedFunctions: string[] = [];
    if (openedFilePath.endsWith(TRACE_FILE_EXT)) {
        const fun = await constructTraceInfo(openedFilePath);
        if (!fun) {
            return undefined;
        }
        tracedFunctions = [fun];
    } else {
        const openedFileExt = path.extname(openedFilePath);
        if (openedFileExt !== MOVE_FILE_EXT) {
            throw new Error('Unsupported file extension to start debugging '
                + `'${openedFileExt}'`
                + '(currently supporting: '
                + `'${MOVE_FILE_EXT}'`
                + ', '
                + `'${TRACE_FILE_EXT}'`
                + ')');
        }
        const pkgRoot = await findPkgRoot(openedFilePath);
        if (!pkgRoot) {
            throw new Error(`Cannot find package root for file  '${openedFilePath}'`);
        }
        const pkgModules = findSrcModules(editor.document.getText());
        if (pkgModules.length === 0) {
            throw new Error(`Cannot find any modules in file '${openedFilePath}'`);
        }
        tracedFunctions = findTracedFunctionsFromPath(pkgRoot, pkgModules);

        if (!tracedFunctions || tracedFunctions.length === 0) {
            throw new Error(`No traced functions found for package at '${pkgRoot}'`);
        }
    }
    const fun = tracedFunctions.length === 1
        ? tracedFunctions[0]
        : await pickFunctionToDebug(tracedFunctions);

    if (!fun) {
        throw new Error(`No function to be trace - debugged selected from\n` + tracedFunctions.join('\n'));
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
 * Finds modules by searching the content of a source file to look for
 * module declarations of the form `module <package>::<module>`.
 * We cannot rely on the directory structure to find modules because
 * trace info is generated based on module names in the source files.
 *
 * @param file_content content of the file.
 * @returns modules in the file content of the form `<package>::<module>`.
 */
function findSrcModules(file_content: string): string[] {
    const modulePattern = /\bmodule\s+\w+::\w+\b/g;
    const moduleSequences = file_content.match(modulePattern);
    return moduleSequences
        ? moduleSequences.map(str => str.substring('module'.length).trim())
        : [];
}

/**
 * Find all functions that have a corresponding trace file by looking at
 * the trace file names that have the following format and extracting all
 * function names that match:
 * `<package>__<module>__<function_name>.json`.
 *
 * @param pkgRoot root directory of the package.
 * @param pkgModules modules in the package of the form `<package>::<module>`.
 * @returns list of functions of the form `<package>::<module>::<function_name>`.
 * @throws Error (containing a descriptive message) if no traced functions are found for the package.
 */
function findTracedFunctionsFromPath(pkgRoot: string, pkgModules: string[]): string[] {

    const filePaths = getTraceFiles(pkgRoot);
    const result: [string, string[]][] = [];

    pkgModules.forEach((module) => {
        const prefix = module.replace(/:/g, '_') + '__';
        const prefixFiles = filePaths.filter((filePath) => filePath.startsWith(prefix));
        const suffixes = prefixFiles.map((file) => {
            const suffix = file.substring(module.length);
            if (suffix.startsWith('__') && suffix.endsWith(TRACE_FILE_EXT)) {
                return suffix.substring(2, suffix.length - TRACE_FILE_EXT.length);
            }
            return suffix;
        });
        result.push([module, suffixes]);
    });

    return result.map(([module, functionName]) => {
        return functionName.map((func) => module + "::" + func);
    }).flat();
}

/**
 * Retrieves traced function info from the trace file.
 *
 * @param traceFilePath path to the trace file.
 * @returns traced function info containing package address, module, and function itself.
 */
async function getTracedFunctionInfo(traceFilePath: string): Promise<TracedFunctionInfo> {
    const traceLines = await decompressTraceFile(traceFilePath);
    if (traceLines.length <= 1) {
        throw new Error(`Empty trace file at '${traceFilePath}`);
    }
    const firstEvent = JSON.parse(traceLines[1]);
    if (!firstEvent) {
        throw new Error(`Error parsing first trace event '${traceLines[1]}'`);
    }
    const frame = firstEvent.OpenFrame?.frame;
    const pkgAddrStrInTrace = frame?.module?.address;
    if (!pkgAddrStrInTrace) {
        throw new Error(`No package address for the initial frame in first trace event: '${firstEvent}'`);
    }
    const pkgAddrInTrace = parseInt(pkgAddrStrInTrace);
    if (isNaN(pkgAddrInTrace)) {
        throw new Error('Cannot parse package address '
            + pkgAddrStrInTrace
            + ' for the initial frame in trace file '
            + firstEvent);
    }
    const moduleInTrace = frame?.module?.name;
    if (!moduleInTrace) {
        throw new Error(`No module name for the initial frame in first trace event: '${firstEvent}'`);
    }
    const functionInTrace = frame?.function_name;
    if (!functionInTrace) {
        throw new Error(`No function name for the initial frame in first trace event: '${firstEvent}'`);
    }
    return {
        pkgAddr: pkgAddrInTrace,
        module: moduleInTrace,
        function: functionInTrace
    };
}

/**
 * Given a path to trace file generated from a unit test, constructs a string of the form
 * `<package>::<module>::<function_name>`, taking package from the trace file name
 * (module name and function are the same in the file name and in the trace itself).
 * Returns `undefined` if thrace contains external events as in this case trace info
 * is ignored by the runtime.
 *
 * @param tracePath path to the trace file.
 * @returns string of the form `<package>::<module>::<function_name>` if trace
 * file was generated from a unit test, `undefined` if the trace file contains external events.
 * @throws Error with a descriptive error message if the trace information cannot be constructed.
  *
 */
async function constructTraceInfo(tracePath: string): Promise<string | undefined> {
    if (tracePath.endsWith(TRACE_FILE_EXT) &&
        path.basename(tracePath, TRACE_FILE_EXT) === EXT_EVENTS_TRACE_FILE_NAME) {
        return undefined;
    }
    const tracedFunctionInfo = await getTracedFunctionInfo(tracePath);
    const tracedFileBaseName = path.basename(tracePath, TRACE_FILE_EXT);
    if (tracedFileBaseName === EXT_EVENTS_TRACE_FILE_NAME) {
        // it is assumed that the trace containing external events is stored
        // in a file named 'EXT_EVENTS_TRACE_FILE_NAME'.json
        return tracedFunctionInfo.pkgAddr
            + '::'
            + tracedFunctionInfo.module
            + '::'
            + tracedFunctionInfo.function;
    } else {
        // trace files containing only a single top-level Move call have the following format:
        // `<package>__<module>__<function_name>.json`
        const fileBaseNameSuffix = '__' + tracedFunctionInfo.module + '__' + tracedFunctionInfo.function;
        if (!tracedFileBaseName.endsWith(fileBaseNameSuffix)) {
            throw new Error('Trace file name (' + tracedFileBaseName + ')'
                + 'does not end with expected suffix (' + fileBaseNameSuffix + ')'
                + ' obtained from concateneting module and entry function found in the trace');
        }
        const pkgName = tracedFileBaseName.substring(0, tracedFileBaseName.length - fileBaseNameSuffix.length);
        return pkgName + '::' + tracedFunctionInfo.module + '::' + tracedFunctionInfo.function;
    }
}

/**
 * Return list of trace files for a given package.
 *
 * @param pkgRoot root directory of the package.
 * @returns list of trace files for the package.
 * @throws Error (containing a descriptive message) if no trace files are found for the package.
 */
function getTraceFiles(pkgRoot: string): string[] {
    const tracesDir = path.join(pkgRoot, 'traces');
    let filePaths = [];
    try {
        filePaths = fs.readdirSync(tracesDir);
    } catch (err) {
        throw new Error(`Error accessing 'traces' directory for package at '${pkgRoot}'`);
    }
    if (filePaths.length === 0) {
        throw new Error(`No trace files for package at ${pkgRoot}`);
    }
    return filePaths;
}

/**
 * Prompts the user to select a function to debug from a list of traced functions.
 *
 * @param tracedFunctions list of traced functions of the form `<package>::<module>::<function_name>`.
 * @returns single function to debug of the form `<package>::<module>::<function_name>`.
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

/**
 * Prompts the user to select a trace file to debug from a list of trace files.
 * @param folderPath path to the folder containing the trace files.
 * @param traceFilePaths list of trace file paths.
 * @returns single trace file to debug.
 */
async function pickTraceFileToDebug(
    folderPath: string,
    traceFilePaths: string[]
): Promise<string | undefined> {
    const traceFileSuffixes = traceFilePaths.map(filePath => {
        if (filePath.startsWith(folderPath)) {
            return filePath.slice(folderPath.length + 1);
        }
        return filePath;
    });

    const selectedSuffix = await vscode.window.showQuickPick(traceFileSuffixes, {
        canPickMany: false,
        placeHolder: 'Select a trace file to debug'
    });

    if (selectedSuffix) {
        return traceFilePaths.find(filePath => filePath.endsWith(selectedSuffix));
    }
    return undefined;
}

/**
 * Splits decompressed trace file data into lines without creating a large intermediate string.
 * This avoids hitting JavaScript's maximum string length limit for large trace files.
 *
 * @param decompressed the decompressed buffer containing trace data
 * @returns array of strings representing lines from the trace file
 */
function splitTraceFileLines(decompressed: Uint8Array): string[] {
    const NEWLINE_BYTE = 0x0A;
    const decoder = new TextDecoder();
    const lines: string[] = [];

    let lineStart = 0;

    for (let i = 0; i <= decompressed.length; i++) {
        if (i === decompressed.length || decompressed[i] === NEWLINE_BYTE) {
            // end of the buffer or a new line
            if (i > lineStart) {
                const lineBytes = decompressed.slice(lineStart, i);
                const line = decoder.decode(lineBytes).trimEnd();
                lines.push(line);
            }
            lineStart = i + 1;
        }
    }

    return lines;
}

/**
 * Reads and decompresses a trace file.
 * @param traceFilePath path to the trace file.
 * @returns decompressed trace file content as a string.
 */
async function decompressTraceFile(traceFilePath: string): Promise<string[]> {
    const buf = fs.readFileSync(traceFilePath);
    const decompressed = await decompress(buf);
    return splitTraceFileLines(decompressed);
}

/**
 * Trims the trace file content to have it fit in VSCode's
 * 50M display limit.
 * @param traceFileContent content of the trace file.
 * @returns string representation of the trace file content.
 */
function trimTraceFileContent(lines: string[]): string {
    // Max numbers of lines to display, including effects
    const maxLinesWithEffects = 1000;
    if (lines.length <= maxLinesWithEffects) {
        return lines.join('\n');
    }
    // Max number of lines to display without effects
    // (if the number of lines in the trace is greater thatn
    // maxLinesWithEffects, let's filter out the effects, but
    // display a larger number of lines to hopefully include
    // the whole trace sans effects).
    let maxLines = 10000;
    let result = "";
    for (let i = 0; i < lines.length; i++) {
        if (!lines[i].startsWith('{\"Effect\":')) {
            maxLines--;
            result += lines[i] + '\n';
        }
        if (maxLines === 0) {
            break;
        }
    }
    return result;
}

/**
 * Get the URI of the currently active trace view tab.
 * @returns uri of the active trace view tab or undefined if no such tab is active.
 */
function traceViewTabUri(): vscode.Uri | undefined {
    const activeTab = vscode.window.tabGroups.activeTabGroup.activeTab;
    if (!activeTab) {
        return undefined;
    }
    const input = activeTab.input;
    if (!input) {
        return undefined;
    }
    if (input instanceof vscode.TabInputCustom) {
        if (input.viewType === TRACE_CUSTOM_EDITOR_ID &&
            input.uri.fsPath.endsWith(TRACE_FILE_EXT)) {
            return input.uri;
        }
    }
    return undefined;
}
