// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { EventEmitter } from 'events';
import * as crypto from 'crypto';
import * as fs from 'fs';
import * as path from 'path';
import toml from 'toml';
import { ISourceMap, IFileInfo, readAllSourceMaps } from './source_map_utils';
import {
    TraceEffectKind,
    TraceEvent,
    TraceEventKind,
    TraceInstructionKind,
    readTrace
} from './trace_utils';
import { ModuleInfo } from './utils';

/**
 * Describes the runtime variable scope (e.g., local variables
 * or shadowed variables).
 */
export interface IRuntimeVariableScope {
    locals: (IRuntimeVariable | undefined)[];
}

/**
 * A compound type:
 * - a vector (converted to an array of values)
 * - a struct/enum (converted to an array of string/field value pairs)
 */
export type CompoundType = RuntimeValueType[] | IRuntimeCompundValue;

/**
 * A runtime value can have any of the following types:
 * - boolean, number, string (converted to string)
 * - compound type (vector, struct, enum)
 */
export type RuntimeValueType = string | CompoundType | IRuntimeRefValue;

/**
 * Locaction of a local variable in the runtime.
 */
export interface IRuntimeVariableLoc {
    frameID: number;
    localIndex: number;
}

/**
 * Value of a reference in the runtime.
 */
export interface IRuntimeRefValue {
    mutable: boolean;
    loc: IRuntimeVariableLoc
}

/**
 * Information about a runtime compound value (struct/enum).
 */
export interface IRuntimeCompundValue {
    fields: [string, RuntimeValueType][];
    type: string;
    variantName?: string;
    variantTag?: number;
}

/**
 * Describes a runtime local variable.
 */
interface IRuntimeVariable {
    name: string;
    value: RuntimeValueType;
    type: string;
}

/**
 * Describes a stack frame in the runtime and its current state
 * during trace viewing session.
 */
interface IRuntimeStackFrame {
    // Source map for the frame.
    sourceMap: ISourceMap;
    // Frame identifier.
    id: number;
    // Name of the function in this frame.
    name: string;
    // Path to the file containing the function.
    file: string;
    // Current line in the file correponding to currently viewed instruction.
    line: number; // 1-based
    // Local variable types by variable frame index.
    localsTypes: string[];
    // Local variables per scope (local scope at 0 and then following block scopes),
    // indexed by variable frame index.
    locals: (IRuntimeVariable | undefined)[][];
    /**
     * Line of the last call instruction that was processed in this frame.
     * It's needed to make sure that step/next into/over call works correctly.
     */
    lastCallInstructionLine: number | undefined;
}

/**
 * Describes the runtime stack during trace viewing session
 * (oldest frame is at the bottom of the stack at index 0).
 */
export interface IRuntimeStack {
    frames: IRuntimeStackFrame[];
}

/**
 * Events emitted by the runtime during trace viewing session.
 */
export enum RuntimeEvents {
    // Stop after step/next action is performed.
    stopOnStep = 'stopOnStep',
    // Finish trace viewing session.
    end = 'end',
}

/**
 * The runtime for viewing traces.
 */
export class Runtime extends EventEmitter {

    /**
     * Trace being viewed.
     */
    private trace = { events: [] as TraceEvent[], localLifetimeEnds: new Map<number, number[]>() };

    /**
     * Index of the current trace event being processed.
     */
    private eventIndex = 0;

    /**
     * Current frame stack.
     */
    private frameStack = { frames: [] as IRuntimeStackFrame[] };

    /**
     * Map of file hashes to file info.
     */
    private filesMap = new Map<string, IFileInfo>();

    /**x
     * Map of stringified module info to source maps.
     */
    private sourceMapsMap = new Map<string, ISourceMap>();

