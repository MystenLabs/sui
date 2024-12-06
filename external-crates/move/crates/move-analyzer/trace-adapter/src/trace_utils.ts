// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import * as fs from 'fs';
import { FRAME_LIFETIME, ModuleInfo } from './utils';
import {
    IRuntimeCompoundValue,
    RuntimeValueType,
    IRuntimeVariableLoc,
    IRuntimeRefValue
} from './runtime';
import { ISourceMap, IFileLoc, IFileInfo, ILoc, ISourceMapFunction } from './source_map_utils';


// Data types corresponding to trace file JSON schema.

interface JSONTraceModule {
    address: string;
    name: string;
}

interface JSONStructTypeDescription {
    address: string;
    module: string;
    name: string;
    type_args: string[];
}

interface JSONStructType {
    struct: JSONStructTypeDescription;
}

interface JSONVectorType {
    vector: JSONBaseType;
}

type JSONBaseType = string | JSONStructType | JSONVectorType;

enum JSONTraceRefType {
    Mut = 'Mut',
    Imm = 'Imm'
}

interface JSONTraceType {
    type_: JSONBaseType;
    ref_type?: JSONTraceRefType
}

type JSONTraceRuntimeValueType = boolean | number | string | JSONTraceRuntimeValueType[] | JSONTraceCompound;

interface JSONTraceFields {
    [key: string]: JSONTraceRuntimeValueType;
}

interface JSONTraceCompound {
    fields: JSONTraceFields;
    type: string;
    variant_name?: string;
    variant_tag?: number;
}

interface JSONTraceRefValueContent {
    location: JSONTraceLocation;
    snapshot: JSONTraceRuntimeValueType;
}

interface JSONTraceMutRefValue {
    MutRef: JSONTraceRefValueContent;
}

interface JSONTraceImmRefValue {
    ImmRef: JSONTraceRefValueContent;
}

interface JSONTraceRuntimeValueContent {
    value: JSONTraceRuntimeValueType;
}

interface JSONTraceRuntimeValue {
    RuntimeValue: JSONTraceRuntimeValueContent;
}

export type JSONTraceRefValue = JSONTraceMutRefValue | JSONTraceImmRefValue;

export type JSONTraceValue = JSONTraceRuntimeValue | JSONTraceRefValue;

interface JSONTraceFrame {
    binary_member_index: number;
    frame_id: number;
    function_name: string;
    is_native: boolean;
    locals_types: JSONTraceType[];
    module: JSONTraceModule;
    parameters: JSONTraceValue[];
    return_types: JSONTraceType[];
    type_instantiation: string[];
}

interface JSONTraceOpenFrame {
    frame: JSONTraceFrame;
    gas_left: number;
}

interface JSONTraceInstruction {
    gas_left: number;
    instruction: string;
    pc: number;
    type_parameters: any[];
}

interface JSONTraceLocalLocation {
    Local: [number, number];
}

interface JSONTraceIndexedLocation {
    Indexed: [JSONTraceLocalLocation, number];
}

type JSONTraceLocation = JSONTraceLocalLocation | JSONTraceIndexedLocation;

interface JSONTraceWriteEffect {
    location: JSONTraceLocation;
    root_value_after_write: JSONTraceRuntimeValue;
}

interface JSONTraceReadEffect {
    location: JSONTraceLocation;
    moved: boolean;
    root_value_read: JSONTraceRuntimeValue;
}

interface JSONTracePushEffect {
    RuntimeValue?: JSONTraceRuntimeValueContent;
    MutRef?: {
        location: JSONTraceLocation;
        snapshot: any[];
    };
}

interface JSONTracePopEffect {
    RuntimeValue?: JSONTraceRuntimeValueContent;
    MutRef?: {
        location: JSONTraceLocation;
        snapshot: any[];
    };
}

interface JSONTraceEffect {
    Push?: JSONTracePushEffect;
    Pop?: JSONTracePopEffect;
    Write?: JSONTraceWriteEffect;
    Read?: JSONTraceReadEffect;
    ExecutionError?: string;

}

