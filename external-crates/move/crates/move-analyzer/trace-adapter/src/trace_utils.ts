// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import * as fs from 'fs';
import { FRAME_LIFETIME, ModuleInfo } from './utils';

// Data types corresponding to trace file JSON schema.

interface ITraceModule {
    address: string;
    name: string;
}

interface ITraceType {
    ref_type: string | null;
    type_: string;
}

interface ITraceRuntimeValue {
    value: any;
}

interface ITraceValue {
    RuntimeValue: ITraceRuntimeValue;
}

interface ITraceFrame {
    binary_member_index: number;
    frame_id: number;
    function_name: string;
    is_native: boolean;
    locals_types: ITraceType[];
    module: ITraceModule;
    parameters: ITraceValue[];
    return_types: ITraceType[];
    type_instantiation: string[];
}

interface ITraceOpenFrame {
    frame: ITraceFrame;
    gas_left: number;
}

interface ITraceInstruction {
    gas_left: number;
    instruction: string;
    pc: number;
    type_parameters: any[];
}

interface ITraceLocation {
    Local: [number, number];
}

interface ITraceWriteEffect {
    location: ITraceLocation;
    root_value_after_write: ITraceValue;
}

interface ITraceReadEffect {
    location: ITraceLocation;
    moved: boolean;
    root_value_read: ITraceValue;
}

interface ITracePushEffect {
    RuntimeValue?: ITraceRuntimeValue;
    MutRef?: {
        location: ITraceLocation;
        snapshot: any[];
    };
}

interface ITracePopEffect {
    RuntimeValue?: ITraceRuntimeValue;
    MutRef?: {
        location: ITraceLocation;
        snapshot: any[];
    };
}

interface ITraceEffect {
    Push?: ITracePushEffect;
    Pop?: ITracePopEffect;
    Write?: ITraceWriteEffect;
    Read?: ITraceReadEffect;
}

interface ITraceCloseFrame {
    frame_id: number;
    gas_left: number;
    return_: ITraceRuntimeValue[];
}

interface ITraceEvent {
    OpenFrame?: ITraceOpenFrame;
    Instruction?: ITraceInstruction;
    Effect?: ITraceEffect;
    CloseFrame?: ITraceCloseFrame;
}

interface ITraceRootObject {
    events: ITraceEvent[];
    version: number;
}

// Runtime data types.

/**
 * Kind of a trace event.
 */
export enum TraceEventKind {
    OpenFrame = 'OpenFrame',
    CloseFrame = 'CloseFrame',
    Instruction = 'Instruction',
    Effect = 'Effect'
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
    | { type: TraceEventKind.Instruction, pc: number }
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
    | { type: TraceValKind.Runtime, value: string };

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
    const traceJSON: ITraceRootObject = JSON.parse(fs.readFileSync(traceFilePath, 'utf8'));
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
                localsTypes.push(type.type_);
            }
            // process parameters - store their values in trace and set their
            // initial lifetimes
            const paramValues = [];
            const lifetimeEnds = localLifetimeEnds.get(frame.frame_id) || [];
            for (let i = 0; i < frame.parameters.length; i++) {
                const value = frame.parameters[i];
                if (value) {
                    const runtimeValue: TraceValue =
                        { type: TraceValKind.Runtime, value: JSON.stringify(value.RuntimeValue.value) };
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
            events.push({
                type: TraceEventKind.Instruction,
                pc: event.Instruction.pc
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
                const frameId = location.Local[0];
                const localIndex = location.Local[1];
                const lifetimeEnds = localLifetimeEnds.get(frameId) || [];
                lifetimeEnds[localIndex] = FRAME_LIFETIME;
                localLifetimeEnds.set(frameId, lifetimeEnds);

                if (effect.Write) {
                    const value = JSON.stringify(effect.Write.root_value_after_write.RuntimeValue.value);
                    const traceValue: TraceValue = {
                        type: TraceValKind.Runtime,
                        value
                    };
                    const TraceLocation: TraceLocation = {
                        type: TraceLocKind.Local,
                        frameId,
                        localIndex
                    };
                    events.push({
                        type: TraceEventKind.Effect,
                        effect: {
                            type: TraceEffectKind.Write,
                            location: TraceLocation,
                            value: traceValue
                        }
                    });
                }
            }


        }
    }
    return { events, localLifetimeEnds };
}
