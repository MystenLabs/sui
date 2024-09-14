// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { logger } from '@vscode/debugadapter';
import { EventEmitter } from 'events';
import * as crypto from 'crypto';
import * as fs from 'fs';
import * as path from 'path';
import toml from 'toml';
import { ISourceMap, IFileInfo, readAllSourceMaps } from './source_map_utils';
import { TraceEvent, readTrace } from './trace_utils';
import { ModuleInfo } from './utils';

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

    // Trace being viewed.
    private trace = { events: [] as TraceEvent[] };

    // Index of the current trace event being processed.
    private eventIndex = 0;

    // Current frame stack.
    private frameStack = { frames: [] as IRuntimeStackFrame[] };

    // Map of file hashes to file info.
    private filesMap = new Map<string, IFileInfo>();

    // Map of stringified module info to source maps.
    private sourceMapsMap = new Map<string, ISourceMap>();

    /**
     * Start a trace viewing session and set up the initial state of the runtime.
     *
     * @param source  path to the Move source file whose traces are to be viewed.
     * @param traceInfo  trace selected for viewing.
     *
     */
    public async start(source: string, traceInfo: string, stopOnEntry: boolean): Promise<void> {
        logger.log("Starting runtime for file: " + source + " and trace: " + traceInfo);

        const pkgRoot = await findPkgRoot(source);
        if (!pkgRoot) {
            throw new Error("Cannot find package root for file: " + source);
        }
        const manifest_path = path.join(pkgRoot, 'Move.toml');

        // find package name from manifest file which corresponds `build` directory's subdirectory
        // name containing this package's build files
        const pkg_name = getPkgNameFromManifest(manifest_path);
        if (!pkg_name) {
            throw Error("Cannot find package name in manifest file: " + manifest_path);
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
        if (currentEvent.type !== 'OpenFrame') {
            throw new Error("First event in trace is not an OpenFrame event");
        }
        const newFrame =
            this.newStackFrame(currentEvent.id, currentEvent.name, currentEvent.modInfo);
        this.frameStack = {
            frames: [newFrame]
        };
        this.step(/* next */ false, /* stopAtCloseFrame */ false, /* nextLineSkip */ true);
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
    public step(
        next: boolean,
        stopAtCloseFrame: boolean,
        nextLineSkip: boolean
    ): boolean {
        this.eventIndex++;
        if (this.eventIndex >= this.trace.events.length) {
            this.sendEvent(RuntimeEvents.stopOnStep);
            return true;
        }
        let currentEvent = this.trace.events[this.eventIndex];
        if (currentEvent.type === 'Instruction') {
            let sameLine = this.instruction(currentEvent);
            if (sameLine && nextLineSkip) {
                return this.step(next, stopAtCloseFrame, nextLineSkip);
            }
            this.sendEvent(RuntimeEvents.stopOnStep);
            return false;
        } else if (currentEvent.type === 'OpenFrame') {
            // create a new frame and push it onto the stack
            const newFrame =
                this.newStackFrame(currentEvent.id, currentEvent.name, currentEvent.modInfo);
            this.frameStack.frames.push(newFrame);
            if (next) {
                // step out of the frame right away
                return this.stepOut();
            } else {
                return this.step(next, stopAtCloseFrame, nextLineSkip);
            }
        } else if (currentEvent.type === 'CloseFrame') {
            if (stopAtCloseFrame) {
                // don't do anything as the caller needs to inspect
                // the event before proceeing
                return false;
            } else {
                // pop the top frame from the stack
                this.frameStack.frames.pop();
                return this.step(next, stopAtCloseFrame, nextLineSkip);
            }
        } else {
            // ignore other events
            return this.step(next, stopAtCloseFrame, nextLineSkip);
        }
    }

    /**
     * Handles "step out" adapter action.
     *
     * @returns `true` if the trace viewing session is finished, `false` otherwise.
     * @throws Error with a descriptive error message if the step out event cannot be handled.
     */
    public stepOut(): boolean {
        const stackHeight = this.frameStack.frames.length;
        if (stackHeight <= 1) {
            // do nothing as there is no frame to step out to
            logger.log("At the outermost function, cannot step out");
            this.sendEvent(RuntimeEvents.stopOnStep);
            return false;
        }
        // newest frame is at the top of the stack
        const currentFrame = this.frameStack.frames[stackHeight - 1];
        let currentEvent = this.trace.events[this.eventIndex];
        // skip all events until the corresponding CloseFrame event,
        // pop the top frame from the stack, and proceed to the next event
        while (true) {
            if (this.step(/* next */ false, /* stopAtCloseFrame */ true, /* nextLineSkip */ true)) {
                // trace viewing session finished
                throw new Error("Cannot find corresponding CloseFrame event for function: " +
                    currentFrame.name);
            }
            currentEvent = this.trace.events[this.eventIndex];
            if (currentEvent.type === 'CloseFrame') {
                const currentFrameID = currentFrame.id;
                // `step` call finished at the CloseFrame event
                // but did not process it so we need pop the frame here
                this.frameStack.frames.pop();
                if (currentEvent.id === currentFrameID) {
                    break;
                }
            }
        }

        // Do not skip to same line when stepping out as this may lead
        // to unusual behavior if multiple bytcode instructions are on the same line.
        // For example, consider the following code:
        // ```
        // assert(foo() == bar());
        // ```
        // In the code above if we enter `foo` and then step out of it,
        // we want to end up on the same line (where the next instruction is)
        // but we don't want to call `bar` in the same debugging step.
        return this.step(/* next */ false, /* stopAtCloseFrame */ false, /* nextLineSkip */ false);
    }
    /**
     * Handles "step back" adapter action.
     * @returns `true` if was able to step back, `false` otherwise.
     * @throws Error with a descriptive error message if the step back event cannot be handled.
     */
    public stepBack(): boolean {
        if (this.eventIndex <= 1) {
            // no where to step back to (event 0 is the `OpenFrame` event for the first frame)
            // and is processed in runtime.start() which is executed only once
            logger.log("At the beginning of the trace, cannot step back");
            this.sendEvent(RuntimeEvents.stopOnStep);
            return false;
        }
        let currentEvent = this.trace.events[this.eventIndex - 1];
        if (currentEvent.type === 'CloseFrame') {
            // cannot step back into or over function calls
            logger.log("After a function call, cannot step back");
            this.sendEvent(RuntimeEvents.stopOnStep);
            return false;
        } else {
            this.eventIndex--;
            if (currentEvent.type === 'Instruction') {
                let sameLine = this.instruction(currentEvent);
                if (sameLine) {
                    this.stepBack();
                    return true;
                }
                this.sendEvent(RuntimeEvents.stopOnStep);
                return true;
            } else if (currentEvent.type === 'OpenFrame') {
                const stackHeight = this.frameStack.frames.length;
                if (stackHeight <= 0) {
                    // should never happen but better to signal than crash
                    throw new Error("Error stepping back to caller function "
                        + currentEvent.name
                        + " as there is no frame on the stack"
                    );
                }
                if (stackHeight <= 1) {
                    // should never happen as we never step back out of the outermost function
                    // (never step back to event 0 as per first conditional in this function)
                    throw new Error("Error stepping back to caller function "
                        + currentEvent.name
                        + " from callee "
                        + this.frameStack.frames[stackHeight - 1].name
                        + " as there would be no frame on the stack afterwards"
                    );
                }
                // pop the top frame from the stack
                this.frameStack.frames.pop();
                // cannot simply call stepBack as we are stepping back to the same line
                // that is now in the current frame, which would result in unintentionally
                // recursing to previous events
                if (this.eventIndex <= 1) {
                    // no where to step back to
                    logger.log("At the beginning of the trace, cannot step back");
                    this.sendEvent(RuntimeEvents.stopOnStep);
                    return true; // we actually stepped back just can't step back further
                }
                this.eventIndex--;
                let prevCurrentEvent = this.trace.events[this.eventIndex];
                if (prevCurrentEvent.type !== 'Instruction') {
                    throw new Error("Expected an Instruction event before OpenFrame event in function"
                        + currentEvent.name
                    );
                }
                if (!this.instruction(prevCurrentEvent)) {
                    // we should be steppping back to the instruction on the same line
                    // as the one in the current frame
                    throw new Error("Wrong line of an instruction (at PC " + prevCurrentEvent.pc + ")"
                        + " in the caller function"
                        + currentEvent.name
                        + " to step back to from callee "
                        + this.frameStack.frames[stackHeight - 1].name
                        + " as there would be no frame on the stack afterwards"
                    );
                }
                this.sendEvent(RuntimeEvents.stopOnStep);
                return true;
            } else {
                // ignore other events
                this.stepBack();
                return true;
            }
        }
    }

    /**
     * Handles "continue" adapter action.
     * @returns `true` if the trace viewing session is finished, `false` otherwise.
     * @throws Error with a descriptive error message if the continue event cannot be handled.
     */
    public continue(reverse: boolean): boolean {
        if (reverse) {
            while (true) {
                if (!this.stepBack()) {
                    return false;
                }
            }
        } else {
            while (true) {
                if (this.step(
                    /* next */ false,
                    /* stopAtCloseFrame */ false,
                    /* nextLineSkip */ true)
                ) {
                    return true;
                }
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
    private instruction(instructionEvent: Extract<TraceEvent, { type: 'Instruction' }>): boolean {
        const stackHeight = this.frameStack.frames.length;
        // newest frame is at the top of the stack
        let currentFrame = this.frameStack.frames[stackHeight - 1];
        const currentFun = currentFrame.sourceMap.functions.get(currentFrame.name);
        if (!currentFun) {
            throw new Error("Cannot find function: " + currentFrame.name + " in source map");
        }

        // if map does not contain an entry for a PC that can be found in the trace file,
        // it means that the position of the last PC in the source map should be used
        let currentPCLoc = instructionEvent.pc >= currentFun.pcLocs.length
            ? currentFun.pcLocs[currentFun.pcLocs.length - 1]
            : currentFun.pcLocs[instructionEvent.pc];

        if (!currentPCLoc) {
            throw new Error("Cannot find location for PC: "
                + instructionEvent.pc
                + " in function: "
                + currentFrame.name);
        }

        if (currentPCLoc.line === currentFrame.line) {
            // so that instructions on the same line can be bypassed
            return true;
        } else {
            currentFrame.line = currentPCLoc.line;
            return false;
        }
    }


    /**
     * Creates a new runtime stack frame based on info from the `OpenFrame` trace event.
     *
     * @param frameID frame identifier from the trace event.
     * @param funName function name.
     * @param modInfo information about module containing the function.
     * @returns new frame.
     * @throws Error with a descriptive error message if frame cannot be constructed.
     */
    private newStackFrame(frameID: number, funName: string, modInfo: ModuleInfo): IRuntimeStackFrame {
        const sourceMap = this.sourceMapsMap.get(JSON.stringify(modInfo));

        if (!sourceMap) {
            throw new Error("Cannot find source map for module: "
                + modInfo.name
                + " in package: "
                + modInfo.addr);
        }
        const currentFile = this.filesMap.get(sourceMap.fileHash);

        if (!currentFile) {
            throw new Error("Cannot find file with hash: " + sourceMap.fileHash);
        }

        const stackFrame: IRuntimeStackFrame = {
            sourceMap,
            id: frameID,
            name: funName,
            file: currentFile.path,
            line: 0 // line will be updated when next event (Instruction) is processed
        };

        if (this.trace.events.length <= this.eventIndex + 1 ||
            this.trace.events[this.eventIndex + 1].type !== 'Instruction') {
            throw new Error("Expected an Instruction event after OpenFrame event");
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