interface JSONTraceCloseFrame {
    frame_id: number;
    gas_left: number;
    return_: JSONTraceRuntimeValueContent[];
}

interface JSONTraceEvent {
    OpenFrame?: JSONTraceOpenFrame;
    Instruction?: JSONTraceInstruction;
    Effect?: JSONTraceEffect;
    CloseFrame?: JSONTraceCloseFrame;
}

interface JSONTraceRootObject {
    events: JSONTraceEvent[];
    version: number;
}

// Runtime data types.

/**
 * Kind of instruction in the trace. Enum member names correspond to instruction names.
 * (other than UNKNOWN which is used for instructions whose kind does not matter).
 */
export enum TraceInstructionKind {
    /**
     * Call instruction.
     */
    CALL,
    /**
     * Generic call instruction.
     */
    CALL_GENERIC,
    // for now we don't care about other kinds of instructions
    UNKNOWN
}

/**
 * Kind of a trace event.
 */
export enum TraceEventKind {
    /**
     * Artificial event to replace the content of the current inlined frame
     * with the content of another frame. This is to make sure that there is
     * only one inlined frame on the stack at any given time as inlined frames
     * are not being pushed and popped symmetrically and need to be handled
     * differently than regular frames.
     */
    ReplaceInlinedFrame,
    OpenFrame,
    CloseFrame,
    Instruction,
    Effect
}

/**
 * Trace event types containing relevant data.
 */
export type TraceEvent =
    | {
        type: TraceEventKind.ReplaceInlinedFrame
        fileHash: string
        optimizedLines: number[]
    }
    | {
        type: TraceEventKind.OpenFrame,
        id: number,
        name: string,
        fileHash: string
        isNative: boolean,
        localsTypes: string[],
        localsNames: string[],
        paramValues: RuntimeValueType[]
        optimizedLines: number[]
    }
    | { type: TraceEventKind.CloseFrame, id: number }
    | { type: TraceEventKind.Instruction, pc: number, loc: ILoc, kind: TraceInstructionKind }
    | { type: TraceEventKind.Effect, effect: EventEffect };

/**
 * Kind of an effect of an instruction.
 */
export enum TraceEffectKind {
    Write = 'Write',
    ExecutionError = 'ExecutionError'
    // TODO: other effect types
}

/**
 * Effect of an instruction.
 */
export type EventEffect =
    | { type: TraceEffectKind.Write, loc: IRuntimeVariableLoc, value: RuntimeValueType }
    | { type: TraceEffectKind.ExecutionError, msg: string };

/**
 * Execution trace consisting of a sequence of trace events.
 */
interface ITrace {
    events: TraceEvent[];
    /**
     * Maps frame ID to an array of local variable lifetime ends
     * indexed by the local variable index in the trace
     * (variable lifetime end is PC of an instruction following
     * the last variable access).
     */
    localLifetimeEnds: Map<number, number[]>;

    /**
     * Maps file path to the lines of code present in the trace instructions
     * in functions defined in the file.
     */
    tracedLines: Map<string, Set<number>>;
}

/**
 * Information about the frame being currently processed used during trace generation.
 */
interface ITraceGenFrameInfo {
    /**
     * Frame ID.
     */
    ID: number;
    /**
     * PC locations traced in the frame
     */
    pcLocs: IFileLoc[];
    /**
     * Path to a file containing function represented by the frame.
     */
    filePath: string;
    /**
     * Hash of a file containing function represented by the frame.
     */
    fileHash: string;
    /**
     * Code ines in a given file that have been optimized away.
     */
    optimizedLines: number[];
    /**
     * Name of the function represented by the frame.
     */
    funName: string;
    /**
     * Source map information for a given function.
     */
    funEntry: ISourceMapFunction;
}

/**
 * An ID of a virtual frame representing a macro defined in the same file
 * where it is inlined.
 */
