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

interface ISrcStructSourceMapEntry {
    definition_location: ISrcDefinitionLocation;
    type_parameters: [string, ISrcDefinitionLocation][];
    fields: ISrcDefinitionLocation[];
}

interface ISrcEnumSourceMapEntry {
    definition_location: ISrcDefinitionLocation;
    type_parameters: [string, ISrcDefinitionLocation][];
    variants: [[string, ISrcDefinitionLocation], ISrcDefinitionLocation[]][];
}

interface ISrcFunctionMapEntry {
    definition_location: ISrcDefinitionLocation;
    type_parameters: [string, ISrcDefinitionLocation][];
    parameters: [string, ISrcDefinitionLocation][];
    locals: [string, ISrcDefinitionLocation][];
    nops: Record<string, any>;
    code_map: Record<string, ISrcDefinitionLocation>;
    is_native: boolean;
}

interface ISrcRootObject {
    definition_location: ISrcDefinitionLocation;
    module_name: string[];
    struct_map: Record<string, ISrcStructSourceMapEntry>;
    enum_map: Record<string, ISrcEnumSourceMapEntry>;
    function_map: Record<string, ISrcFunctionMapEntry>;
    constant_map: Record<string, string>;
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
    pcLocs: ILoc[],
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
    functions: Map<string, ISourceMapFunction>,
    // Lines that are not present in the source map.
    optimized_lines: number[]
}

export function readAllSourceMaps(
    directory: string,
    filesMap: Map<string, IFileInfo>
): Map<string, ISourceMap> {
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
    const allSourceMapLines = new Set<number>();
    prePopulateSourceMapLines(sourceMapJSON, fileInfo, allSourceMapLines);
    const functionMap = sourceMapJSON.function_map;
    for (const funEntry of Object.values(functionMap)) {
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
            allSourceMapLines.add(currentLoc.line);
            // add the end line to the set as well even if we don't need it for pcLocs
            allSourceMapLines.add(byteOffsetToLineColumn(fileInfo, defLocation.end).line);
            for (let i = prevPC + 1; i < currentPC; i++) {
                pcLocs.push(prevLoc);
            }
            pcLocs.push(currentLoc);
            prevPC = currentPC;
            prevLoc = currentLoc;
        }
        functions.set(funName, { pcLocs });
    }
    let optimized_lines: number[] = [];
    for (let i = 0; i < fileInfo.lines.length; i++) {
        if (!allSourceMapLines.has(i + 1)) { // allSourceMapLines is 1-based
            optimized_lines.push(i); // result must be 0-based
        }
    }

    return { fileHash, modInfo, functions, optimized_lines };
}

/**
 * Pre-populates the set of source file lines that are present in the source map
 * with lines corresponding to the definitions of module, structs, enums, and functions
 * (excluding location of instructions in the function body which are handled elsewhere).
 * Constants do not have location information in the source map and must be handled separately.
 *
 * @param sourceMapJSON
 * @param fileInfo
 * @param sourceMapLines
 */
function prePopulateSourceMapLines(
    sourceMapJSON: ISrcRootObject,
    fileInfo: IFileInfo,
    sourceMapLines: Set<number>
): void {
    addLinesForLocation(sourceMapJSON.definition_location, fileInfo, sourceMapLines);
    const structMap = sourceMapJSON.struct_map;
    for (const structEntry of Object.values(structMap)) {
        addLinesForLocation(structEntry.definition_location, fileInfo, sourceMapLines);
        for (const typeParam of structEntry.type_parameters) {
            addLinesForLocation(typeParam[1], fileInfo, sourceMapLines);
        }
        for (const fieldDef of structEntry.fields) {
            addLinesForLocation(fieldDef, fileInfo, sourceMapLines);
        }
    }

    const enumMap = sourceMapJSON.enum_map;
    for (const enumEntry of Object.values(enumMap)) {
        addLinesForLocation(enumEntry.definition_location, fileInfo, sourceMapLines);
        for (const typeParam of enumEntry.type_parameters) {
            addLinesForLocation(typeParam[1], fileInfo, sourceMapLines);
        }
        for (const variant of enumEntry.variants) {
            addLinesForLocation(variant[0][1], fileInfo, sourceMapLines);
            for (const fieldDef of variant[1]) {
                addLinesForLocation(fieldDef, fileInfo, sourceMapLines);
            }
        }
    }

    const functionMap = sourceMapJSON.function_map;
    for (const funEntry of Object.values(functionMap)) {
        addLinesForLocation(funEntry.definition_location, fileInfo, sourceMapLines);
        for (const typeParam of funEntry.type_parameters) {
            addLinesForLocation(typeParam[1], fileInfo, sourceMapLines);
        }
        for (const param of funEntry.parameters) {
            addLinesForLocation(param[1], fileInfo, sourceMapLines);
        }
        for (const local of funEntry.locals) {
            addLinesForLocation(local[1], fileInfo, sourceMapLines);
        }
    }
}

/**
 * Adds source file lines for the given location to the set.
 *
 * @param loc  location in the source file.
 * @param fileInfo  source file information.
 * @param sourceMapLines  set of source file lines.
 */
function addLinesForLocation(
    loc: ISrcDefinitionLocation,
    fileInfo: IFileInfo,
    sourceMapLines: Set<number>
): void {
    const startLine = byteOffsetToLineColumn(fileInfo, loc.start).line;
    sourceMapLines.add(startLine);
    const endLine = byteOffsetToLineColumn(fileInfo, loc.end).line;
    sourceMapLines.add(endLine);
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