    /**
     * Start a trace viewing session and set up the initial state of the runtime.
     *
     * @param source  path to the Move source file whose traces are to be viewed.
     * @param traceInfo  trace selected for viewing.
     *
     */
    public async start(source: string, traceInfo: string, stopOnEntry: boolean): Promise<void> {
        const pkgRoot = await findPkgRoot(source);
        if (!pkgRoot) {
            throw new Error(`Cannot find package root for file: ${source}`);
        }
        const manifest_path = path.join(pkgRoot, 'Move.toml');

        // find package name from manifest file which corresponds `build` directory's subdirectory
        // name containing this package's build files
        const pkg_name = getPkgNameFromManifest(manifest_path);
        if (!pkg_name) {
            throw Error(`Cannot find package name in manifest file: ${manifest_path}`);
        }

        // create file maps for all files in the `build` directory, including both package source
        // files and source files for dependencies
        hashToFileMap(path.join(pkgRoot, 'build', pkg_name, 'sources'), this.filesMap);
        // update with files from the actual "sources" directory rather than from the "build" directory
        hashToFileMap(path.join(pkgRoot, 'sources'), this.filesMap);

        // create source maps for all modules in the `build` directory
        this.sourceMapsMap = readAllSourceMaps(path.join(pkgRoot, 'build', pkg_name, 'source_maps'), this.filesMap);

        // reconstruct trace file path from trace info
        const traceFilePath = path.join(pkgRoot, 'traces', traceInfo.replace(/:/g, '_') + '.json');
        this.trace = readTrace(traceFilePath);

        // start trace viewing session with the first trace event
        this.eventIndex = 0;

        // setup frame stack with the first frame
        const currentEvent = this.trace.events[this.eventIndex];
        if (currentEvent.type !== TraceEventKind.OpenFrame) {
            throw new Error(`First event in trace is not an OpenFrame event`);
        }
        const newFrame =
            this.newStackFrame(
                currentEvent.id,
                currentEvent.name,
                currentEvent.modInfo,
                currentEvent.localsTypes
            );
        this.frameStack = {
            frames: [newFrame]
        };
        this.step(/* next */ false, /* stopAtCloseFrame */ false);
    }

    /**
     * Handles "get current stack" adapter action.
     *
     * @returns current frame stack.
     */
    public stack(): IRuntimeStack {
        return this.frameStack;
    }