const INLINED_FRAME_ID_SAME_FILE = -1;
/**
 * An ID of a virtual frame representing a macro defined in a different file
 * than file where it is inlined.
 */
const INLINED_FRAME_ID_DIFFERENT_FILE = -2;

/**
 * Reads a Move VM execution trace from a JSON file.
 *
 * @param traceFilePath path to the trace JSON file.
 * @param sourceMapsModMap a map from stringified module info to a source map.
 * @param sourceMapsHashMap a map from file hash to a source map.
 * @returns execution trace.
 * @throws Error with a descriptive error message if reading trace has failed.
 */
export function readTrace(
    traceFilePath: string,
    sourceMapsModMap: Map<string, ISourceMap>,
    sourceMapsHashMap: Map<string, ISourceMap>,
    filesMap: Map<string, IFileInfo>
): ITrace {
    const traceJSON: JSONTraceRootObject = JSON.parse(fs.readFileSync(traceFilePath, 'utf8'));
    if (traceJSON.events.length === 0) {
        throw new Error('Trace contains no events');
    }
    const events: TraceEvent[] = [];
    // We compute the end of lifetime for a local variable as follows.
    // When a given local variable is read or written in an effect, we set the end of its lifetime
    // to FRAME_LIFETIME. When a new instruction is executed, we set the end of its lifetime
    // to be the PC of this instruction. The caveat here is that we must use
    // the largest PC of all encountered instructions for this to avoid incorrectly
    // setting the end of lifetime to a smaller PC in case of a loop.
    //
    // For example, consider the following code:
    // ```
    // while (x < foo()) {
    //    x = x + 1;
    // }
    // ```
    // In this case (simplifying a bit), `x` should be live throughout
    // (unrolled in the trace) iterations of the loop. However, the last
    // instruction executed after `x` is accessed for the last time
    // will be `foo` whose PC is lower than PCs of instructions in/beyond
    // the loop
    const localLifetimeEnds = new Map<number, number[]>();
    const localLifetimeEndsMax = new Map<number, number[]>();
    const tracedLines = new Map<string, Set<number>>();
    // stack of frame infos OpenFrame and popped on CloseFrame
    const frameInfoStack: ITraceGenFrameInfo[] = [];
    for (const event of traceJSON.events) {
        if (event.OpenFrame) {
            const localsTypes = [];
            const frame = event.OpenFrame.frame;
            for (const type of frame.locals_types) {
                localsTypes.push(JSONTraceTypeToString(type.type_, type.ref_type));
            }
            // process parameters - store their values in trace and set their
            // initial lifetimes
            const paramValues = [];
            const lifetimeEnds = localLifetimeEnds.get(frame.frame_id) || [];
            for (let i = 0; i < frame.parameters.length; i++) {
                const value = frame.parameters[i];
                if (value) {
                    const runtimeValue: RuntimeValueType = 'RuntimeValue' in value
                        ? traceRuntimeValueFromJSON(value.RuntimeValue.value)
                        : traceRefValueFromJSON(value);

                    paramValues.push(runtimeValue);
                    lifetimeEnds[i] = FRAME_LIFETIME;
                }
            }
            localLifetimeEnds.set(frame.frame_id, lifetimeEnds);
            const modInfo = {
                addr: frame.module.address,
                name: frame.module.name
            };
            const sourceMap = sourceMapsModMap.get(JSON.stringify(modInfo));
            if (!sourceMap) {
                throw new Error('Source map for module '
                    + modInfo.name
                    + ' in package '
                    + modInfo.addr
                    + ' not found');
            }
            const funEntry = sourceMap.functions.get(frame.function_name);
            if (!funEntry) {
                throw new Error('Cannot find function entry in source map for function '
                    + frame.function_name
                    + ' when processing OpenFrame event');
            }
            events.push({
                type: TraceEventKind.OpenFrame,
                id: frame.frame_id,
                name: frame.function_name,
                fileHash: sourceMap.fileHash,
                isNative: frame.is_native,
                localsTypes,
                localsNames: funEntry.localsNames,
                paramValues,
                optimizedLines: sourceMap.optimizedLines
            });
            const currentFile = filesMap.get(sourceMap.fileHash);

            if (!currentFile) {
                throw new Error(`Cannot find file with hash: ${sourceMap.fileHash}`);
            }
            frameInfoStack.push({
                ID: frame.frame_id,
                pcLocs: funEntry.pcLocs,
                filePath: currentFile.path,
                fileHash: sourceMap.fileHash,
                optimizedLines: sourceMap.optimizedLines,
                funName: frame.function_name,
                funEntry
            });
        } else if (event.CloseFrame) {
            events.push({
                type: TraceEventKind.CloseFrame,
                id: event.CloseFrame.frame_id
            });
            frameInfoStack.pop();
        } else if (event.Instruction) {
            const name = event.Instruction.instruction;
            let frameInfo = frameInfoStack[frameInfoStack.length - 1];
            const fid = frameInfo.ID;
            const pcLocs = frameInfo.pcLocs;
            // if map does not contain an entry for a PC that can be found in the trace file,
            // it means that the position of the last PC in the source map should be used
            let instLoc = event.Instruction.pc >= pcLocs.length
                ? pcLocs[pcLocs.length - 1]
                : pcLocs[event.Instruction.pc];

            if (!instLoc) {
                throw new Error('Cannot find location for PC: '
                    + event.Instruction.pc
                    + ' in frame: '
                    + fid);
            }

            const differentFileVirtualFramePop = processInstructionIfMacro(
                sourceMapsHashMap,
                events,
                frameInfoStack,
                event.Instruction.pc,
                instLoc
            );

            if (differentFileVirtualFramePop) {
                // if we pop a virtual frame for a macro defined in a different file,
                // we may still land in a macro defined in the same file, in which case
                // we need to push another virtual frame for this instruction right away
                processInstructionIfMacro(
                    sourceMapsHashMap,
                    events,
                    frameInfoStack,
                    event.Instruction.pc,
                    instLoc
                );
            }


            // re-read frame info as it may have changed as a result of processing
            // and inlined call
            frameInfo = frameInfoStack[frameInfoStack.length - 1];
            const filePath = frameInfo.filePath;
            const lines = tracedLines.get(filePath) || new Set<number>();
            // floc is still good as the pc_locs used for its computation
            // do not change as a result of processing inlined frames
            lines.add(instLoc.loc.line);
            tracedLines.set(filePath, lines);
            events.push({
                type: TraceEventKind.Instruction,
                pc: event.Instruction.pc,
                loc: instLoc.loc,
                kind: name in TraceInstructionKind
                    ? TraceInstructionKind[name as keyof typeof TraceInstructionKind]
                    : TraceInstructionKind.UNKNOWN
            });

            // Set end of lifetime for all locals to the max instruction PC ever seen
            // for a given local (if they are live after this instructions, they will
            // be reset to FRAME_LIFETIME when processing subsequent effects).
            // All instructions in a given function, regardless of whether they are
            // in the inlined portion of the code or not, reset variable lifetimes.
            const nonInlinedFrameID = frameInfo.ID !== INLINED_FRAME_ID_SAME_FILE &&
                frameInfo.ID !== INLINED_FRAME_ID_DIFFERENT_FILE
                ? frameInfo.ID
                : frameInfoStack[frameInfoStack.length - 2].ID;
            const lifetimeEnds = localLifetimeEnds.get(nonInlinedFrameID) || [];
            const lifetimeEndsMax = localLifetimeEndsMax.get(nonInlinedFrameID) || [];
            for (let i = 0; i < lifetimeEnds.length; i++) {
                if (lifetimeEnds[i] === undefined || lifetimeEnds[i] === FRAME_LIFETIME) {
                    // only set new end of lifetime if it has not been set before
                    // or if variable is live
                    const pc = event.Instruction.pc;
                    if (lifetimeEndsMax[i] === undefined || lifetimeEndsMax[i] < pc) {
                        lifetimeEnds[i] = pc;
                        lifetimeEndsMax[i] = pc;
                    }
                }
            }
            localLifetimeEnds.set(nonInlinedFrameID, lifetimeEnds);
            localLifetimeEndsMax.set(nonInlinedFrameID, lifetimeEndsMax);
        } else if (event.Effect) {
            const effect = event.Effect;
            if (effect.Write || effect.Read) {
                // if a local is read or written, set its end of lifetime
                // to infinite (end of frame)
                const location = effect.Write ? effect.Write.location : effect.Read!.location;
                const loc = processJSONLocalLocation(location, localLifetimeEnds);
                if (effect.Write) {
                    if (!loc) {
                        throw new Error('Unsupported location type in Write effect');
                    }
                    // process a write only if the location is supported
                    const value = 'RuntimeValue' in effect.Write.root_value_after_write
                        ? traceRuntimeValueFromJSON(effect.Write.root_value_after_write.RuntimeValue.value)
                        : traceRefValueFromJSON(effect.Write.root_value_after_write);
                    events.push({
                        type: TraceEventKind.Effect,
                        effect: {
                            type: TraceEffectKind.Write,
                            loc,
                            value
                        }
                    });
                }
            }
            if (effect.ExecutionError) {
                events.push({
                    type: TraceEventKind.Effect,
                    effect: {
                        type: TraceEffectKind.ExecutionError,
                        msg: effect.ExecutionError
                    }
                });
            }
        }
    }
    return { events, localLifetimeEnds, tracedLines };
}

