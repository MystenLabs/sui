// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import * as fs from 'fs';
import { FRAME_LIFETIME, ModuleInfo } from './utils';
import { IRuntimeCompundValue, RuntimeValueType } from './runtime';


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

interface JSONTraceType {
    ref_type: string | null;
    type_: JSONBaseType;
}

type JSONTraceValueType = boolean | number | string | JSONTraceValueType[] | JSONTraceCompound;

interface JSONTraceFields {
    [key: string]: JSONTraceValueType;
}

interface JSONTraceCompound {
    fields: JSONTraceFields;
    type: string;
    variant_name?: string;
    variant_tag?: number;
}

interface JSONTraceRuntimeValue {
    value: JSONTraceValueType;
}

interface JSONTraceValue {
    RuntimeValue: JSONTraceRuntimeValue;
}

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
    root_value_after_write: JSONTraceValue;
}

interface JSONTraceReadEffect {
    location: JSONTraceLocation;
    moved: boolean;
    root_value_read: JSONTraceValue;
}

interface JSONTracePushEffect {
    RuntimeValue?: JSONTraceRuntimeValue;
    MutRef?: {
        location: JSONTraceLocation;
        snapshot: any[];
    };
}

interface JSONTracePopEffect {
    RuntimeValue?: JSONTraceRuntimeValue;
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
}

interface JSONTraceCloseFrame {
    frame_id: number;
    gas_left: number;
    return_: JSONTraceRuntimeValue[];
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
        type: TraceEventKind.OpenFrame,
        id: number,
        name: string,
        modInfo: ModuleInfo,
        localsTypes: string[],
        paramValues: TraceValue[]
    }
    | { type: TraceEventKind.CloseFrame, id: number }
    | { type: TraceEventKind.Instruction, pc: number, kind: TraceInstructionKind }
    | { type: TraceEventKind.Effect, effect: EventEffect };

/**
 * Kind of a location in the trace.
 */
export enum TraceLocKind {
    Local = 'Local'
    // TODO: other location types
}

/**
 * Location in the trace.
 */
export type TraceLocation =
    | { type: TraceLocKind.Local, frameId: number, localIndex: number };

/**
 * Kind of a value in the trace.
 */
export enum TraceValKind {
    Runtime = 'RuntimeValue'
    // TODO: other value types
}

/**
 * Value in the trace.
 */
export type TraceValue =
    | { type: TraceValKind.Runtime, value: RuntimeValueType };

/**
 * Kind of an effect of an instruction.
 */
export enum TraceEffectKind {
    Write = 'Write'
    // TODO: other effect types
}

/**
 * Effect of an instruction.
 */
export type EventEffect =
    | { type: TraceEffectKind.Write, location: TraceLocation, value: TraceValue };

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
}

/**
 * Reads a Move VM execution trace from a JSON file.
 *
 * @param traceFilePath path to the trace JSON file.
 * @returns execution trace.
 */