    /**
     * Handles step/next adapter action.
     *
     * @param next determines if it's `next` (or otherwise `step`) action.
     * @param stopAtCloseFrame determines if the action should stop at `CloseFrame` event
     * (rather then proceedint to the following instruction).
     * @returns `true` if the trace viewing session is finished, `false` otherwise.
     * @throws Error with a descriptive error message if the step event cannot be handled.
     */
    public step(next: boolean, stopAtCloseFrame: boolean): boolean {
        this.eventIndex++;
        if (this.eventIndex >= this.trace.events.length) {
            this.sendEvent(RuntimeEvents.stopOnStep);
            return true;
        }
        let currentEvent = this.trace.events[this.eventIndex];
        if (currentEvent.type === TraceEventKind.Instruction) {
            const stackHeight = this.frameStack.frames.length;
            if (stackHeight <= 0) {
                throw new Error('No frame on the stack when processing Instruction event at PC: '
                    + currentEvent.pc);
            }
            const currentFrame = this.frameStack.frames[stackHeight - 1];
            // remember last call instruction line before it (potentially) changes
            // in the `instruction` call below
            const lastCallInstructionLine = currentFrame.lastCallInstructionLine;
            let [sameLine, currentLine] = this.instruction(currentFrame, currentEvent);
            if (sameLine) {
                if (!next && (currentEvent.kind === TraceInstructionKind.CALL
                    || currentEvent.kind === TraceInstructionKind.CALL_GENERIC)
                    && lastCallInstructionLine === currentLine) {
                    // We are about to step into another call on the same line
                    // but we should wait for user action to do so rather than
                    // having debugger step into it automatically. If we don't
                    // the user will observe a weird effect. For example,
                    // consider the following code:
                    // ```
                    // foo();
                    // assert(bar() == baz());
                    // ```
                    // In the code above, after executing `foo()`, the user
                    // will move to the next line and will expect to only
                    // step into `bar` rather than having debugger to step
                    // immediately into `baz` as well. At the same time,
                    // if the user intended to step over functions using `next`,
                    // we shuld skip over all calls on the same line (both `bar`
                    // and `baz` in the example above).
                    //
                    // The following explains a bit more formally what needs
                    // to happen both on on `next` and `step` actions when
                    // call and non-call instructions are interleaved:
                    //
                    // When `step` is called:
                    //
                    // When there is only one call on the same line, we want to
                    // stop on the first instruction of this line, then after
                    // user `step` action enter the call, and then after
                    // exiting the call go to the instruction on the next line:
                    // 6: instruction
                    // 7: instruction       // stop here
                    // 7: call              // enter call here
                    // 7: instruction
                    // 8: instruction       // stop here
                    //
                    // When there is more than one call on the same line, we
                    // want to stop on the first instruction of this line,
                    // then after user `step` action enter the call, then
                    // after exiting the call stop on the next call instruction
                    // and waitl for another `step` action from the user:
                    // 6: instruction
                    // 7: instruction       // stop here
                    // 7: call              // enter call here
                    // 7: instruction
                    // 7: call              // stop and then enter call here
                    // 7: instruction
                    // 8: instruction       // stop here
                    //
                    // When `next` is called, things have to happen differently,
                    // particularly when there are multiple calls on the same line:
                    // 6: instruction
                    // 7: instruction       // stop here
                    // 7: call
                    // 7: instruction
                    // 7: call
                    // 7: instruction
                    // 8: instruction       // stop here
                    //
                    // To support this, we need to keep track of the line number when
                    // the last call instruction in a give frame happened, and
                    // also we need to make `stepOut` aware of whether it is executed
                    // as part of `next` (which is how `next` is implemented) or not.
                    this.sendEvent(RuntimeEvents.stopOnStep);
                    return false;
                } else {
                    return this.step(next, stopAtCloseFrame);
                }
            }
            this.sendEvent(RuntimeEvents.stopOnStep);
            return false;
        } else if (currentEvent.type === TraceEventKind.OpenFrame) {
            // create a new frame and push it onto the stack
            const newFrame =
                this.newStackFrame(
                    currentEvent.id,
                    currentEvent.name,
                    currentEvent.modInfo,
                    currentEvent.localsTypes
                );
            // set values of parameters in the new frame
            this.frameStack.frames.push(newFrame);
            for (let i = 0; i < currentEvent.paramValues.length; i++) {
                localWrite(newFrame, i, currentEvent.paramValues[i]);
            }

            if (next) {
                // step out of the frame right away
                this.stepOut(next);
                return false;
            } else {
                return this.step(next, stopAtCloseFrame);
            }
        } else if (currentEvent.type === TraceEventKind.CloseFrame) {
            if (stopAtCloseFrame) {
                // don't do anything as the caller needs to inspect
                // the event before proceeing
                return false;
            } else {
                // pop the top frame from the stack
                if (this.frameStack.frames.length <= 0) {
                    throw new Error('No frame to pop at CloseFrame event with ID: '
                        + currentEvent.id);
                }
                this.frameStack.frames.pop();
                return this.step(next, stopAtCloseFrame);
            }
        } else if (currentEvent.type === TraceEventKind.Effect) {
            const effect = currentEvent.effect;
            if (effect.type === TraceEffectKind.Write) {
                const traceLocation = effect.loc;
                const traceValue = effect.value;
                const frame = this.frameStack.frames.find(
                    frame => frame.id === traceLocation.frameID
                );
                if (!frame) {
                    throw new Error('Cannot find frame with ID: '
                        + traceLocation.frameID
                        + ' when processing Write effect for local variable at index: '
                        + traceLocation.localIndex);
                }
                localWrite(frame, traceLocation.localIndex, traceValue);
            }
            return this.step(next, stopAtCloseFrame);
        } else {
            // ignore other events
            return this.step(next, stopAtCloseFrame);
        }
    }

    /**
     * Handles "step out" adapter action.
     *
     * @param next determines if it's  part of `next` (or otherwise `step`) action.
     * @returns `true` if was able to step out of the frame, `false` otherwise.
     * @throws Error with a descriptive error message if the step out event cannot be handled.
     */
    public stepOut(next: boolean): boolean {
        const stackHeight = this.frameStack.frames.length;
        if (stackHeight <= 1) {
            // do nothing as there is no frame to step out to
            this.sendEvent(RuntimeEvents.stopOnStep);
            return false;
        }
        // newest frame is at the top of the stack
        const currentFrame = this.frameStack.frames[stackHeight - 1];
        let currentEvent = this.trace.events[this.eventIndex];
        // skip all events until the corresponding CloseFrame event,
        // pop the top frame from the stack, and proceed to the next event
        while (true) {
            // when calling `step` in the loop below, we need to avoid
            // skipping over calls next-style otherwise we can miss seeing
            // the actual close frame event that we are looking for
            // and have the loop execute too far
            if (this.step(/* next */ false, /* stopAtCloseFrame */ true)) {
                // trace viewing session finished
                throw new Error('Cannot find corresponding CloseFrame event for function: ' +
                    currentFrame.name);
            }
            currentEvent = this.trace.events[this.eventIndex];
            if (currentEvent.type === TraceEventKind.CloseFrame) {
                const currentFrameID = currentFrame.id;
                // `step` call finished at the CloseFrame event
                // but did not process it so we need pop the frame here
                this.frameStack.frames.pop();
                if (currentEvent.id === currentFrameID) {
                    break;
                }
            }
        }
        return this.step(next, /* stopAtCloseFrame */ false);
    }