/**
 * Additional processing of an instruction if it's detected that it belongs
 * to an inlined macro. If this is the case, then virtual frames may be pushed
 * to the stack or popped from it.
 *
 * @param sourceMapsHashMap a map from file hash to a source map.
 * @param events trace events.
 * @param frameInfoStack stack of frame infos used during trace generation.
 * @param instPC PC of the instruction.
 * @param instLoc location of the instruction.
 * @returns `true` if this instruction caused a pop of a virtual frame for
 * an inlined macro defined in a different file, `false` otherwise.
 */
function processInstructionIfMacro(
    sourceMapsHashMap: Map<string, ISourceMap>,
    events: TraceEvent[],
    frameInfoStack: ITraceGenFrameInfo[],
    instPC: number,
    instLoc: IFileLoc
): boolean {
    let frameInfo = frameInfoStack[frameInfoStack.length - 1];
    const fid = frameInfo.ID;
    if (instLoc.fileHash !== frameInfo.fileHash) {
        // This indicates that we are going to an instruction in the same function
        // but in a different file, which can happen due to macro inlining.
        // One could think of "outlining" the inlined code to create separate
        // frames for each inlined macro but unfortunately this will not quite work.
        // The reason is that we cannot rely on these the inlined frame pushes and pops
        // being symmetric. Consider the following example:
        //```
        // macro fun baz() {
        //     ...
        // }
        // macro fun bar() {
        //     baz!();
        //     ...
        // }
        // fun foo() {
        //     bar!();
        // }
        //```
        // In the example above, according to the trace, there will be only
        // one inlined frame push as the first instruction of function `foo`
        // will be an instruction in macro `baz` instead of an instruction
        // in macro `bar`. Yet, when the control flow exits `baz`, it will go
        // to `bar`, and then to `foo`.
        //
        // The high level idea of how to handle this situation is to always
        // keep only a single inlined frame on the stack:
        // - the first time we see different file hashes, we push an inlined
        //   frame on the stack
        // - if an inlined frame is already on the stack, and the next file
        //   hash transition happens, then we do ond of the following:
        //   - if the next file hash is the same as the file hash of the frame
        //     before the current one, we pop the current inlined frame
        //   - otherwise, we replace the current inlined frame with the new one
        //
        // The exception to this single-inlined-frame rule is when we are already
        // in an inlined frame for a macro defined in the same file, and go to
        // a macro in a different file. In this case, we will have two inlined
        // frames on the stack.
        if (frameInfoStack.length > 1 &&
            frameInfoStack[frameInfoStack.length - 2].fileHash === instLoc.fileHash
        ) {
            frameInfoStack.pop();
            events.push({
                type: TraceEventKind.CloseFrame,
                id: fid
            });
            return true;
        } else {
            const sourceMap = sourceMapsHashMap.get(instLoc.fileHash);
            if (!sourceMap) {
                throw new Error('Cannot find source map for file with hash: '
                    + instLoc.fileHash
                    + ' when frame switching within frame '
                    + fid
                    + ' at PC '
                    + instPC);
            }
            if (frameInfo.ID === INLINED_FRAME_ID_DIFFERENT_FILE) {
                events.push({
                    type: TraceEventKind.ReplaceInlinedFrame,
                    fileHash: instLoc.fileHash,
                    optimizedLines: sourceMap.optimizedLines
                });
                // pop the current inlined frame so that it can
                // be replaced on the frame info stack below
                frameInfoStack.pop();
            } else {
                events.push({
                    type: TraceEventKind.OpenFrame,
                    id: INLINED_FRAME_ID_DIFFERENT_FILE,
                    name: '__inlined__',
                    fileHash: instLoc.fileHash,
                    isNative: false,
                    localsTypes: [],
                    localsNames: [],
                    paramValues: [],
                    optimizedLines: sourceMap.optimizedLines
                });
            }
            frameInfoStack.push({
                ID: INLINED_FRAME_ID_DIFFERENT_FILE,
                // same pcLocs as before since we are in the same function
                pcLocs: frameInfo.pcLocs,
                filePath: sourceMap.filePath,
                fileHash: sourceMap.fileHash,
                optimizedLines: sourceMap.optimizedLines,
                // same function name and source map as before since we are in the same function
                funName: frameInfo.funName,
                funEntry: frameInfo.funEntry
            });
        }
    } else if (frameInfo.ID !== INLINED_FRAME_ID_DIFFERENT_FILE) {
        // We are in the same file here, though perhaps this instruction
        // belongs to an inlined macro. If we are already in an inlined
        // frame for a macro defined in a different file, we don't do
        // anything do avoid pushing a new inlined frame for a macro.
        //
        // Otherwise, below we check if instruction belongs to an inlined macro
        // when this macro is defined in the same file to provide similar
        // behavior as when the macro is defined in a different file
        // (push/pop virtual inlined frames). The implementation here is
        // a bit different, though, as we don't have explicit boundaries
        // for when the code transitions from/to inlined code. Instead,
        // we need to inspect each instruction and act as follows:
        // - if the instruction is outside of the function (belongs to inlined macro):
        //   - if we are not in an inlined frame, we need to push one
        //   - if we are in an inlined frame, we don't need to do anything
        // - if the instruction is in the function:
        //   - if we are in an inlined frame, we need to pop it
        //   - if we are not in an inlined frame, we don't need to do anything
        if (instLoc.loc.line < frameInfo.funEntry.startLoc.line ||
            instLoc.loc.line > frameInfo.funEntry.endLoc.line ||
            (instLoc.loc.line === frameInfo.funEntry.startLoc.line &&
                instLoc.loc.column < frameInfo.funEntry.startLoc.column) ||
            (instLoc.loc.line === frameInfo.funEntry.endLoc.line &&
                instLoc.loc.column > frameInfo.funEntry.endLoc.column)) {
            // the instruction is outside of the function
            // (belongs to inlined macro)
            if (frameInfo.ID !== INLINED_FRAME_ID_SAME_FILE) {
                // if we are not in an inlined frame, we need to push one
                events.push({
                    type: TraceEventKind.OpenFrame,
                    id: INLINED_FRAME_ID_SAME_FILE,
                    name: '__inlined__',
                    fileHash: instLoc.fileHash,
                    isNative: false,
                    localsTypes: [],
                    localsNames: [],
                    paramValues: [],
                    optimizedLines: frameInfo.optimizedLines
                });
                // we get a lot of data for the new frame info from the current on
                // since we are still in the same function
                frameInfoStack.push({
                    ID: INLINED_FRAME_ID_SAME_FILE,
                    pcLocs: frameInfo.pcLocs,
                    filePath: frameInfo.filePath,
                    fileHash: instLoc.fileHash,
                    optimizedLines: frameInfo.optimizedLines,
                    funName: frameInfo.funName,
                    funEntry: frameInfo.funEntry
                });
            } // else we are already in an inlined frame, so we don't need to do anything
        } else {
            // the instruction is in the function
            if (frameInfo.ID === INLINED_FRAME_ID_SAME_FILE) {
                // If we are in an inlined frame, we need to pop it.
                // This the place where we need different inlined frame id
                // for macros defined in the same or different file than
                // the file where they are inlined. Since this check is executed
                // for each instruction that is within the function, we could
                // accidentally (and incorrectly) at this point pop virtual inlined
                // frame for a macro defined in a different file, if we did could not
                // distinguish between the two cases.
                events.push({
                    type: TraceEventKind.CloseFrame,
                    id: INLINED_FRAME_ID_SAME_FILE
                });
                frameInfoStack.pop();
            } // else we are not in an inlined frame, so we don't need to do anything
        }
    }
    return false;
}




