// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { EventEmitter } from 'events';
import * as fs from 'fs';
import * as path from 'path';
import toml from 'toml';
import {
    createFileInfo,
    IFileInfo,
    ILocalInfo,
    IDebugInfo,
    readAllDebugInfos
} from './debug_info_utils';
import {
    INLINED_FRAME_ID_SAME_FILE,
    INLINED_FRAME_ID_DIFFERENT_FILE,
    TraceEffectKind,
    TraceEvent,
    TraceEventKind,
    TraceInstructionKind,
    readTrace,
} from './trace_utils';
import { TRACE_FILE_EXT } from './utils';

/**
 * File extension for Move source files.
 */
const MOVE_FILE_EXT = ".move";

/**
 * File extension for disassembled bytecode files.
 */
const BCODE_FILE_EXT = ".mvb";

/**
 * Name of the trace file containing external events.
 */
const EXT_EVENTS_TRACE_FILE_NAME = 'trace';

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
export type CompoundType = RuntimeValueType[] | IRuntimeCompoundValue;

/**
 * A runtime value can have any of the following types:
 * - boolean, number, string (converted to string)
 * - compound type (vector, struct, enum)
 */
export type RuntimeValueType = string | CompoundType | IRuntimeRefValue;

/**
 * Location of a local variable in the runtime.
 */
export interface IRuntimeVariableLoc {
    frameID: number;
    localIndex: number;
}

/**
 * Global location (typically used for values
 * computed in native functions)
 */
export interface IRuntimeGlobalLoc {
    globalIndex: number;
}

/**
 * Location where a runtime value is stored.
 */
export interface IRuntimeLoc {
    loc: IRuntimeVariableLoc | IRuntimeGlobalLoc;
    indexPath: number[];
}

/**
 * Value of a reference in the runtime.
 */
export interface IRuntimeRefValue {
    mutable: boolean;
    indexedLoc: IRuntimeLoc;
}

/**
 * Information about a runtime compound value (struct/enum).
 */
export interface IRuntimeCompoundValue {
    fields: [string, RuntimeValueType][];
    type: string;
    variantName?: string;
    variantTag?: number;
}

/**
 * Describes a runtime local variable.
 */
interface IRuntimeVariable {
    info: ILocalInfo;
    value: RuntimeValueType;
    type: string;
    frameIdx: number;
}

/**
 * Describes a stack frame in the runtime representing a Move function
 * call and its current state during trace viewing session.
 */
interface IMoveCallStackFrame {
    /**
     *  Frame identifier.
     */
    id: number;
    /**
     *  Name of the function in this frame.
     */
    name: string;
    /**
    *  Path to the source file containing currently executing instruction.
    */
    srcFilePath: string;
    /**
    *  Path to the disassembled bytecode file containing currently executing instruction.
    */
    bcodeFilePath?: string;
    /**
     *  File hash of the source file containing currently executing instruction.
     */
    srcFileHash: string;
    /**
     *  File hash of the disassembled bytecode file containing currently executing instruction.
     */
    bcodeFileHash?: string;
    /**
     * Current line in the source file corresponding to currently viewed instruction.
     */
    srcLine: number; // 1-based
    /**
     * Current line in the disassembled bytecode file corresponding to currently viewed instruction.
     */
    bcodeLine?: number; // 1-based
    /**
     *  Local variable types by variable frame index.
     */
    localsTypes: string[];
    /**
     * Local variables info by their index in the frame
     * (parameters first, then actual locals).
     */
    localsInfo: ILocalInfo[];
    /**
     * Local variables per scope (local scope at 0 and then following block scopes),
     * indexed by variable frame index.
     */
    locals: (IRuntimeVariable | undefined)[][];
    /**
     * Line in the source file of the last call instruction that was processed in this frame.
     * It's needed to make sure that step/next into/over call works correctly.
     */
    lastCallInstructionSrcLine?: number;
    /**
     * Line in the disassembled bytecode file of the last call instruction that was processed in this frame.
     * It's needed to make sure that step/next into/over call works correctly.
     */
    lastCallInstructionBcodeLine?: number;
    /**
     * Lines that are not present in the debug info.
     */
    optimizedSrcLines: number[];
    /**
     * Lines that are not present in the bytecode map.
     */
    optimizedBcodeLines?: number[];
    /**
     * Disassembly mode has been triggered (we have both
     * source and disassembly mode available for this frame).
     * This flag use used to choose between data stored for
     * source files and disassembled bytecode files.
     */
    disassemblyModeTriggered: boolean;
    /**
     * We are showing disassembly view (this may be the case
     * either because disassembly mode has been triggered or
     * because we only have disassembled bytecode available).
     * This flag is used to control how debugger behaves when
     * showing disasembled bytecode.
     */
    disassemblyView: boolean;
}

/**
 * Describes the Move call stack during trace viewing session
 * (oldest frame is at the bottom of the stack at index 0).
 */
export interface IMoveCallStack {
    frames: IMoveCallStackFrame[];
    globals: Map<number, RuntimeValueType>;
}

/**
 * Describes information about a Move call.
 */
export interface IMoveCallInfo {
    pkg: string;
    module: string;
    function: string;
}

/**
 * Describes the type of external event summary,
 * a Move call info descrption or a string representing
 * another external event.
 */
export type ExtEventSummary = IMoveCallInfo | string;

/**
 * Describes a frame containing a summary of all external events.
 */
interface IExtEventsSummaryFrame {
    /**
     * Summary frame ID.
     */
    id: number;
    /**
     * Summary frame (virtual) line.
     */
    line: number;
    /**
     * Name of the summary frame.
     */
    name: string;
    /**
     * Summary itself.
     */
    summary: ExtEventSummary[];
}

/**
 * Kind of external evemt. Strings reflect names
 * of the external events.
 */
export enum ExtEventKind {
    MoveCallStart = 'MoveCallStart', // used in trace to identify Move call start
    MoveCallEnd = 'MoveCallEnd', // used in trace to identify Move call end
    ExtEventStart = 'ExtEventStart',
    ExtEventEnd = 'ExtEventEnd',
}

/**
 * Describes external event stack frame.
 */
export interface IExtEventStackFrame {
    id: number;
    line: number;
    description: string;
    name: string;
    locals: IRuntimeVariable[];
}

/**
 * Describes an events stack during trace viewing session.
 */
export interface IEventsStack {
    /**
     * A frame containing the events summary. It's present
     * when the trace includes external events and not just
     * a single Move function.
     */
    summaryFrame?: IExtEventsSummaryFrame
    /**
     * A frame containing either the external event information
     * or a Move call execution stack. It's present either when
     * processing an external event (along with summary frame), or
     * by itself if the trace represents a single Move function call.
     */
    eventFrame?: IExtEventStackFrame | IMoveCallStack;
}

/**
 * Events emitted by the runtime during trace viewing session.
 */