export function readTrace(traceFilePath: string): ITrace {
    const traceJSON: JSONTraceRootObject = JSON.parse(fs.readFileSync(traceFilePath, 'utf8'));
    const events: TraceEvent[] = [];
    // We compute the end of lifetime for a local variable as follows.
    // When a given local variable is read or written in an effect, we set the end of its lifetime
    // to INFINITE_LIFETIME. When a new instruction is executed, we set the end of its lifetime
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
    const locaLifetimeEndsMax = new Map<number, number[]>();
    let frameIDs = [];
    for (const event of traceJSON.events) {
        if (event.OpenFrame) {
            const localsTypes = [];
            const frame = event.OpenFrame.frame;
            for (const type of frame.locals_types) {
                localsTypes.push(JSONTraceTypeToString(type.type_));
            }
            // process parameters - store their values in trace and set their
            // initial lifetimes
            const paramValues = [];
            const lifetimeEnds = localLifetimeEnds.get(frame.frame_id) || [];
            for (let i = 0; i < frame.parameters.length; i++) {
                const value = frame.parameters[i];
                if (value) {
                    const runtimeValue: TraceValue =
                    {
                        type: TraceValKind.Runtime,
                        value: traceValueFromJSON(value.RuntimeValue.value)
                    };
                    paramValues.push(runtimeValue);
                    lifetimeEnds[i] = FRAME_LIFETIME;
                }
            }
            localLifetimeEnds.set(frame.frame_id, lifetimeEnds);
            events.push({
                type: TraceEventKind.OpenFrame,
                id: frame.frame_id,
                name: frame.function_name,
                modInfo: {
                    addr: frame.module.address,
                    name: frame.module.name
                },
                localsTypes,
                paramValues,
            });
            frameIDs.push(frame.frame_id);
        } else if (event.CloseFrame) {
            events.push({
                type: TraceEventKind.CloseFrame,
                id: event.CloseFrame.frame_id
            });
            frameIDs.pop();
        } else if (event.Instruction) {
            const name = event.Instruction.instruction;
            events.push({
                type: TraceEventKind.Instruction,
                pc: event.Instruction.pc,
                kind: name in TraceInstructionKind
                    ? TraceInstructionKind[name as keyof typeof TraceInstructionKind]
                    : TraceInstructionKind.UNKNOWN
            });
            // Set end of lifetime for all locals to the max instruction PC ever seen
            // for a given local (if they are live after this instructions, they will
            // be reset to INFINITE_LIFETIME when processing subsequent effects).
            const currentFrameID = frameIDs[frameIDs.length - 1];
            const lifetimeEnds = localLifetimeEnds.get(currentFrameID) || [];
            const lifetimeEndsMax = locaLifetimeEndsMax.get(currentFrameID) || [];
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
            localLifetimeEnds.set(currentFrameID, lifetimeEnds);
            locaLifetimeEndsMax.set(currentFrameID, lifetimeEndsMax);
        } else if (event.Effect) {
            const effect = event.Effect;
            if (effect.Write || effect.Read) {
                // if a local is read or written, set its end of lifetime
                // to infinite (end of frame)
                const location = effect.Write ? effect.Write.location : effect.Read!.location;
                // there must be at least one frame on the stack when processing a write effect
                // so we can safely access the last frame ID
                const currentFrameID = frameIDs[frameIDs.length - 1];
                const localIndex = processJSONLocation(location, localLifetimeEnds, currentFrameID);
                if (localIndex === undefined) {
                    continue;
                }
                if (effect.Write) {
                    const value = traceValueFromJSON(effect.Write.root_value_after_write.RuntimeValue.value);
                    const traceValue: TraceValue = {
                        type: TraceValKind.Runtime,
                        value
                    };
                    const traceLocation: TraceLocation = {
                        type: TraceLocKind.Local,
                        frameId: currentFrameID,
                        localIndex
                    };
                    events.push({
                        type: TraceEventKind.Effect,
                        effect: {
                            type: TraceEffectKind.Write,
                            location: traceLocation,
                            value: traceValue
                        }
                    });
                }
            }
        }
    }
    return { events, localLifetimeEnds };
}

/**
 * Converts a JSON trace type to a string representation.
 */
function JSONTraceTypeToString(type: JSONBaseType): string {
    if (typeof type === 'string') {
        return type;
    } else if ('vector' in type) {
        return `vector<${JSONTraceTypeToString(type.vector)}>`;
    } else {
        return JSONTraceAddressToHexString(type.struct.address)
            + "::"
            + type.struct.module
            + "::"
            + type.struct.name;
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

/// Processes a location in a JSON trace (sets the end of lifetime for a local variable)
/// and returns the local index if the location is a local variable in the current frame.
function processJSONLocation(
    location: JSONTraceLocation,
    localLifetimeEnds: Map<number, number[]>,
    currentFrameID: number
): number | undefined {
    // TODO: handle Global and Indexed for other frames
    if ('Local' in location) {
        const frameId = location.Local[0];
        const localIndex = location.Local[1];
        const lifetimeEnds = localLifetimeEnds.get(frameId) || [];
        lifetimeEnds[localIndex] = FRAME_LIFETIME;
        localLifetimeEnds.set(frameId, lifetimeEnds);
        return localIndex;
    } else if ('Indexed' in location) {
        const frameId = location.Indexed[0].Local[0];
        if (frameId === currentFrameID) {
            const localIndex = location.Indexed[0].Local[1];
            const lifetimeEnds = localLifetimeEnds.get(frameId) || [];
            lifetimeEnds[localIndex] = FRAME_LIFETIME;
            localLifetimeEnds.set(frameId, lifetimeEnds);
            return localIndex;
        }
    }
    return undefined;
}

/// Converts a JSON trace value to a runtime trace value.
function traceValueFromJSON(value: JSONTraceValueType): RuntimeValueType {
    if (typeof value === 'boolean'
        || typeof value === 'number'
        || typeof value === 'string') {
        return String(value);
    } else if (Array.isArray(value)) {
        return value.map(item => traceValueFromJSON(item));
    } else {
        const fields: [string, RuntimeValueType][] =
            Object.entries(value.fields).map(([key, value]) => [key, traceValueFromJSON(value)]);
        const compoundValue: IRuntimeCompundValue = {
            fields,
            type: value.type,
            variantName: value.variant_name,
            variantTag: value.variant_tag
        };
        return compoundValue;
    }
}
