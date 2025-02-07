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

const MOVE_FILE_EXT = ".move";
const BCODE_FILE_EXT = ".mvb";


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
                        const editor = vscode.window.activeTextEditor;
                        if (editor) {
                            const optimized_lines = stackTraceResponse.optimizedLines;
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
            if (editor && (editor.document.languageId === 'move'
                || editor.document.languageId === 'mvb'
                || editor.document.languageId === 'mtrace')) {

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
 * @returns trace information of the form `<package>::<module>::<function_name>`.
 * @throws Error with a descriptive error message if the trace information cannot be found.
 */
async function findTraceInfo(editor: vscode.TextEditor): Promise<string> {
    const pkgRoot = await findPkgRoot(editor.document.uri.fsPath);
    if (!pkgRoot) {
        throw new Error(`Cannot find package root for file  '${editor.document.uri.fsPath}'`);
    }

    let tracedFunctions: string[] = [];
    if (path.extname(editor.document.uri.fsPath) === MOVE_FILE_EXT) {
        const pkgModules = findSrcModules(editor.document.getText());
        if (pkgModules.length === 0) {
            throw new Error(`Cannot find any modules in file '${editor.document.uri.fsPath}'`);
        }
        tracedFunctions = findTracedFunctionsFromPath(pkgRoot, pkgModules);
    } else if (path.extname(editor.document.uri.fsPath) === BCODE_FILE_EXT) {
        const modulePattern = /\bmodule\s+\d+\.\w+\b/g;
        const moduleSequences = editor.document.getText().match(modulePattern);
        if (!moduleSequences || moduleSequences.length === 0) {
            throw new Error(`Cannot find module declaration in disassembly file '${editor.document.uri.fsPath}'`);
        }
        // there should be only one module declaration in a disassembly file
        const [pkgAddrStr, module] = moduleSequences[0].substring('module'.length).trim().split('.');
        const pkgAddr = parseInt(pkgAddrStr);
        if (isNaN(pkgAddr)) {
            throw new Error(`Cannot parse package address from '${pkgAddrStr}' in disassembly file '${editor.document.uri.fsPath}'`);
        }
        tracedFunctions = findTracedFunctionsFromTrace(pkgRoot, pkgAddr, module);
    } else {
        // this is a JSON (hopefully) trace as this function is only called if
        // the active file is either a .move, .mvb, or .json file
        const fpath = editor.document.uri.fsPath;
        const tracedFunctionInfo = getTracedFunctionInfo(fpath);
        tracedFunctions = [constructTraceInfo(fpath, tracedFunctionInfo)];
    }
    if (!tracedFunctions || tracedFunctions.length === 0) {
        throw new Error(`No traced functions found for package at '${pkgRoot}'`);
    }

    const fun = tracedFunctions.length === 1
        ? tracedFunctions[0]
        : await pickFunctionToDebug(tracedFunctions);

    if (!fun) {
        throw new Error(`No function to be trace-debugged selected from\n` + tracedFunctions.join('\n'));
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
}

/**
 * Find all functions that have a corresponding trace file by looking at
 * the content of the trace file and its name (`<package>__<module>__<function_name>.json`).
 * We need to match the package address, module name, and function name in the trace
 * file itself as this is the only place where we can find the (potentially matching)
 * package address (module name and function name could be extracted from the trace
 * file name).
 *
 * @param pkgRoot root directory of the package.
 * @param pkgAddr package address.
 * @param module module name.
 * @returns list of functions of the form `<package>::<module>::<function_name>`.
 * @throws Error (containing a descriptive message) if no traced functions are found for the package.
 */
function findTracedFunctionsFromTrace(pkgRoot: string, pkgAddr: number, module: string): string[] {
    const filePaths = getTraceFiles(pkgRoot);
    const result: string[] = [];
    for (const p of filePaths) {
        const tracePath = path.join(pkgRoot, 'traces', p);
        const tracedFunctionInfo = getTracedFunctionInfo(tracePath);
        if (tracedFunctionInfo.pkgAddr === pkgAddr && tracedFunctionInfo.module === module) {
            result.push(constructTraceInfo(tracePath, tracedFunctionInfo));
        }
    }
    return result;
}

/**
 * Retrieves traced function info from the trace file.
 *
 * @param tracePath path to the trace file.
 * @returns traced function info containing package address, module, and function itself.
 */
function getTracedFunctionInfo(tracePath: string): TracedFunctionInfo {
    let traceContent = undefined;
    try {
        traceContent = fs.readFileSync(tracePath, 'utf-8');
    } catch {
        throw new Error(`Error reading trace file '${tracePath}'`);
    }

    const trace = JSON.parse(traceContent);
    if (!trace) {
        throw new Error(`Error parsing trace file '${tracePath}'`);
    }
    if (trace.events.length === 0) {
        throw new Error(`Empty trace file '${tracePath}'`);
    }
    const frame = trace.events[0]?.OpenFrame?.frame;
    const pkgAddrStrInTrace = frame?.module?.address;
    if (!pkgAddrStrInTrace) {
        throw new Error(`No package address for the initial frame in trace file '${tracePath}'`);
    }
    const pkgAddrInTrace = parseInt(pkgAddrStrInTrace);
    if (isNaN(pkgAddrInTrace)) {
        throw new Error('Cannot parse package address '
            + pkgAddrStrInTrace
            + ' for the initial frame in trace file '
            + tracePath);
    }
    const moduleInTrace = frame?.module?.name;
    if (!moduleInTrace) {
        throw new Error(`No module name for the initial frame in trace file '${tracePath}'`);
    }
    const functionInTrace = frame?.function_name;
    if (!functionInTrace) {
        throw new Error(`No function name for the initial frame in trace file '${tracePath}'`);
    }
    return {
        pkgAddr: pkgAddrInTrace,
        module: moduleInTrace,
        function: functionInTrace
    };
}

/**
 * Given trace file path and traced function, constructs a string of the form
 * `<package>::<module>::<function_name>`, taking package from the trace file name
 * (module name and function are the same in the file name and in the trace itself).
 *
 * @param tracePath path to the trace file.
 * @param tracedFunctionInfo traced function info.
 * @returns string of the form `<package>::<module>::<function_name>`.
 */
function constructTraceInfo(tracePath: string, tracedFunctionInfo: TracedFunctionInfo): string {
    const tracedFileBaseName = path.basename(tracePath, path.extname(tracePath));
    const fileBaseNameSuffix = '__' + tracedFunctionInfo.module + '__' + tracedFunctionInfo.function;
    if (!tracedFileBaseName.endsWith(fileBaseNameSuffix)) {
        throw new Error('Trace file name (' + tracedFileBaseName + ')'
            + 'does not end with expected suffix (' + fileBaseNameSuffix + ')'
            + ' obtained from concateneting module and entry function found in the trace');
    }
    const pkgName = tracedFileBaseName.substring(0, tracedFileBaseName.length - fileBaseNameSuffix.length);
    return pkgName + '::' + tracedFunctionInfo.module + '::' + tracedFunctionInfo.function;
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