/**
 * Converts a JSON trace type to a string representation.
 */
function JSONTraceTypeToString(baseType: JSONBaseType, refType?: JSONTraceRefType): string {
    const refPrefix = refType === JSONTraceRefType.Mut
        ? '&mut '
        : (refType === JSONTraceRefType.Imm
            ? '&'
            : '');
    if (typeof baseType === 'string') {
        return refPrefix + baseType;
    } else if ('vector' in baseType) {
        return refPrefix + `vector<${JSONTraceTypeToString(baseType.vector)}>`;
    } else {
        return refPrefix
            + JSONTraceAddressToHexString(baseType.struct.address)
            + "::"
            + baseType.struct.module
            + "::"
            + baseType.struct.name;
    }
}

/**
 * Attempts to convert an address found in the trace (which is a string
 * representing a 32-byte number) to a shorter and more readable hex string.
 * Returns original string address if conversion fails.
 */
function JSONTraceAddressToHexString(address: string): string {
    try {
        const number = BigInt(address);
        const hexAddress = number.toString(16);
        return `0x${hexAddress}`;
    } catch (error) {
        // Return the original string if it's not a valid number
        return address;
    }
}

/**
 * Processes a location of a local variable in a JSON trace: sets the end of its lifetime
 * when requested and returns its location
 * @param traceLocation location in the trace.
 * @param localLifetimeEnds map of local variable lifetimes (defined if local variable
 * lifetime should happen).
 * @returns variable location.
 */