    /**
     * Handles "continue" adapter action.
     * @returns `true` if the trace viewing session is finished, `false` otherwise.
     * @throws Error with a descriptive error message if the continue event cannot be handled.
     */
    public continue(): boolean {
        while (true) {
            if (this.step(/* next */ false, /* stopAtCloseFrame */ false)) {
                return true;
            }
        }
    }

    /**
     * Handles `Instruction` trace event which represents instruction in the current stack frame.
     *
     * @param instructionEvent `Instruction` trace event.
     * @returns `true` if the instruction is on the same line as the one in the current frame,
     * `false` otherwise (so that instructions on the same line can be skipped).
     * @throws Error with a descriptive error message if instruction event cannot be handled.
     */
    private instruction(
        currentFrame: IRuntimeStackFrame,
        instructionEvent: Extract<TraceEvent, { type: TraceEventKind.Instruction }>
    ): [boolean, number] {
        const currentFun = currentFrame.sourceMap.functions.get(currentFrame.name);
        if (!currentFun) {
            throw new Error(`Cannot find function: ${currentFrame.name} in source map`);
        }

        // if map does not contain an entry for a PC that can be found in the trace file,
        // it means that the position of the last PC in the source map should be used
        let currentPCLoc = instructionEvent.pc >= currentFun.pcLocs.length
            ? currentFun.pcLocs[currentFun.pcLocs.length - 1]
            : currentFun.pcLocs[instructionEvent.pc];

        if (!currentPCLoc) {
            throw new Error('Cannot find location for PC: '
                + instructionEvent.pc
                + ' in function: '
                + currentFrame.name);
        }

        // if current instruction ends lifetime of a local variable, mark this in the
        // local variable array
        const frameLocalLifetimeEnds = this.trace.localLifetimeEnds.get(currentFrame.id);
        if (frameLocalLifetimeEnds) {
            const localsLength = currentFrame.locals.length;
            for (let i = 0; i < localsLength; i++) {
                for (let j = 0; j < currentFrame.locals[i].length; j++) {
                    if (frameLocalLifetimeEnds[j] === instructionEvent.pc) {
                        currentFrame.locals[i][j] = undefined;
                    }
                }
            }
            // trim shadowed scopes that have no live variables in them
            for (let i = localsLength - 1; i > 0; i--) {
                const liveVar = currentFrame.locals[i].find(runtimeVar => {
                    return runtimeVar !== undefined;
                });
                if (!liveVar) {
                    currentFrame.locals.pop();
                }
            }
        }

        if (instructionEvent.kind === TraceInstructionKind.CALL ||
            instructionEvent.kind === TraceInstructionKind.CALL_GENERIC) {
            currentFrame.lastCallInstructionLine = currentPCLoc.line;
        }

        if (currentPCLoc.line === currentFrame.line) {
            // so that instructions on the same line can be bypassed
            return [true, currentPCLoc.line];
        } else {
            currentFrame.line = currentPCLoc.line;
            return [false, currentPCLoc.line];
        }
    }


