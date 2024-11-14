"use strict";
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || function (mod) {
    if (mod && mod.__esModule) return mod;
    var result = {};
    if (mod != null) for (var k in mod) if (k !== "default" && Object.prototype.hasOwnProperty.call(mod, k)) __createBinding(result, mod, k);
    __setModuleDefault(result, mod);
    return result;
};
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.Runtime = exports.ExecutionResult = exports.RuntimeEvents = void 0;
const events_1 = require("events");
const crypto = __importStar(require("crypto"));
const fs = __importStar(require("fs"));
const path = __importStar(require("path"));
const toml_1 = __importDefault(require("toml"));
const source_map_utils_1 = require("./source_map_utils");
const trace_utils_1 = require("./trace_utils");
/**
 * Events emitted by the runtime during trace viewing session.
 */
var RuntimeEvents;
(function (RuntimeEvents) {
    /**
     *  Stop after step/next action is performed.
     */
    RuntimeEvents["stopOnStep"] = "stopOnStep";
    /**
     * Stop after a line breakpoint is hit.
     */
    RuntimeEvents["stopOnLineBreakpoint"] = "stopOnLineBreakpoint";
    /**
     * Stop after exception has been encountered.
     */
    RuntimeEvents["stopOnException"] = "stopOnException";
    /**
     *  Finish trace viewing session.
     */
    RuntimeEvents["end"] = "end";
})(RuntimeEvents || (exports.RuntimeEvents = RuntimeEvents = {}));
/**
 * Describes result of the execution.
 */
var ExecutionResult;
(function (ExecutionResult) {
    ExecutionResult[ExecutionResult["Ok"] = 0] = "Ok";
    ExecutionResult[ExecutionResult["TraceEnd"] = 1] = "TraceEnd";
    ExecutionResult[ExecutionResult["Exception"] = 2] = "Exception";
})(ExecutionResult || (exports.ExecutionResult = ExecutionResult = {}));
/**
 * The runtime for viewing traces.
 */