function processJSONLocalLocation(
    traceLocation: JSONTraceLocation,
    localLifetimeEnds?: Map<number, number[]>,
): IRuntimeVariableLoc | undefined {
    if ('Local' in traceLocation) {
        const frameID = traceLocation.Local[0];
        const localIndex = traceLocation.Local[1];
        if (localLifetimeEnds) {
            const lifetimeEnds = localLifetimeEnds.get(frameID) || [];
            lifetimeEnds[localIndex] = FRAME_LIFETIME;
            localLifetimeEnds.set(frameID, lifetimeEnds);
        }
        return { frameID, localIndex };
    } else if ('Indexed' in traceLocation) {
        return processJSONLocalLocation(traceLocation.Indexed[0], localLifetimeEnds);
    } else {
        // Currently, there is nothing that needs to be done for 'Global' locations,
        // neither with respect to lifetime nor with respect to location itself.
        // This is because `Global` locations currently only represent read-only
        // reference values returned from native functions. If there ever was
        // a native function that would return a mutable reference, we should
        // consider how to handle value changes via such reference, but it's unlikely
        // that such a function would ever be added to either Move stdlib or
        // the Sui framework.
        return undefined;
    }
}

/**
 * Converts a JSON trace reference value to a runtime value.
 *
 * @param value JSON trace reference value.
 * @returns runtime value.
 * @throws Error with a descriptive error message if conversion has failed.
 */
