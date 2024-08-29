// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import * as fs from 'fs';
import * as path from 'path';
import { ModuleInfo } from './utils';

// Data types corresponding to source map file JSON schema.

interface ISrcDefinitionLocation {
    file_hash: number[];
    start: number;
    end: number;
}

interface ISrcFunctionMapEntry {
    definition_location: ISrcDefinitionLocation;
    type_parameters: any[];
    parameters: any[];
    locals: [string, ISrcDefinitionLocation][];
    nops: Record<string, any>;
    code_map: Record<string, ISrcDefinitionLocation>;
    is_native: boolean;
}

interface ISrcRootObject {
    definition_location: ISrcDefinitionLocation;
    module_name: string[];
    struct_map: Record<string, any>;
    enum_map: Record<string, any>;
    function_map: Record<string, ISrcFunctionMapEntry>;
    constant_map: Record<string, any>;
}

// Runtime data types.

/**
 * Describes a location in the source file.
 */
interface ILoc {
    line: number;
    column: number;
}

/**
 * Describes a function in the source map.
 */
interface ISourceMapFunction {
    // Locations indexed with PC values.
    pcLocs: ILoc[]
}

/**
 * Information about a Move source file.
 */
export interface IFileInfo {
    // File path.
    path: string;
    // File content.
    content: string;
    // File content split into lines (for efficient line/column calculations).
    lines: string[];
}

/**
 * Source map for a Move module.
 */
export interface ISourceMap {
    fileHash: string
    modInfo: ModuleInfo,
    functions: Map<string, ISourceMapFunction>
}

export function readAllSourceMaps(directory: string, filesMap: Map<string, IFileInfo>): Map<string, ISourceMap> {
    const sourceMapsMap = new Map<string, ISourceMap>();

    const processDirectory = (dir: string) => {
        const files = fs.readdirSync(dir);
        for (const f of files) {
            const filePath = path.join(dir, f);
            const stats = fs.statSync(filePath);
            if (stats.isDirectory()) {
                processDirectory(filePath);
            } else if (path.extname(f) === ".json") {
                const sourceMap = readSourceMap(filePath, filesMap);
                sourceMapsMap.set(JSON.stringify(sourceMap.modInfo), sourceMap);
            }
        }
    };

    processDirectory(directory);

    return sourceMapsMap;
}

/**
 * Reads a Move VM source map from a JSON file.
 *
 * @param sourceMapPath path to the source map JSON file.
 * @param filesMap map from file hash to file information.
 * @returns source map.
 */
function readSourceMap(sourceMapPath: string, filesMap: Map<string, IFileInfo>): ISourceMap {
    const sourceMapJSON: ISrcRootObject = JSON.parse(fs.readFileSync(sourceMapPath, 'utf8'));

    const fileHash = Buffer.from(sourceMapJSON.definition_location.file_hash).toString('base64');
    const modInfo: ModuleInfo = {
        addr: sourceMapJSON.module_name[0],
        name: sourceMapJSON.module_name[1]
    };
    const functions = new Map<string, ISourceMapFunction>();
    const fileInfo = filesMap.get(fileHash);
    if (!fileInfo) {
        throw new Error("Could not find file with hash: "
            + fileHash
            + " when processing source map at: "
            + sourceMapPath);
    }
    for (const funEntry of Object.values(sourceMapJSON.function_map)) {
        let nameStart = funEntry.definition_location.start;
        let nameEnd = funEntry.definition_location.end;
        const funName = fileInfo.content.slice(nameStart, nameEnd);
        const pcLocs: ILoc[] = [];
        let prevPC = 0;
        // we need to initialize `prevLoc` to make the compiler happy but it's never
        // going to be used as the first PC in the frame is always 0 so the inner
        // loop never gets executed during first iteration of the outer loopq
        let prevLoc = { line: -1, column: -1 };
        // create a list of locations for each PC, even those not explicitly listed
        // in the source map
        for (const [pc, defLocation] of Object.entries(funEntry.code_map)) {
            const currentPC = parseInt(pc);
            const currentLoc = byteOffsetToLineColumn(fileInfo, defLocation.start);
            for (let i = prevPC + 1; i < currentPC; i++) {
                pcLocs.push(prevLoc);
            }
            pcLocs.push(currentLoc);
            prevPC = currentPC;
            prevLoc = currentLoc;
        }
        functions.set(funName, { pcLocs });
    }
    return { fileHash, modInfo, functions };
}

/**
 * Computes source file location (line/colum) from the byte offset
 * (assumes that lines and columns are 1-based).
 *
 * @param fileInfo  source file information.
 * @param offset  byte offset in the source file.
 * @returns Source file location (line/column).
 */
function byteOffsetToLineColumn(fileInfo: IFileInfo, offset: number): ILoc {
    if (offset < 0) {
        return { line: 1, column: 1 };
    }
    const lines = fileInfo.lines;
    if (offset >= fileInfo.content.length) {
        return { line: lines.length, column: lines[lines.length - 1].length + 1 /* 1-based */ };
    }
    let accumulatedLength = 0;

    for (let lineNumber = 0; lineNumber < lines.length; lineNumber++) {
        const lineLength = lines[lineNumber].length + 1; // +1 for the newline character

        if (accumulatedLength + lineLength > offset) {
            return {
                line: lineNumber + 1, // 1-based
                column: offset - accumulatedLength + 1 // 1-based
            };
        }

        accumulatedLength += lineLength;
    }
    return { line: lines.length, column: lines[lines.length - 1].length + 1 /* 1-based */ };
}