    /**
     * Creates a new runtime stack frame based on info from the `OpenFrame` trace event.
     *
     * @param frameID frame identifier from the trace event.
     * @param funName function name.
     * @param modInfo information about module containing the function.
     * @param localsTypes types of local variables in the frame.
     * @returns new frame.
     * @throws Error with a descriptive error message if frame cannot be constructed.
     */
    private newStackFrame(
        frameID: number,
        funName: string,
        modInfo: ModuleInfo,
        localsTypes: string[]
    ): IRuntimeStackFrame {
        const sourceMap = this.sourceMapsMap.get(JSON.stringify(modInfo));

        if (!sourceMap) {
            throw new Error('Cannot find source map for module: '
                + modInfo.name
                + ' in package: '
                + modInfo.addr);
        }
        const currentFile = this.filesMap.get(sourceMap.fileHash);

        if (!currentFile) {
            throw new Error(`Cannot find file with hash: ${sourceMap.fileHash}`);
        }

        let locals = [];
        // create first scope for local variables
        locals[0] = [];
        const stackFrame: IRuntimeStackFrame = {
            sourceMap,
            id: frameID,
            name: funName,
            file: currentFile.path,
            line: 0, // line will be updated when next event (Instruction) is processed
            localsTypes,
            locals,
            lastCallInstructionLine: undefined,
        };

        if (this.trace.events.length <= this.eventIndex + 1 ||
            this.trace.events[this.eventIndex + 1].type !== TraceEventKind.Instruction) {
            throw new Error('Expected an Instruction event after OpenFrame event');
        }
        return stackFrame;
    }

    /**
     * Emits an event to the adapter.
     *
     * @param event string representing the event.
     * @param args optional arguments to be passed with the event.
     */
    private sendEvent(event: string, ...args: any[]): void {
        setTimeout(() => {
            this.emit(event, ...args);
        }, 0);
    }

    //
    // Utility functions for testing and debugging.
    //

    /**
     * Whitespace used for indentation in the string representation of the runtime.
     */
    private singleTab = '  ';

    /**
     * Returns a string representig the current state of the runtime.
     *
     * @returns string representation of the runtime.
     */
    public toString(): string {
        let res = 'current frame stack:\n';
        for (const frame of this.frameStack.frames) {
            res += this.singleTab + 'function: ' + frame.name + ' (line ' + frame.line + ')\n';
            for (let i = 0; i < frame.locals.length; i++) {
                res += this.singleTab + this.singleTab + 'scope ' + i + ' :\n';
                for (let j = 0; j < frame.locals[i].length; j++) {
                    const local = frame.locals[i][j];
                    if (local) {
                        res += this.varToString(this.singleTab
                            + this.singleTab
                            + this.singleTab, local) + '\n';
                    }
                }
            }
        }
        return res;
    }
    /**
     * Returns a string representation of a runtime variable.
     *
     * @param variable runtime variable.
     * @returns string representation of the variable.
     */
    private varToString(tabs: string, variable: IRuntimeVariable): string {
        return this.valueToString(tabs, variable.value, variable.name, variable.type);
    }

    /**
     * Returns a string representation of a runtime compound value.
     *
     * @param compoundValue runtime compound value.
     * @returns string representation of the compound value.
     */
    private compoundValueToString(tabs: string, compoundValue: IRuntimeCompundValue): string {
        const type = compoundValue.variantName
            ? compoundValue.type + '::' + compoundValue.variantName
            : compoundValue.type;
        let res = '(' + type + ') {\n';
        for (const [name, value] of compoundValue.fields) {
            res += this.valueToString(tabs + this.singleTab, value, name);
        }
        res += tabs + '}\n';
        return res;
    }

    /**
     * Returns a string representation of a runtime reference value.
     *
     * @param refValue runtime reference value.
     * @param name name of the variable containing reference value.
     * @param type optional type of the variable containing reference value.
     * @returns string representation of the reference value.
     */
    private refValueToString(
        tabs: string,
        refValue: IRuntimeRefValue,
        name: string,
        type?: string
    ): string {
        let res = '';
        const frame = this.frameStack.frames.find(frame => frame.id === refValue.loc.frameID);
        let local = undefined;
        if (!frame) {
            return res;
        }
        for (const scope of frame.locals) {
            local = scope[refValue.loc.localIndex];
            if (local) {
                break;
            }
        }
        if (!local) {
            return res;
        }
        return this.valueToString(tabs, local.value, name, type);
    }