function traceRefValueFromJSON(value: JSONTraceRefValue): RuntimeValueType {
    if ('MutRef' in value) {
        const loc = processJSONLocalLocation(value.MutRef.location);
        if (!loc) {
            throw new Error('Unsupported location type in MutRef');
        }
        const ret: IRuntimeRefValue = { mutable: true, loc };
        return ret;
    } else {
        const loc = processJSONLocalLocation(value.ImmRef.location);
        if (!loc) {
            throw new Error('Unsupported location type in ImmRef');
        }
        const ret: IRuntimeRefValue = { mutable: false, loc };
        return ret;
    }
}

/**
 * Converts a JSON trace runtime value to a runtime trace value.
 *
 * @param value JSON trace runtime value.
 * @returns runtime trace value.
 */
function traceRuntimeValueFromJSON(value: JSONTraceRuntimeValueType): RuntimeValueType {
    if (typeof value === 'boolean'
        || typeof value === 'number'
        || typeof value === 'string') {
        return String(value);
    } else if (Array.isArray(value)) {
        return value.map(item => traceRuntimeValueFromJSON(item));
    } else {
        const fields: [string, RuntimeValueType][] =
            Object.entries(value.fields).map(([key, value]) => [key, traceRuntimeValueFromJSON(value)]);
        const compoundValue: IRuntimeCompoundValue = {
            fields,
            type: value.type,
            variantName: value.variant_name,
            variantTag: value.variant_tag
        };
        return compoundValue;
    }
}