export enum RuntimeEvents {
    /**
     *  Stop after step/next action is performed.
     */
    stopOnStep = 'stopOnStep',

    /**
     * Stop after a line breakpoint is hit.
     */
    stopOnLineBreakpoint = 'stopOnLineBreakpoint',

    /**
     * Stop after exception has been encountered.
     */
    stopOnException = 'stopOnException',

    /**
     *  Finish trace viewing session.
     */
    end = 'end',
}
/**
 * Describes result of the execution.
 */
export enum ExecutionResult {
    Ok,
    TraceEnd,
    Exception,
    Breakpoint,
}

/**
 * The runtime for viewing traces.
 */
export class Runtime extends EventEmitter {

    /**
     * Trace being viewed.
     */
    private trace = {
        events: [] as TraceEvent[],
        localLifetimeEnds: new Map<number, number[]>(),
        tracedSrcLines: new Map<string, Set<number>>(),
        tracedBcodeLines: new Map<string, Set<number>>()
    };

    /**
     * Index of the current trace event being processed.
     */
    private eventIndex = 0;

    /**
     * Current events stack.
     */
    private eventsStack: IEventsStack = {};

    /**
     * Map of file hashes to file info (both for source files
     * and for files representing disassembled bytecode).
     */
    private filesMap = new Map<string, IFileInfo>();

    /**
     * Map of line breakpoints, keyed on a file path.
     */
    private lineBreakpoints = new Map<string, Set<number>>();

    /**
     * A Move file currently opened in the editor window that
     * corresponds to one of the frames on the stack.
     */
    private currentMoveFile: string | undefined = undefined;

    /**
     * Start a trace viewing session and set up the initial state of the runtime.
     * We recognize two types of traces and corresponding file names as well as directory
     * structure: containing only a single top-level Move call (typically generated
     * when running unit tests) and containing external events.
     *
     * @param openedFilePath path to the Move source file (or disassembled bytecode file,)
     * or the trace file itself), whose traces are to be viewed.
     * @param traceInfo  trace selected for viewing.
     * @throws Error with a descriptive error message if starting runtime has failed.
     *
     */
    public async start(
        openedFilePath: string,
        traceInfo: string,
        stopOnEntry: boolean
    ): Promise<void> {
        const openedFileExt = path.extname(openedFilePath);
        const openedFileBaseName = path.basename(openedFilePath, TRACE_FILE_EXT);
        let srcDebugInfosModMap = new Map<string, IDebugInfo>();
        let bcodeDebugInfosModMap = new Map<string, IDebugInfo>();
        let disassemblyView = false;
        let traceFilePath = ''; // updated in both conditional branches
        if (openedFilePath.endsWith(TRACE_FILE_EXT) && openedFileBaseName === EXT_EVENTS_TRACE_FILE_NAME) {
            // Trace containing external events. Reading all data required for debugging
            // assumes a certain directory structure rooted in `extRoot`, where the trace
            // file is located. The `bytecode` directory contains disassembled bytecode
            // files and their debug infos. The `source` directory contains Move source files
            // and their debug infos.
            const extRoot = path.dirname(openedFilePath);
            const bytecodeDir = path.join(extRoot, 'bytecode');
            hashToFileMap(bytecodeDir, this.filesMap, BCODE_FILE_EXT);
            bcodeDebugInfosModMap = readAllDebugInfos(bytecodeDir, this.filesMap, true);
            const sourceDir = path.join(extRoot, 'source');
            if (fs.existsSync(sourceDir)) {
                const sourceFilesMap = new Map<string, IFileInfo>();
                hashToFileMap(sourceDir, sourceFilesMap, MOVE_FILE_EXT);
                // We are getting files and debug infos from the source directory
                // which is populated by the user. One way to do it would be to copy
                // `build` directory of package to the source directory, which would
                // contain all the required sources and debug infos. However, this
                // build directory may also contain disassembled bytecode files and
                // their corresponding debug infos, which need to be filtered out.
                // This is accomplished by passing `mustHaveSourceFile` as `false`.
                // and sourceFilesMap that contain only Move source files - this way,
                // since disassembled bytecode files are not present in sourceFilesMap,
                // debug infos for disassembled bytecode will be excluded.
                srcDebugInfosModMap =
                    readAllDebugInfos(sourceDir, sourceFilesMap, /* mustHaveSourceFile */ false);
                sourceFilesMap.forEach((fileInfo, fileHash) => {
                    this.filesMap.set(fileHash, fileInfo);
                });
            }
            traceFilePath = openedFilePath;
        } else {
            // Trace containing only a single top-level Move function call,
            // typically generated by running Move unit tests with tracing enabled.
            // Assumes the standard Move package directory structure rooted in `pkgRoot`,
            // including all date required for debugging in the `build` directory.
            const pkgRoot = await findPkgRoot(openedFilePath);
            if (!pkgRoot) {
                throw new Error(`Cannot find package root for file: ${openedFilePath}`);
            }
            const manifest_path = path.join(pkgRoot, 'Move.toml');

            // find package name from manifest file which corresponds `build` directory's subdirectory
            // name containing this package's build files
            const pkg_name = getPkgNameFromManifest(manifest_path);
            if (!pkg_name) {
                throw Error(`Cannot find package name in manifest file: ${manifest_path}`);
            }

            if (!openedFilePath.endsWith(MOVE_FILE_EXT) &&
                !openedFilePath.endsWith(TRACE_FILE_EXT)) {
                throw new Error(`File extension: ${openedFileExt} is not supported by trace debugger`);
            }

            // create file maps for all files in the `sources` directory, including both package source
            // files and source files for dependencies
            hashToFileMap(path.join(pkgRoot, 'build', pkg_name, 'sources'), this.filesMap, MOVE_FILE_EXT);
            // update with files from the actual "sources" directory rather than from the "build" directory
            hashToFileMap(path.join(pkgRoot, 'sources'), this.filesMap, MOVE_FILE_EXT);

            // create debug infos for all modules in the `build` directory
            const srcSourceMapDir = path.join(pkgRoot, 'build', pkg_name, 'source_maps');
            const srcDbgInfoDir = fs.existsSync(srcSourceMapDir)
                ? srcSourceMapDir
                : path.join(pkgRoot, 'build', pkg_name, 'debug_info');

            srcDebugInfosModMap = readAllDebugInfos(srcDbgInfoDir, this.filesMap, true);

            // reconstruct trace file path from trace info
            traceFilePath = path.join(pkgRoot, 'traces', traceInfo.replace(/:/g, '_') + TRACE_FILE_EXT);

            const disassemblyDir = path.join(pkgRoot, 'build', pkg_name, 'disassembly');
            if (fs.existsSync(disassemblyDir)) {
                // create file maps for all bytecode files in the `disassembly` directory
                hashToFileMap(disassemblyDir, this.filesMap, BCODE_FILE_EXT);
                // created bytecode maps for disassembled bytecode files
                bcodeDebugInfosModMap = readAllDebugInfos(disassemblyDir, this.filesMap, true);
            }
        }
        Array.from(srcDebugInfosModMap.entries()).forEach((entry) => {
            const [mod, bcodeMap] = entry;
        });

        // create a mapping from source file hash to its corresponding debug info
        const srcDebugInfosHashMap = new Map<string, IDebugInfo>;
        for (const [_, info] of srcDebugInfosModMap) {
            srcDebugInfosHashMap.set(info.fileHash, info);
        }

        // if we are missing source debug infos (and thus source files), but have bytecode debug infos
        // (and thus disassembled bytecode files), we will only be able to show disassembly,
        // which becomes the default (source) view
        Array.from(bcodeDebugInfosModMap.entries()).forEach((entry) => {
            const [mod, bcodeMap] = entry;
            if (!srcDebugInfosModMap.has(mod)) {
                srcDebugInfosModMap.set(mod, bcodeMap);
                bcodeDebugInfosModMap.delete(mod);
            }
        });

        this.trace = await readTrace(traceFilePath, srcDebugInfosHashMap, srcDebugInfosModMap, bcodeDebugInfosModMap, this.filesMap);

        // start trace viewing session with the first trace event
        this.eventIndex = 0;

        const currentEvent = this.trace.events[this.eventIndex];
        if (openedFilePath.endsWith(TRACE_FILE_EXT) && openedFileBaseName === EXT_EVENTS_TRACE_FILE_NAME) {
            // trace containing external events
            if (currentEvent.type !== TraceEventKind.ExternalSummary) {
                throw new Error(`First event in trace is not an OpenFrame event`);
            }
            const eventsSummaryFrame: IExtEventsSummaryFrame = {
                id: currentEvent.id,
                line: 1,
                name: currentEvent.name,
                summary: currentEvent.summary
            };
            this.eventsStack.summaryFrame = eventsSummaryFrame;
        } else {
            // setup frame stack with the first frame
            if (currentEvent.type !== TraceEventKind.OpenFrame) {
                throw new Error(`First event in trace is not an OpenFrame event`);
            }
            const newFrame =
                this.newMoveStackFrame(
                    currentEvent.id,
                    currentEvent.name,
                    currentEvent.srcFileHash,
                    currentEvent.bcodeFileHash,
                    currentEvent.localsTypes,
                    currentEvent.localsNames,
                    currentEvent.optimizedSrcLines,
                    currentEvent.optimizedBcodeLines
                );
            if (path.extname(newFrame.srcFilePath) === BCODE_FILE_EXT) {
                // disassembed bytecode file is the only file available
                // meaning disassembly view is on
                if (newFrame.bcodeFilePath !== undefined) {
                    // this should never happen but assert just in case
                    throw new Error('Disassembled bytecode file path is not expected to be set for file: '
                        + newFrame.srcFilePath);
                }
                disassemblyView = true;
            }
            // disassembly mode was triggered if disassembly view is on
            // and we have both source and disassembled bytecode files
            newFrame.disassemblyModeTriggered = disassemblyView && newFrame.bcodeFilePath !== undefined;
            newFrame.disassemblyView = disassemblyView;

            const frameStack: IMoveCallStack = {
                frames: [newFrame],
                globals: new Map<number, RuntimeValueType>()
            };
            this.eventsStack.eventFrame = frameStack;
            this.stepInternal(/* next */ false, /* stopAtCloseFrame */ false);
        }
    }