    /**
     * Returns a string representation of a runtime value.
     *
     * @param value runtime value.
     * @param name name of the variable containing the value.
     * @param type optional type of the variable containing the value.
     * @returns string representation of the value.
     */
    private valueToString(
        tabs: string,
        value: RuntimeValueType,
        name: string,
        type?: string
    ): string {
        let res = '';
        if (typeof value === 'string') {
            res += tabs + name + ' : ' + value + '\n';
            if (type) {
                res += tabs + 'type: ' + type + '\n';
            }
        } else if (Array.isArray(value)) {
            res += tabs + name + ' : [\n';
            for (let i = 0; i < value.length; i++) {
                res += this.valueToString(tabs + this.singleTab, value[i], String(i));
            }
            res += tabs + ']\n';
            if (type) {
                res += tabs + 'type: ' + type + '\n';
            }
            return res;
        } else if ('fields' in value) {
            res += tabs + name + ' : ' + this.compoundValueToString(tabs, value);
            if (type) {
                res += tabs + 'type: ' + type + '\n';
            }
        } else {
            res += this.refValueToString(tabs, value, name, type);
        }
        return res;
    }
}

/**
 * Handles a write to a local variable in a stack frame.
 *
 * @param frame stack frame frame.
 * @param localIndex variable index in the frame.
 * @param runtimeValue variable value.
 */
function localWrite(
    frame: IRuntimeStackFrame,
    localIndex: number,
    value: RuntimeValueType
): void {
    const type = frame.localsTypes[localIndex];
    if (!type) {
        throw new Error('Cannot find type for local variable at index: '
            + localIndex
            + ' in function: '
            + frame.name);
    }
    const funEntry = frame.sourceMap.functions.get(frame.name);
    if (!funEntry) {
        throw new Error('Cannot find function entry in source map for function: '
            + frame.name);
    }
    const name = funEntry.localsNames[localIndex];
    if (!name) {
        throw new Error('Cannot find local variable at index: '
            + localIndex
            + ' in function: '
            + frame.name);
    }

    const scopesCount = frame.locals.length;
    if (scopesCount <= 0) {
        throw new Error("There should be at least one variable scope in functon"
            + frame.name);
    }
    // If a variable has the same name but a different index (it is shadowed)
    // it has to be put in a different scope (e.g., locals[1], locals[2], etc.).
    // Find scope already containing variable name, if any, starting from
    // the outermost one
    let existingVarScope = -1;
    for (let i = scopesCount - 1; i >= 0; i--) {
        const existingVarIndex = frame.locals[i].findIndex(runtimeVar => {
            return runtimeVar && runtimeVar.name === name;
        });
        if (existingVarIndex !== -1 && existingVarIndex !== localIndex) {
            existingVarScope = i;
            break;
        }
    }
    if (existingVarScope >= 0) {
        const shadowedScope = frame.locals[existingVarScope + 1];
        if (!shadowedScope) {
            frame.locals.push([]);
        }
        frame.locals[existingVarScope + 1][localIndex] = { name, value, type };
    } else {
        // put variable in the "main" locals scope
        frame.locals[0][localIndex] = { name, value, type };
    }
}

/**
 * Finds the root directory of the package containing the active file.
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
 * Find the package name in the manifest file.
 *
 * @param pkgRoot root directory of the package.
 * @returns package name.
 */
function getPkgNameFromManifest(pkgRoot: string): string | undefined {
    const manifest = fs.readFileSync(pkgRoot, 'utf8');
    const parsedManifest = toml.parse(manifest);
    const packageName = parsedManifest.package.name;
    return packageName;
}

/**
 * Creates a map from a file hash to file information for all Move source files in a directory.
 *
 * @param directory path to the directory containing Move source files.
 * @param filesMap map to update with file information.
 */
function hashToFileMap(directory: string, filesMap: Map<string, IFileInfo>): void {
    const processDirectory = (dir: string) => {
        const files = fs.readdirSync(dir);
        for (const f of files) {
            const filePath = path.join(dir, f);
            const stats = fs.statSync(filePath);
            if (stats.isDirectory()) {
                processDirectory(filePath);
            } else if (path.extname(f) === '.move') {
                const content = fs.readFileSync(filePath, 'utf8');
                const hash = fileHash(content);
                const lines = content.split('\n');
                const fileInfo = { path: filePath, content, lines };
                filesMap.set(Buffer.from(hash).toString('base64'), fileInfo);
            }
        }
    };

    processDirectory(directory);
}

/**
 * Computes the SHA-256 hash of a file's contents.
 *
 * @param fileContents contents of the file.
 */
function fileHash(fileContents: string): Uint8Array {
    const hash = crypto.createHash('sha256').update(fileContents).digest();
    return new Uint8Array(hash);
}