//
// Utility functions for testing and debugging.
//

/**
 * Converts trace events to an array of strings
 * representing these events.
 *
 * @param trace trace.
 * @returns array of strings representing trace events.
 */
export function traceEventsToString(trace: ITrace): string[] {
    return trace.events.map(event => eventToString(event));
}

/**
 * Converts a trace event to a string representation.
 *
 * @param event trace event.
 * @returns string representation of the event.
 */
function eventToString(event: TraceEvent): string {
    switch (event.type) {
        case TraceEventKind.ReplaceInlinedFrame:
            return 'ReplaceInlinedFrame';
        case TraceEventKind.OpenFrame:
            return `OpenFrame ${event.id} for ${event.name}`;
        case TraceEventKind.CloseFrame:
            return `CloseFrame ${event.id}`;
        case TraceEventKind.Instruction:
            return 'Instruction '
                + instructionKindToString(event.kind)
                + ' at PC '
                + event.pc
                + ', line '
                + event.loc.line;
        case TraceEventKind.Effect:
            return `Effect ${effectToString(event.effect)}`;
    }
}

/**
 * Converts a trace instruction kind to a string representation.
 *
 * @param kind instruction kind.
 * @returns string representation of the instruction kind.
 */
function instructionKindToString(kind: TraceInstructionKind): string {
    switch (kind) {
        case TraceInstructionKind.CALL:
            return 'CALL';
        case TraceInstructionKind.CALL_GENERIC:
            return 'CALL_GENERIC';
        case TraceInstructionKind.UNKNOWN:
            return 'UNKNOWN';
    }
}

/**
 * Converts an effect of an instruction to a string representation.
 *
 * @param effect effect.
 * @returns string representation of the effect.
 */
function effectToString(effect: EventEffect): string {
    switch (effect.type) {
        case TraceEffectKind.Write:
            return `Write at idx ${effect.loc.localIndex} in frame ${effect.loc.frameID}`;
        case TraceEffectKind.ExecutionError:
            return `ExecutionError ${effect.msg}`;
    }
}