    /**
     * Handles "get current stack" adapter action.
     *
     * @returns current frame stack.
     */
    public stack(): IEventsStack {
        return this.eventsStack;
    }

    /**
     * Processes the result of an action returnd from an internal
     * function responsible for handling a given event.
     *
     * @param result the result of the action.
     * @returns processed (potentially different) result of the action.
     */
    private handleActionResult(result: ExecutionResult): ExecutionResult {
        switch (result) {
            case ExecutionResult.Ok:
            case ExecutionResult.TraceEnd:
                this.sendEvent(RuntimeEvents.stopOnStep);
                break;
            case ExecutionResult.Exception:
                this.sendEvent(RuntimeEvents.stopOnException);
                break;
            case ExecutionResult.Breakpoint:
                this.sendEvent(RuntimeEvents.stopOnLineBreakpoint);
                return ExecutionResult.Ok;
        }
        return result;
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
    public step(next: boolean, stopAtCloseFrame: boolean): ExecutionResult {
        const result = this.stepInternal(next, stopAtCloseFrame);
        return this.handleActionResult(result);
    }

    /**
     * Implements step/next action.
     *
     * @param next determines if it's `next` (or otherwise `step`) action.
     * @param stopAtCloseFrame determines if the action should stop at `CloseFrame` event
     * (rather then proceed to the following instruction).
     * @returns ExecutionResult.Ok if the step action was successful, ExecutionResult.TraceEnd if we
     * reached the end of the trace, and ExecutionResult.Exception if an exception was encountered.
     * @throws Error with a descriptive error message if the step event cannot be handled.
     */
    private stepInternal(next: boolean, stopAtCloseFrame: boolean): ExecutionResult {
        this.eventIndex++;
        if (this.eventIndex >= this.trace.events.length) {
            return ExecutionResult.TraceEnd;
        }
        let currentEvent = this.trace.events[this.eventIndex];

        if (currentEvent.type === TraceEventKind.Instruction ||
            currentEvent.type === TraceEventKind.ReplaceInlinedFrame ||
            currentEvent.type === TraceEventKind.OpenFrame ||
            currentEvent.type === TraceEventKind.CloseFrame ||
            currentEvent.type === TraceEventKind.Effect) {
            // events ralated to Move call execution

            const eventFrame = this.eventsStack.eventFrame;
            if (!eventFrame || !('frames' in eventFrame) || !('globals' in eventFrame)) {
                throw new Error(`No active Move call when processing event ${currentEvent.type}`);
            }
            const moveCallStack = eventFrame as IMoveCallStack;

            if (currentEvent.type === TraceEventKind.Instruction) {
                const stackHeight = moveCallStack.frames.length;
                if (stackHeight <= 0) {
                    // this should never happen
                    throw new Error('No frame on the stack when processing Instruction event when stepping');
                }
                const currentFrame = moveCallStack.frames[stackHeight - 1];
                // remember last call instruction line before it (potentially) changes
                // in the `instruction` call below
                const lastCallInstructionLine = currentFrame.disassemblyModeTriggered
                    ? currentFrame.lastCallInstructionBcodeLine
                    : currentFrame.lastCallInstructionSrcLine;
                let [sameLine, currentLine] = this.instruction(currentFrame, currentEvent);
                // do not attempt to skip events on the same line if the previous event
                // was a switch to/from an inlined frame - we want execution to stop before
                // the first instruction of the inlined frame is processed
                const prevEvent = this.trace.events[this.eventIndex - 1];
                sameLine = sameLine &&
                    !(prevEvent.type === TraceEventKind.ReplaceInlinedFrame
                        || prevEvent.type === TraceEventKind.OpenFrame &&
                        (prevEvent.id === INLINED_FRAME_ID_SAME_FILE || prevEvent.id === INLINED_FRAME_ID_DIFFERENT_FILE)
                        || prevEvent.type === TraceEventKind.CloseFrame &&
                        (prevEvent.id === INLINED_FRAME_ID_SAME_FILE || prevEvent.id === INLINED_FRAME_ID_DIFFERENT_FILE));
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
                        // we should skip over all calls on the same line (both `bar`
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
                        // and wait for another `step` action from the user:
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
                        return ExecutionResult.Ok;
                    } else {
                        return this.stepInternal(next, stopAtCloseFrame);
                    }
                }
                return ExecutionResult.Ok;
            } else if (currentEvent.type === TraceEventKind.ReplaceInlinedFrame) {
                let currentFrame = moveCallStack.frames.pop();
                if (!currentFrame) {
                    throw new Error('No frame to pop when processing `ReplaceInlinedFrame` event');
                }
                currentFrame.srcFileHash = currentEvent.fileHash;
                currentFrame.optimizedSrcLines = currentEvent.optimizedLines;
                const currentFile = this.filesMap.get(currentFrame.srcFileHash);
                if (!currentFile) {
                    throw new Error('Cannot find file with hash '
                        + currentFrame.srcFileHash
                        + ' when processing `ReplaceInlinedFrame` event');
                }
                currentFrame.srcFilePath = currentFile.path;
                moveCallStack.frames.push(currentFrame);
                return this.stepInternal(next, stopAtCloseFrame);
            } else if (currentEvent.type === TraceEventKind.OpenFrame) {
                // if function is native then the next event will be CloseFrame
                if (currentEvent.isNative) {
                    // see if native function aborted
                    if (this.trace.events.length > this.eventIndex + 1) {
                        const nextEvent = this.trace.events[this.eventIndex + 1];
                        if (nextEvent.type === TraceEventKind.Effect &&
                            nextEvent.effect.type === TraceEffectKind.ExecutionError) {
                            return ExecutionResult.Exception;
                        }
                    }
                    // process optional effects until reaching CloseFrame for the native function
                    while (true) {
                        const executionResult = this.stepInternal(/* next */ false, /* stopAtCloseFrame */ true);
                        if (executionResult === ExecutionResult.Exception) {
                            return executionResult;
                        }
                        if (executionResult === ExecutionResult.TraceEnd) {
                            throw new Error('Cannot find CloseFrame event for native function');
                        }
                        const currentEvent = this.trace.events[this.eventIndex];
                        if (currentEvent.type === TraceEventKind.CloseFrame) {
                            break;
                        }
                    }
                    // skip over CloseFrame as there is no frame to pop
                    this.eventIndex++;
                    return this.stepInternal(next, stopAtCloseFrame);
                }

                // create a new frame and push it onto the stack
                const newFrame =
                    this.newMoveStackFrame(
                        currentEvent.id,
                        currentEvent.name,
                        currentEvent.srcFileHash,
                        currentEvent.bcodeFileHash,
                        currentEvent.localsTypes,
                        currentEvent.localsNames,
                        currentEvent.optimizedSrcLines,
                        currentEvent.optimizedBcodeLines
                    );
                // when creating a new frame maintain the invariant
                // that all frames that belong to modules in the same
                // file get the same view
                newFrame.disassemblyModeTriggered = moveCallStack.frames.find(
                    frame => frame.disassemblyModeTriggered
                        && frame.bcodeFilePath === newFrame.bcodeFilePath
                        && frame.srcFilePath === newFrame.srcFilePath
                ) !== undefined;
                newFrame.disassemblyView = moveCallStack.frames.find(
                    frame => frame.disassemblyView
                        && frame.bcodeFilePath === newFrame.bcodeFilePath
                        && frame.srcFilePath === newFrame.srcFilePath
                ) !== undefined;

                // set values of parameters in the new frame
                moveCallStack.frames.push(newFrame);
                for (let i = 0; i < currentEvent.paramValues.length; i++) {
                    localWrite(
                        newFrame,
                        moveCallStack.frames.length - 1,
                        i,
                        currentEvent.paramValues[i]
                    );
                }

                if (next && !(newFrame.disassemblyView &&
                    (newFrame.id === INLINED_FRAME_ID_SAME_FILE
                        || newFrame.id === INLINED_FRAME_ID_DIFFERENT_FILE))) {
                    // step out of the frame right away unless this frame is inlined
                    // and it's showing disassembly (otherwise we will see instructions
                    // skipped in the disassembly view for apparently no reason)
                    return this.stepOutInternal(next);
                } else {
                    return this.stepInternal(next, stopAtCloseFrame);
                }
            } else if (currentEvent.type === TraceEventKind.CloseFrame) {
                if (stopAtCloseFrame) {
                    // don't do anything as the caller needs to inspect
                    // the event before proceeding
                    return ExecutionResult.Ok;
                } else {
                    // pop the top frame from the stack
                    const framesLength = moveCallStack.frames.length;
                    if (framesLength <= 0) {
                        throw new Error('No frame to pop at CloseFrame event with ID: '
                            + currentEvent.id);
                    }
                    const currentFrameID = moveCallStack.frames[framesLength - 1].id;
                    if (currentFrameID !== currentEvent.id) {
                        throw new Error('Frame ID mismatch at CloseFrame event with ID: '
                            + currentEvent.id
                            + ' (current frame ID: '
                            + currentFrameID
                            + ')');
                    }
                    moveCallStack.frames.pop();
                    return this.stepInternal(next, stopAtCloseFrame);
                }
            } else if (currentEvent.type === TraceEventKind.Effect) {
                const effect = currentEvent.effect;
                if (effect.type === TraceEffectKind.ExecutionError) {
                    return ExecutionResult.Exception;
                }
                if (effect.type === TraceEffectKind.Write) {
                    const traceLocation = effect.indexedLoc.loc;
                    if ('globalIndex' in traceLocation) {
                        const globalValue = effect.value;
                        moveCallStack.globals.set(traceLocation.globalIndex, globalValue);
                    } else if ('frameID' in traceLocation && 'localIndex' in traceLocation) {
                        const traceValue = effect.value;
                        let frame = undefined;
                        let frameIdx = 0;
                        for (const f of moveCallStack.frames) {
                            if (f.id === traceLocation.frameID) {
                                frame = f;
                                break;
                            }
                            frameIdx++;
                        }
                        if (!frame) {
                            throw new Error('Cannot find frame with ID: '
                                + traceLocation.frameID
                                + ' when processing Write effect for local variable at index: '
                                + traceLocation.localIndex);
                        }
                        localWrite(frame, frameIdx, traceLocation.localIndex, traceValue);
                    }
                }
                return this.stepInternal(next, stopAtCloseFrame);
            }
            throw new Error('Unknown Move call event: ' + currentEvent);
        } else {
            if (currentEvent.type === TraceEventKind.ExternalEvent) {
                switch (currentEvent.event.kind) {
                    case ExtEventKind.MoveCallStart:
                        const frameStack: IMoveCallStack = {
                            frames: [],
                            globals: new Map<number, RuntimeValueType>()
                        };
                        this.eventsStack.eventFrame = frameStack;
                        return this.stepInternal(next, stopAtCloseFrame);
                    case ExtEventKind.ExtEventStart:
                        if (next) {
                            // simply skip over external event as its
                            // "execution" has no bearing on execution
                            // of subsequent events
                            return this.stepInternal(next, stopAtCloseFrame);
                        }
                        const frameIdx = currentEvent.event.id;
                        const names = currentEvent.event.localsNames;
                        const values = currentEvent.event.localsValues;
                        const locals = currentEvent.event.localsTypes.map((type, idx) => {
                            const name = names[idx];
                            const info: ILocalInfo = {
                                name,
                                internalName: name,
                            };
                            const local: IRuntimeVariable = {
                                info,
                                value: values[idx],
                                type,
                                frameIdx,
                            };
                            return local;
                        });
                        const eventFrame: IExtEventStackFrame = {
                            id: currentEvent.event.id,
                            line: 1,
                            description: currentEvent.event.description,
                            name: currentEvent.event.name,
                            locals,
                        };
                        this.eventsStack.eventFrame = eventFrame;
                        return ExecutionResult.Ok;
                    case ExtEventKind.MoveCallEnd:
                    case ExtEventKind.ExtEventEnd:
                        // go back to summary frame
                        this.eventsStack.eventFrame = undefined;
                        if (this.eventsStack.summaryFrame) {
                            this.eventsStack.summaryFrame.line += 1;
                        }
                        return ExecutionResult.Ok;
                }
            }
            throw new Error('Unknown external event: ' + currentEvent);
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
    public stepOut(next: boolean): ExecutionResult {
        const result = this.stepOutInternal(next);
        return this.handleActionResult(result);
    }

    /**
     * Implements "step out" action.
     *
     * @param next determines if it's  part of `next` (or otherwise `step`) action.
     * @returns ExecutionResult.Ok if the step action was successful, ExecutionResult.TraceEnd if we
     * reached the end of the trace, and ExecutionResult.Exception if an exception was encountered.
     * @throws Error with a descriptive error message if the step out event cannot be handled.
     */
    private stepOutInternal(next: boolean): ExecutionResult {
        const summaryFrame = this.eventsStack.summaryFrame;
        const eventFrame = this.eventsStack.eventFrame;
        if (summaryFrame && !eventFrame) {
            // stepping out of (top) active summary frame
            // finishes debugging session
            return ExecutionResult.TraceEnd;
        }

        // summary frame is not active here which means that
        // event frame must be

        if (!eventFrame) {
            throw new Error('No active frame when processing step out event');
        }

        if ('frames' in eventFrame && 'globals' in eventFrame) {
            // Move call frame
            const moveCallStack = eventFrame as IMoveCallStack;
            const stackHeight = moveCallStack.frames.length;
            if (stackHeight === 0 || (stackHeight === 1 && !summaryFrame)) {
                // do nothing as there is no frame to step out to
                return ExecutionResult.Ok;
            }
            // newest frame is at the top of the stack
            const currentFrame = moveCallStack.frames[stackHeight - 1];
            let currentEvent = this.trace.events[this.eventIndex];
            // skip all events until the corresponding CloseFrame event,
            // pop the top frame from the stack, and proceed to the next event
            while (true) {
                // when calling `step` in the loop below, we need to avoid
                // skipping over calls next-style otherwise we can miss seeing
                // the actual close frame event that we are looking for
                // and have the loop execute too far
                const executionResult = this.stepInternal(/* next */ false, /* stopAtCloseFrame */ true);
                if (executionResult === ExecutionResult.Exception) {
                    return executionResult;
                }
                if (executionResult === ExecutionResult.TraceEnd) {
                    throw new Error('Cannot find corresponding CloseFrame event for function: ' +
                        currentFrame.name);
                }
                currentEvent = this.trace.events[this.eventIndex];
                if (this.is_event_at_breakpoint(currentEvent)) {
                    return ExecutionResult.Breakpoint;
                }
                if (currentEvent.type === TraceEventKind.CloseFrame) {
                    const currentFrameID = currentFrame.id;
                    // `step` call finished at the CloseFrame event
                    // but did not process it so we need pop the frame here
                    moveCallStack.frames.pop();
                    if (currentEvent.id === currentFrameID) {
                        break;
                    }
                }
            }
            return this.stepInternal(next, /* stopAtCloseFrame */ false);
        } else {
            // external event frame
            return this.stepInternal(next, /* stopAtCloseFrame */ false);
        }
    }

    /**
     * Handles "continue" adapter action.
     * @returns ExecutionResult.Ok if the step action was successful, ExecutionResult.TraceEnd if we
     * reached the end of the trace, and ExecutionResult.Exception if an exception was encountered.
     * @throws Error with a descriptive error message if the continue event cannot be handled.
     */
    public continue(): ExecutionResult {
        const result = this.continueInternal();
        return this.handleActionResult(result);
    }

    /**
     * Implements "continue" action.
     * @returns ExecutionResult.Ok if the step action was successful, ExecutionResult.TraceEnd if we
     * reached the end of the trace, and ExecutionResult.Exception if an exception was encountered.
     * @throws Error with a descriptive error message if the continue event cannot be handled.
     */
    private continueInternal(): ExecutionResult {
        while (true) {
            const executionResult = this.stepInternal(/* next */ false, /* stopAtCloseFrame */ false);
            if (executionResult === ExecutionResult.TraceEnd ||
                executionResult === ExecutionResult.Exception) {
                return executionResult;
            }
            const currentEvent = this.trace.events[this.eventIndex];
            if (this.is_event_at_breakpoint(currentEvent)) {
                return ExecutionResult.Breakpoint;
            }
        }
    }

    /**
     *  Checks if an event is an instruction at a breakpoint.
     *
     *  @returns true if the current event is an instruction at a breakpoint, false otherwise.
     *  @throws Error with a descriptive error message if the instruction event cannot be handled.
    */
    private is_event_at_breakpoint(event: TraceEvent): boolean {
        if (event.type === TraceEventKind.Instruction) {
            const eventFrame = this.eventsStack.eventFrame;
            if (eventFrame && 'frames' in eventFrame && 'globals' in eventFrame) {
                const moveCallStack = eventFrame as IMoveCallStack;
                const stackHeight = moveCallStack.frames.length;
                if (stackHeight <= 0) {
                    // this should never happen
                    throw new Error('No frame on the stack when processing Instruction event when continuing');
                }
                const currentFrame = moveCallStack.frames[stackHeight - 1];
                const filePath = currentFrame.disassemblyModeTriggered
                    ? currentFrame.bcodeFilePath!
                    : currentFrame.srcFilePath;
                const breakpoints = this.lineBreakpoints.get(filePath);
                if (!breakpoints) {
                    return false;
                }
                const instLine = currentFrame.disassemblyModeTriggered
                    ? event.bcodeLoc!.line
                    : event.srcLoc.line;
                if (breakpoints.has(instLine)) {
                    return true;
                }
            } else {
                throw new Error('No active Move call when processing Instruction event');
            }
        }
        return false;
    }

    /**
     * Sets line breakpoints for a file (resetting any existing ones).
     *
     * @param filePath file path.
     * @param lines breakpoints lines.
     * @returns array of booleans indicating if a breakpoint was set on a line.
     * @throws Error with a descriptive error message if breakpoints cannot be set.
     */
    public setLineBreakpoints(filePath: string, lines: number[]): boolean[] {
        const breakpoints = new Set<number>();
        const fileExt = path.extname(filePath);
        if (fileExt !== MOVE_FILE_EXT && fileExt !== BCODE_FILE_EXT) {
            return [];
        }
        // For a source file, `tracedLines` will be in `tracedSrcLines`,
        // but if no source file exists (only bytecode) then it may be
        // in `tracedSourceLines` for the bytecode file as well, so simply
        // use the path for search. If not found, and it's the bytecode file
        // then search in `tracedBcodeLines` as well.
        let tracedLines = this.trace.tracedSrcLines.get(filePath);
        if (!tracedLines && fileExt === BCODE_FILE_EXT) {
            tracedLines = this.trace.tracedBcodeLines.get(filePath);
        }

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
        this.lineBreakpoints.set(filePath, breakpoints);
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
    private instruction(
        currentFrame: IMoveCallStackFrame,
        instructionEvent: Extract<TraceEvent, { type: TraceEventKind.Instruction }>
    ): [boolean, number] {
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
            currentFrame.lastCallInstructionSrcLine = instructionEvent.srcLoc.line;
            currentFrame.lastCallInstructionBcodeLine = instructionEvent.bcodeLoc?.line;
        }

        const instLine = currentFrame.disassemblyModeTriggered
            ? instructionEvent.bcodeLoc!.line
            : instructionEvent.srcLoc.line;
        const frameLine = currentFrame.disassemblyModeTriggered
            ? currentFrame.bcodeLine!
            : currentFrame.srcLine;

        if (instLine === frameLine) {
            // so that instructions on the same line can be bypassed
            return [true, instLine];
        } else {
            currentFrame.srcLine = instructionEvent.srcLoc.line;
            currentFrame.bcodeLine = instructionEvent.bcodeLoc?.line;
            return [false, instLine];
        }
    }

    /**
     * Given a path to a file, sets the currently opened Move file to that path
     * if this file that corresponds to one of the frames on the stack.
     *
     * @param filePath path to the currently opened Move file.
     * @returns path to newly modified currently opened Move file
     * or `undefined` if the file cannot be found.
     */
    public setCurrentMoveFileFromPath(filePath: string): string | undefined {
        const summaryFrame = this.eventsStack.summaryFrame;
        const eventFrame = this.eventsStack.eventFrame;
        if (summaryFrame && !eventFrame) {
            // summary frame is active, do nothing
            this.currentMoveFile = undefined;
        } else {
            if (eventFrame && 'frames' in eventFrame && 'globals' in eventFrame) {
                const moveCallStack = eventFrame as IMoveCallStack;
                if (moveCallStack.frames.find(frame =>
                    frame.disassemblyModeTriggered
                        ? frame.bcodeFilePath === filePath
                        : frame.srcFilePath === filePath)) {
                    this.currentMoveFile = filePath;
                } else {
                    this.currentMoveFile = undefined;
                }
            } else {
                this.currentMoveFile = undefined;
            }
        }
        return this.currentMoveFile;
    }

    /**
     * Given a frame ID, sets the currently opened Move file to the file
     * corresponding to the frame with that ID.
     *
     * @param frameId frame identifier.
     *
     * @returns path to newly modified currently opened Move file
     * or `undefined` if the file cannot be found.
     */
    public setCurrentMoveFileFromFrame(frameId: number): string | undefined {
        const summaryFrame = this.eventsStack.summaryFrame;
        const eventFrame = this.eventsStack.eventFrame;
        if (summaryFrame && !eventFrame) {
            // summary frame is active, no current Move file
            this.currentMoveFile = undefined;
        } else {
            if (eventFrame && 'frames' in eventFrame && 'globals' in eventFrame) {
                const moveCallStack = eventFrame as IMoveCallStack;
                const frame = moveCallStack.frames.find(frame => frame.id === frameId);
                if (frame) {
                    this.currentMoveFile = frame.disassemblyModeTriggered
                        ? frame.bcodeFilePath
                        : frame.srcFilePath;
                } else {
                    this.currentMoveFile = undefined;
                }
            } else {
                this.currentMoveFile = undefined;
            }
        }
        return this.currentMoveFile;
    }

    /**
     * Toggles disassembly view for all frames on the stack whose
     * source is the currently opened Move file.
     */
    public toggleDisassembly(): void {
        if (!this.currentMoveFile) {
            return;
        }
        const eventFrame = this.eventsStack.eventFrame;
        if (this.eventsStack.summaryFrame && !eventFrame) {
            // summary frame is active, do nothing
            return;
        }
        if (eventFrame && 'frames' in eventFrame && 'globals' in eventFrame) {
            const moveCallStack = eventFrame as IMoveCallStack;
            moveCallStack.frames.forEach(frame => {
                if (frame.srcFilePath === this.currentMoveFile
                    && frame.bcodeFileHash !== undefined
                    && frame.bcodeFilePath !== undefined
                    && frame.bcodeLine !== undefined
                    && frame.optimizedBcodeLines !== undefined) {
                    frame.disassemblyModeTriggered = true;
                    frame.disassemblyView = true;
                }
            });
        }
    }

    public toggleSource(): void {
        const eventFrame = this.eventsStack.eventFrame;
        if (this.eventsStack.summaryFrame && !eventFrame) {
            // summary frame is active, do nothing
            return;
        }
        if (eventFrame && 'frames' in eventFrame && 'globals' in eventFrame) {
            const moveCallStack = eventFrame as IMoveCallStack;
            moveCallStack.frames.forEach(frame => {
                if (frame.bcodeFilePath === this.currentMoveFile) {
                    frame.disassemblyModeTriggered = false;
                    frame.disassemblyView = false;
                }
            });
        }
    }

    /**
     * Creates a new Move stack frame based on info from the `OpenFrame` trace event.
     *
     * @param frameID frame identifier from the trace event.
     * @param funName function name.
     * @param modInfo information about module containing the function.
     * @param srcFileHash hash of the source file containing the function.
     * @param bcodeFileHash hash of the disassembled bytecode file containing the function.
     * @param localsTypes types of local variables in the frame.
     * @param localsInfo information about local variables in the frame.
     * @param optimizedSrcLines lines that are not present in the source debug info.
     * @param optimizedBcodeLines lines that are not present in the bytecode debug info.
     * @returns new frame.
     * @throws Error with a descriptive error message if frame cannot be constructed.
     */
    private newMoveStackFrame(
        frameID: number,
        funName: string,
        srcFileHash: string,
        bcodeFileHash: undefined | string,
        localsTypes: string[],
        localsInfo: ILocalInfo[],
        optimizedSrcLines: number[],
        optimizedBcodeLines: undefined | number[]
    ): IMoveCallStackFrame {
        const currentFile = this.filesMap.get(srcFileHash);

        if (!currentFile) {
            throw new Error(`Cannot find file with hash: ${srcFileHash} `);
        }
        const srcFilePath = currentFile.path;
        let bcodeFilePath = undefined;
        if (bcodeFileHash) {
            const bcodeFile = this.filesMap.get(bcodeFileHash);
            if (bcodeFile) {
                bcodeFilePath = bcodeFile.path;
            }
        }

        let locals = [];
        // create first scope for local variables
        locals[0] = [];
        const stackFrame: IMoveCallStackFrame = {
            id: frameID,
            name: funName,
            srcFilePath,
            bcodeFilePath,
            srcFileHash,
            bcodeFileHash,
            // lines will be updated when next event (Instruction) is processed
            srcLine: 0,
            bcodeLine: 0,
            localsTypes,
            localsInfo,
            locals,
            lastCallInstructionSrcLine: undefined,
            lastCallInstructionBcodeLine: undefined,
            optimizedSrcLines,
            optimizedBcodeLines,
            // diassembly related fields will be set by the caller
            // as this function is executed in different contexts
            disassemblyModeTriggered: false,
            disassemblyView: false,
        };

        if (this.trace.events.length <= this.eventIndex + 1 ||
            (this.trace.events[this.eventIndex + 1].type !== TraceEventKind.Instruction &&
                this.trace.events[this.eventIndex + 1].type !== TraceEventKind.OpenFrame)
        ) {
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
     * Returns a string representing the current state of the runtime.
     *
     * @returns string representation of the runtime.
     */
    public toString(): string {
        let res = '';
        const summaryFrame = this.eventsStack.summaryFrame;
        let initialTab = '';
        if (summaryFrame) {
            res += 'current event stack:\n';
            res += this.singleTab + `summary frame (line ${summaryFrame.line}): \n`;
            for (const summary of summaryFrame.summary) {
                const summaryStr = typeof summary === 'string'
                    ? summary
                    : 'MoveCall';
                res += this.singleTab + this.singleTab + summaryStr + '\n';
            }
            initialTab = this.singleTab;
        }
        const eventFrame = this.eventsStack.eventFrame;
        if (eventFrame) {
            if ('frames' in eventFrame && 'globals' in eventFrame) {
                const moveCallStack = eventFrame as IMoveCallStack;
                res += this.moveCallStacktoString(moveCallStack, initialTab);
            } else if ('id' in eventFrame && 'line' in eventFrame &&
                'description' in eventFrame && 'locals' in eventFrame) {
                // Move call stask is only needed for variable conversion when a Move call is
                // being executed. For external events , we don't need it so we can pass
                // an empty one.
                const moveCallStack: IMoveCallStack = { frames: [], globals: new Map() };
                res += initialTab + 'event frame: \n';
                res += initialTab + this.singleTab + eventFrame.description + '\n';
                for (const local of eventFrame.locals) {
                    res += this.varToString(
                        initialTab + this.singleTab + this.singleTab,
                        local,
                        false
                    ) + '\n';
                }
            }
        } else {
            res += initialTab + 'no active event frame\n';
        }
        return res;
    }

    /**
     * Returns a string representing current Move call stack.
     *
     * @param moveCallStack current Move call stack.
     * @param tabs indentation for the string representation.
     * @returns string representation of the runtime.
     */
    public moveCallStacktoString(moveCallStack: IMoveCallStack, tabs: string): string {
        let res = tabs + 'current frame stack:\n';
        for (const frame of moveCallStack.frames) {
            const fileName = frame.disassemblyModeTriggered ?
                path.basename(frame.bcodeFilePath!) :
                path.basename(frame.srcFilePath);
            const line = frame.disassemblyModeTriggered ? frame.bcodeLine : frame.srcLine;
            res += tabs + this.singleTab
                + 'function: '
                + frame.name
                + ' ('
                + fileName
                + ':'
                + line
                + ')\n';
            for (let i = 0; i < frame.locals.length; i++) {
                res += tabs + this.singleTab + this.singleTab + 'scope ' + i + ' :\n';
                for (let j = 0; j < frame.locals[i].length; j++) {
                    const local = frame.locals[i][j];
                    if (local && (frame.disassemblyView || !isGeneratedLocal(local.info))) {
                        // don't show "artificial" locals outside of the disassembly view
                        res += this.varToString(
                            tabs + this.singleTab + this.singleTab + this.singleTab,
                            local,
                            frame.disassemblyView
                        ) + '\n';
                    }
                }
            }
        }
        if (this.lineBreakpoints && this.lineBreakpoints.size > 0) {
            res += 'line breakpoints\n';
            for (const [file, breakpoints] of this.lineBreakpoints) {
                res += tabs + this.singleTab + path.basename(file) + '\n';
                for (const line of breakpoints) {
                    res += tabs + this.singleTab + this.singleTab + line + '\n';
                }
            }
        }
        return res;
    }
    /**
     * Returns a string representation of a runtime variable.
     *
     * @param tabs indentation for the string representation.
     * @param variable runtime variable.
     * @returns string representation of the variable.
     */
    private varToString(
        tabs: string,
        variable: IRuntimeVariable,
        showDisassembly: boolean
    ): string {
        let varName = showDisassembly
            ? variable.info.internalName
            : variable.info.name;
        return this.valueToString(tabs, variable.value, varName, [], variable.type);
    }

    /**
     * Returns a string representation of a runtime compound value.
     *
     * @param tabs indentation for the string representation.
     * @param compoundValue runtime compound value.
     * @returns string representation of the compound value.
     */
    private compoundValueToString(
        tabs: string,
        compoundValue: IRuntimeCompoundValue
    ): string {
        const type = compoundValue.variantName
            ? compoundValue.type + '::' + compoundValue.variantName
            : compoundValue.type;
        let res = '(' + type + ') {\n';
        for (const [name, value] of compoundValue.fields) {
            res += this.valueToString(tabs + this.singleTab, value, name, []);
        }
        res += tabs + '}\n';
        return res;
    }

    /**
     * Returns a string representation of a runtime reference value.
     *
     * @param tabs indentation for the string representation.
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
        // Reference values are only present in Move calls when
        // the Move call stack is present in the event frame
        const moveCallStack = this.stack().eventFrame as IMoveCallStack;
        const indexedLoc = refValue.indexedLoc;
        let res = '';
        if ('globalIndex' in indexedLoc.loc) {
            // global location
            const globalValue = moveCallStack.globals.get(indexedLoc.loc.globalIndex);
            if (globalValue) {
                const indexPath = [...indexedLoc.indexPath];
                return this.valueToString(tabs, globalValue, name, indexPath, type);
            }
        } else if ('frameID' in indexedLoc.loc && 'localIndex' in indexedLoc.loc) {
            const frameID = indexedLoc.loc.frameID;
            const frame = moveCallStack.frames.find(frame => frame.id === frameID);
            let local = undefined;
            if (!frame) {
                return res;
            }
            for (const scope of frame.locals) {
                local = scope[indexedLoc.loc.localIndex];
                if (local) {
                    break;
                }
            }
            if (!local) {
                return res;
            }
            const indexPath = [...indexedLoc.indexPath];
            return this.valueToString(tabs, local.value, name, indexPath, type);
        }
        return res;
    }

    /**
     * Returns a string representation of a runtime value.
     *
     * @param value runtime value.
     * @param name name of the variable containing the value.
     * @param indexPath a path to actual value for compound types (e.g, [1, 7] means
     * first field/vector element and then seventh field/vector element)
     * @param type optional type of the variable containing the value.
     * @returns string representation of the value.
     */
    private valueToString(
        tabs: string,
        value: RuntimeValueType,
        name: string,
        indexPath: number[],
        type?: string
    ): string {
        let res = '';
        if (typeof value === 'string') {
            res += tabs + name + ' : ' + value + '\n';
            if (type) {
                res += tabs + 'type: ' + type + '\n';
            }
        } else if (Array.isArray(value)) {
            if (indexPath.length > 0) {
                const index = indexPath.pop();
                if (index !== undefined) {
                    res += this.valueToString(tabs, value[index], name, indexPath, type);
                }
            } else {
                res += tabs + name + ' : [\n';
                for (let i = 0; i < value.length; i++) {
                    res += this.valueToString(tabs + this.singleTab, value[i], String(i), indexPath);
                }
                res += tabs + ']\n';
                if (type) {
                    res += tabs + 'type: ' + type + '\n';
                }
            }
        } else if ('fields' in value) {
            if (indexPath.length > 0) {
                const index = indexPath.pop();
                if (index !== undefined) {
                    res += this.valueToString(tabs, value.fields[index][1], name, indexPath, type);
                }
            } else {
                res += tabs + name + ' : ' + this.compoundValueToString(tabs, value);
                if (type) {
                    res += tabs + 'type: ' + type + '\n';
                }
            }
        } else {
            res += this.refValueToString(tabs, value, name, type);
        }
        return res;
    }
}

/**
 * Checks if a given local is generated by the compiler
 * (as opposed to being declared in the source code).
 *
 * @param local local variable info.
 * @returns `true` if the local is generated, `false` otherwise.
 */
function isGeneratedLocal(local: ILocalInfo): boolean {
    return local.name.includes('%') || local.internalName.includes('%');
}

/**
 * Creates a map from a file hash to file information for all Move source files in a directory.
 *
 * @param directory path to the directory containing Move source files.
 * @param filesMap map to update with file information.
 * @param extension file extension of a Move source file or a disassembled bytecode file.
 */
function hashToFileMap(
    directory: string,
    filesMap: Map<string, IFileInfo>,
    extension: String
): void {
    const processDirectory = (dir: string) => {
        const files = fs.readdirSync(dir);
        for (const f of files) {
            const filePath = path.join(dir, f);
            const stats = fs.statSync(filePath);
            if (stats.isDirectory()) {
                processDirectory(filePath);
            } else if (path.extname(f) === extension) {
                const [fileHash, fileInfo] = createFileInfo(filePath);
                filesMap.set(fileHash, fileInfo);
            }
        }
    };

    processDirectory(directory);
}

/**
 * Handles a write to a local variable in a stack frame.
 *
 * @param frame stack frame frame.
 * @param frameIdx index of the frame in the stack.
 * @param localIndex variable index in the frame.
 * @param runtimeValue variable value.
 */
function localWrite(
    frame: IMoveCallStackFrame,
    frameIdx: number,
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
    const localInfo = frame.localsInfo[localIndex];
    if (!localInfo) {
        throw new Error('Cannot find local variable at index: '
            + localIndex
            + ' in function: '
            + frame.name);
    }

    const scopesCount = frame.locals.length;
    if (scopesCount <= 0) {
        throw new Error("There should be at least one variable scope in function"
            + frame.name);
    }
    // If a variable has the same name in the source but a different index (it is shadowed)
    // it has to be put in a different scope (e.g., locals[1], locals[2], etc.).
    // Find scope already containing variable name, if any, starting from
    // the outermost one
    let existingVarScope = -1;
    if (!isGeneratedLocal(localInfo)) {
        // Locals generated by the compiler are only shown in the disassembly view
        // where they have distinct names. In the source view, their names may not
        // be distinct (as variable names are subject to split on `#` character
        // to recover source names from compiler-level names), but if we shadowed
        // them, we would end up in empty scopes in the source view (as compiler-generated
        // locals are not there), and in the disassembly view, there is not need for
        // shadow scopes due to their distinct names. In summary, compiler-generated
        // variables don't need to be put in shadow scopes.
        for (let i = scopesCount - 1; i >= 0; i--) {
            const existingVarIndex = frame.locals[i].findIndex(runtimeVar => {
                return runtimeVar && runtimeVar.info.name === localInfo.name;
            });
            if (existingVarIndex !== -1 && existingVarIndex !== localIndex) {
                existingVarScope = i;
                break;
            }
        }
    }
    if (existingVarScope >= 0) {
        const shadowedScope = frame.locals[existingVarScope + 1];
        if (!shadowedScope) {
            frame.locals.push([]);
        }
        frame.locals[existingVarScope + 1][localIndex] = { info: localInfo, value, type, frameIdx };
    } else {
        // put variable in the "main" locals scope
        frame.locals[0][localIndex] = { info: localInfo, value, type, frameIdx };
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