class Runtime extends events_1.EventEmitter {
    /**
     * Trace being viewed.
     */
    trace = {
        events: [],
        localLifetimeEnds: new Map(),
        tracedLines: new Map()
    };
    /**
     * Index of the current trace event being processed.
     */
    eventIndex = 0;
    /**
     * Current frame stack.
     */
    frameStack = { frames: [] };
    /**
     * Map of file hashes to file info.
     */
    filesMap = new Map();
    /**
     * Map of line breakpoints, keyed on a file path.
     */
    lineBreakpoints = new Map();
    /**
     * Start a trace viewing session and set up the initial state of the runtime.
     *
     * @param source  path to the Move source file whose traces are to be viewed.
     * @param traceInfo  trace selected for viewing.
     * @throws Error with a descriptive error message if starting runtime has failed.
     *
     */
    async start(source, traceInfo, stopOnEntry) {
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
        this.hashToFileMap(path.join(pkgRoot, 'build', pkg_name, 'sources'));
        // update with files from the actual "sources" directory rather than from the "build" directory
        this.hashToFileMap(path.join(pkgRoot, 'sources'));
        // create source maps for all modules in the `build` directory
        const sourceMapsModMap = (0, source_map_utils_1.readAllSourceMaps)(path.join(pkgRoot, 'build', pkg_name, 'source_maps'), this.filesMap);
        // reconstruct trace file path from trace info
        const traceFilePath = path.join(pkgRoot, 'traces', traceInfo.replace(/:/g, '_') + '.json');
        // create a mapping from file hash to its corresponding source map
        const sourceMapsHashMap = new Map;
        for (const [_, sourceMap] of sourceMapsModMap) {
            sourceMapsHashMap.set(sourceMap.fileHash, sourceMap);
        }
        this.trace = (0, trace_utils_1.readTrace)(traceFilePath, sourceMapsModMap, sourceMapsHashMap, this.filesMap);
        // start trace viewing session with the first trace event
        this.eventIndex = 0;
        // setup frame stack with the first frame
        const currentEvent = this.trace.events[this.eventIndex];
        if (currentEvent.type !== trace_utils_1.TraceEventKind.OpenFrame) {
            throw new Error(`First event in trace is not an OpenFrame event`);
        }
        const newFrame = this.newStackFrame(currentEvent.id, currentEvent.name, currentEvent.fileHash, currentEvent.localsTypes, currentEvent.localsNames, currentEvent.optimizedLines);
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
    stack() {
        return this.frameStack;
    }
    /**
     * Handles step/next adapter action.
     *
     * @param next determines if it's `next` (or otherwise `step`) action.
     * @param stopAtCloseFrame determines if the action should stop at `CloseFrame` event
     * (rather then proceed to the following instruction).
     * @returns ExecutionResult.Ok if the step action was successful, ExecutionResult.TraceEnd if we
     * reached the end of the trace, and ExecutionResult.Exception if an exception was encountered.
     * @throws Error with a descriptive error message if the step event cannot be handled.
     */
    step(next, stopAtCloseFrame) {
        this.eventIndex++;
        if (this.eventIndex >= this.trace.events.length) {
            this.sendEvent(RuntimeEvents.stopOnStep);
            return ExecutionResult.TraceEnd;
        }
        let currentEvent = this.trace.events[this.eventIndex];
        if (currentEvent.type === trace_utils_1.TraceEventKind.Instruction) {
            const stackHeight = this.frameStack.frames.length;
            if (stackHeight <= 0) {
                throw new Error('No frame on the stack when processing Instruction event on line: '
                    + currentEvent.loc.line
                    + ' in column: '
                    + currentEvent.loc.column);
            }
            const currentFrame = this.frameStack.frames[stackHeight - 1];
            // remember last call instruction line before it (potentially) changes
            // in the `instruction` call below
            const lastCallInstructionLine = currentFrame.lastCallInstructionLine;
            let [sameLine, currentLine] = this.instruction(currentFrame, currentEvent);
            // do not attempt to skip events on the same line if the previous event
            // was a switch to/from an inlined frame - we want execution to stop before
            // the first instruction of the inlined frame is processed
            const prevEvent = this.trace.events[this.eventIndex - 1];
            sameLine = sameLine &&
                !(prevEvent.type === trace_utils_1.TraceEventKind.ReplaceInlinedFrame
                    || prevEvent.type === trace_utils_1.TraceEventKind.OpenFrame && prevEvent.id < 0
                    || prevEvent.type === trace_utils_1.TraceEventKind.CloseFrame && prevEvent.id < 0);
            if (sameLine) {
                if (!next && (currentEvent.kind === trace_utils_1.TraceInstructionKind.CALL
                    || currentEvent.kind === trace_utils_1.TraceInstructionKind.CALL_GENERIC)
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
                    return ExecutionResult.Ok;
                }
                else {
                    return this.step(next, stopAtCloseFrame);
                }
            }
            this.sendEvent(RuntimeEvents.stopOnStep);
            return ExecutionResult.Ok;
        }
        else if (currentEvent.type === trace_utils_1.TraceEventKind.ReplaceInlinedFrame) {
            let currentFrame = this.frameStack.frames.pop();
            if (!currentFrame) {
                throw new Error('No frame to pop when processing `ReplaceInlinedFrame` event');
            }
            currentFrame.fileHash = currentEvent.fileHash;
            currentFrame.optimizedLines = currentEvent.optimizedLines;
            const currentFile = this.filesMap.get(currentFrame.fileHash);
            if (!currentFile) {
                throw new Error('Cannot find file with hash '
                    + currentFrame.fileHash
                    + ' when processing `ReplaceInlinedFrame` event');
            }
            currentFrame.file = currentFile.path;
            this.frameStack.frames.push(currentFrame);
            return this.step(next, stopAtCloseFrame);
        }
        else if (currentEvent.type === trace_utils_1.TraceEventKind.OpenFrame) {
            // if function is native then the next event will be CloseFrame
            if (currentEvent.isNative) {
                // see if naitve function aborted
                if (this.trace.events.length > this.eventIndex + 1) {
                    const nextEvent = this.trace.events[this.eventIndex + 1];
                    if (nextEvent.type === trace_utils_1.TraceEventKind.Effect &&
                        nextEvent.effect.type === trace_utils_1.TraceEffectKind.ExecutionError) {
                        this.sendEvent(RuntimeEvents.stopOnException, nextEvent.effect.msg);
                        return ExecutionResult.Exception;
                    }
                }
                // if native functino executed successfully, then the next event
                // should be CloseFrame
                if (this.trace.events.length <= this.eventIndex + 1 ||
                    this.trace.events[this.eventIndex + 1].type !== trace_utils_1.TraceEventKind.CloseFrame) {
                    throw new Error('Expected an CloseFrame event after native OpenFrame event');
                }
                // skip over CloseFrame as there is no frame to pop
                this.eventIndex++;
                return this.step(next, stopAtCloseFrame);
            }
            // create a new frame and push it onto the stack
            const newFrame = this.newStackFrame(currentEvent.id, currentEvent.name, currentEvent.fileHash, currentEvent.localsTypes, currentEvent.localsNames, currentEvent.optimizedLines);
            // set values of parameters in the new frame
            this.frameStack.frames.push(newFrame);
            for (let i = 0; i < currentEvent.paramValues.length; i++) {
                localWrite(newFrame, i, currentEvent.paramValues[i]);
            }
            if (next) {
                // step out of the frame right away
                return this.stepOut(next);
            }
            else {
                return this.step(next, stopAtCloseFrame);
            }
        }
        else if (currentEvent.type === trace_utils_1.TraceEventKind.CloseFrame) {
            if (stopAtCloseFrame) {
                // don't do anything as the caller needs to inspect
                // the event before proceeing
                return ExecutionResult.Ok;
            }
            else {
                // pop the top frame from the stack
                if (this.frameStack.frames.length <= 0) {
                    throw new Error('No frame to pop at CloseFrame event with ID: '
                        + currentEvent.id);
                }
                this.frameStack.frames.pop();
                return this.step(next, stopAtCloseFrame);
            }
        }
        else if (currentEvent.type === trace_utils_1.TraceEventKind.Effect) {
            const effect = currentEvent.effect;
            if (effect.type === trace_utils_1.TraceEffectKind.ExecutionError) {
                this.sendEvent(RuntimeEvents.stopOnException, effect.msg);
                return ExecutionResult.Exception;
            }
            if (effect.type === trace_utils_1.TraceEffectKind.Write) {
                const traceLocation = effect.loc;
                const traceValue = effect.value;
                const frame = this.frameStack.frames.find(frame => frame.id === traceLocation.frameID);
                if (!frame) {
                    throw new Error('Cannot find frame with ID: '
                        + traceLocation.frameID
                        + ' when processing Write effect for local variable at index: '
                        + traceLocation.localIndex);
                }
                localWrite(frame, traceLocation.localIndex, traceValue);
            }
            return this.step(next, stopAtCloseFrame);
        }
        else {
            // ignore other events
            return this.step(next, stopAtCloseFrame);
        }
    }
    /**
     * Handles "step out" adapter action.
     *
     * @param next determines if it's  part of `next` (or otherwise `step`) action.
     * @returns ExecutionResult.Ok if the step action was successful, ExecutionResult.TraceEnd if we
     * reached the end of the trace, and ExecutionResult.Exception if an exception was encountered.
     * @throws Error with a descriptive error message if the step out event cannot be handled.
     */
    stepOut(next) {
        const stackHeight = this.frameStack.frames.length;
        if (stackHeight <= 1) {
            // do nothing as there is no frame to step out to
            this.sendEvent(RuntimeEvents.stopOnStep);
            return ExecutionResult.Ok;
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
            const executionResult = this.step(/* next */ false, /* stopAtCloseFrame */ true);
            if (executionResult === ExecutionResult.Exception) {
                return executionResult;
            }
            if (executionResult === ExecutionResult.TraceEnd) {
                throw new Error('Cannot find corresponding CloseFrame event for function: ' +
                    currentFrame.name);
            }
            currentEvent = this.trace.events[this.eventIndex];
            if (currentEvent.type === trace_utils_1.TraceEventKind.CloseFrame) {
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
     * @returns ExecutionResult.Ok if the step action was successful, ExecutionResult.TraceEnd if we
     * reached the end of the trace, and ExecutionResult.Exception if an exception was encountered.
     * @throws Error with a descriptive error message if the continue event cannot be handled.
     */
    continue() {
        while (true) {
            const executionResult = this.step(/* next */ false, /* stopAtCloseFrame */ false);
            if (executionResult === ExecutionResult.TraceEnd ||
                executionResult === ExecutionResult.Exception) {
                return executionResult;
            }
            let currentEvent = this.trace.events[this.eventIndex];
            if (currentEvent.type === trace_utils_1.TraceEventKind.Instruction) {
                const stackHeight = this.frameStack.frames.length;
                if (stackHeight <= 0) {
                    throw new Error('No frame on the stack when processing Instruction event on line: '
                        + currentEvent.loc.line
                        + ' in column: '
                        + currentEvent.loc.column);
                }
                const currentFrame = this.frameStack.frames[stackHeight - 1];
                const breakpoints = this.lineBreakpoints.get(currentFrame.file);
                if (!breakpoints) {
                    continue;
                }
                if (breakpoints.has(currentEvent.loc.line)) {
                    this.sendEvent(RuntimeEvents.stopOnLineBreakpoint);
                    return ExecutionResult.Ok;
                }
            }
        }
    }
    /**
     * Sets line breakpoints for a file (resetting any existing ones).
     *
     * @param path file path.
     * @param lines breakpoints lines.
     * @returns array of booleans indicating if a breakpoint was set on a line.
     * @throws Error with a descriptive error message if breakpoints cannot be set.
     */
    setLineBreakpoints(path, lines) {
        const breakpoints = new Set();
        const tracedLines = this.trace.tracedLines.get(path);
        // Set all breakpoints to invalid and validate the correct ones in the loop,
        // otherwise let them all be invalid if there are no traced lines.
        // Valid breakpoints are those that are on lines that have at least
        // one instruction in the trace on them.
        const validated = lines.map(() => false);
        if (tracedLines) {
            for (let i = 0; i < lines.length; i++) {
                if (tracedLines.has(lines[i])) {
                    validated[i] = true;
                    breakpoints.add(lines[i]);
                }
            }
        }
        this.lineBreakpoints.set(path, breakpoints);
        return validated;
    }
    /**
     * Handles `Instruction` trace event which represents instruction in the current stack frame.
     *
     * @param instructionEvent `Instruction` trace event.
     * @returns `true` if the instruction is on the same line as the one in the current frame,
     * `false` otherwise (so that instructions on the same line can be skipped).
     * @throws Error with a descriptive error message if instruction event cannot be handled.
     */
    instruction(currentFrame, instructionEvent) {
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
        const loc = instructionEvent.loc;
        if (instructionEvent.kind === trace_utils_1.TraceInstructionKind.CALL ||
            instructionEvent.kind === trace_utils_1.TraceInstructionKind.CALL_GENERIC) {
            currentFrame.lastCallInstructionLine = loc.line;
        }
        if (loc.line === currentFrame.line) {
            // so that instructions on the same line can be bypassed
            return [true, loc.line];
        }
        else {
            currentFrame.line = loc.line;
            return [false, loc.line];
        }
    }
    /**
     * Creates a new runtime stack frame based on info from the `OpenFrame` trace event.
     *
     * @param frameID frame identifier from the trace event.
     * @param funName function name.
     * @param modInfo information about module containing the function.
     * @param localsTypes types of local variables in the frame.
     * @param localsNames names of local variables in the frame.
     * @param optimizedLines lines that are not present in the source map.
     * @returns new frame.
     * @throws Error with a descriptive error message if frame cannot be constructed.
     */
    newStackFrame(frameID, funName, fileHash, localsTypes, localsNames, optimizedLines) {
        const currentFile = this.filesMap.get(fileHash);
        if (!currentFile) {
            throw new Error(`Cannot find file with hash: ${fileHash}`);
        }
        let locals = [];
        // create first scope for local variables
        locals[0] = [];
        const stackFrame = {
            id: frameID,
            name: funName,
            file: currentFile.path,
            fileHash,
            line: 0, // line will be updated when next event (Instruction) is processed
            localsTypes,
            localsNames,
            locals,
            lastCallInstructionLine: undefined,
            optimizedLines
        };
        if (this.trace.events.length <= this.eventIndex + 1 ||
            (this.trace.events[this.eventIndex + 1].type !== trace_utils_1.TraceEventKind.Instruction &&
                this.trace.events[this.eventIndex + 1].type !== trace_utils_1.TraceEventKind.OpenFrame)) {
            throw new Error('Expected an Instruction or OpenFrame event after OpenFrame event');
        }
        return stackFrame;
    }
    /**
     * Emits an event to the adapter.
     *
     * @param event string representing the event.
     * @param args optional arguments to be passed with the event.
     */
    sendEvent(event, ...args) {
        setTimeout(() => {
            this.emit(event, ...args);
        }, 0);
    }
    /**
     * Creates a map from a file hash to file information for all Move source files in a directory.
     *
     * @param directory path to the directory containing Move source files.
     * @param filesMap map to update with file information.
     */
    hashToFileMap(directory) {
        const processDirectory = (dir) => {
            const files = fs.readdirSync(dir);
            for (const f of files) {
                const filePath = path.join(dir, f);
                const stats = fs.statSync(filePath);
                if (stats.isDirectory()) {
                    processDirectory(filePath);
                }
                else if (path.extname(f) === '.move') {
                    const content = fs.readFileSync(filePath, 'utf8');
                    const numFileHash = computeFileHash(content);
                    const lines = content.split('\n');
                    const fileInfo = { path: filePath, content, lines };
                    const fileHash = Buffer.from(numFileHash).toString('base64');
                    this.filesMap.set(fileHash, fileInfo);
                }
            }
        };
        processDirectory(directory);
    }
    //
    // Utility functions for testing and debugging.
    //
    /**
     * Whitespace used for indentation in the string representation of the runtime.
     */
    singleTab = '  ';
    /**
     * Returns a string representig the current state of the runtime.
     *
     * @returns string representation of the runtime.
     */
    toString() {
        let res = 'current frame stack:\n';
        for (const frame of this.frameStack.frames) {
            const fileName = path.basename(frame.file);
            res += this.singleTab
                + 'function: '
                + frame.name
                + ' ('
                + fileName
                + ':'
                + frame.line
                + ')\n';
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
        if (this.lineBreakpoints && this.lineBreakpoints.size > 0) {
            res += 'line breakpoints\n';
            for (const [file, breakpoints] of this.lineBreakpoints) {
                res += this.singleTab + path.basename(file) + '\n';
                for (const line of breakpoints) {
                    res += this.singleTab + this.singleTab + line + '\n';
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
    varToString(tabs, variable) {
        return this.valueToString(tabs, variable.value, variable.name, variable.type);
    }
    /**
     * Returns a string representation of a runtime compound value.
     *
     * @param compoundValue runtime compound value.
     * @returns string representation of the compound value.
     */
    compoundValueToString(tabs, compoundValue) {
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
    refValueToString(tabs, refValue, name, type) {
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
    valueToString(tabs, value, name, type) {
        let res = '';
        if (typeof value === 'string') {
            res += tabs + name + ' : ' + value + '\n';
            if (type) {
                res += tabs + 'type: ' + type + '\n';
            }
        }
        else if (Array.isArray(value)) {
            res += tabs + name + ' : [\n';
            for (let i = 0; i < value.length; i++) {
                res += this.valueToString(tabs + this.singleTab, value[i], String(i));
            }
            res += tabs + ']\n';
            if (type) {
                res += tabs + 'type: ' + type + '\n';
            }
            return res;
        }
        else if ('fields' in value) {
            res += tabs + name + ' : ' + this.compoundValueToString(tabs, value);
            if (type) {
                res += tabs + 'type: ' + type + '\n';
            }
        }
        else {
            res += this.refValueToString(tabs, value, name, type);
        }
        return res;
    }
}
exports.Runtime = Runtime;
/**
 * Handles a write to a local variable in a stack frame.
 *
 * @param frame stack frame frame.
 * @param localIndex variable index in the frame.
 * @param runtimeValue variable value.
 */
function localWrite(frame, localIndex, value) {
    const type = frame.localsTypes[localIndex];
    if (!type) {
        throw new Error('Cannot find type for local variable at index: '
            + localIndex
            + ' in function: '
            + frame.name);
    }
    const name = frame.localsNames[localIndex];
    if (!name) {
        throw new Error('Cannot find local variable at index: '
            + localIndex
            + ' in function: '
            + frame.name);
    }
    if (name.includes('%')) {
        // don't show "artificial" variables generated by the compiler
        // for enum and macro execution as they would be quite confusing
        // for the user without knowing compilation internals
        return;
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
    }
    else {
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
async function findPkgRoot(active_file_path) {
    const containsManifest = (dir) => {
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
function getPkgNameFromManifest(pkgRoot) {
    const manifest = fs.readFileSync(pkgRoot, 'utf8');
    const parsedManifest = toml_1.default.parse(manifest);
    const packageName = parsedManifest.package.name;
    return packageName;
}
/**
 * Computes the SHA-256 hash of a file's contents.
 *
 * @param fileContents contents of the file.
 */
function computeFileHash(fileContents) {
    const hash = crypto.createHash('sha256').update(fileContents).digest();
    return new Uint8Array(hash);
}
//# sourceMappingURL=runtime.js.map