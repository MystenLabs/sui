// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import * as fs from 'fs';
import { ModuleInfo } from './utils';

// Data types corresponding to trace file JSON schema.

interface ITraceModule {
    address: string;
    name: string;
}

interface ITraceType {
    ref_type: string | null;
    type_: string | { vector: string };
}

interface ITraceRuntimeValue {
    value: any;
}

interface ITraceFrame {
    binary_member_index: number;
    frame_id: number;
    function_name: string;
    is_native: boolean;
    locals_types: ITraceType[];
    module: ITraceModule;
    parameters: ITraceRuntimeValue[];
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
    root_value_after_write: ITraceRuntimeValue;
}

interface ITraceReadEffect {
    location: ITraceLocation;
    moved: boolean;
    root_value_read: ITraceRuntimeValue;
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
 * Trace event types containing relevant data.
 */
export type TraceEvent =
    | { type: 'OpenFrame', id: number, name: string, modInfo: ModuleInfo }
    | { type: 'CloseFrame', id: number }
    | { type: 'Instruction', pc: number };

/**
 * Execution trace consisting of a sequence of trace events.
 */
interface ITrace {
    events: TraceEvent[];
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
    for (const event of traceJSON.events) {
        if (event.OpenFrame) {
            events.push({
                type: 'OpenFrame',
                id: event.OpenFrame.frame.frame_id,
                name: event.OpenFrame.frame.function_name,
                modInfo: {
                    addr: event.OpenFrame.frame.module.address,
                    name: event.OpenFrame.frame.module.name
                }
            });
        } else if (event.CloseFrame) {
            events.push({
                type: 'CloseFrame',
                id: event.CloseFrame.frame_id
            });
        } else if (event.Instruction) {
            events.push({
                type: 'Instruction',
                pc: event.Instruction.pc
            });
        }
    }
    return { events };
}
