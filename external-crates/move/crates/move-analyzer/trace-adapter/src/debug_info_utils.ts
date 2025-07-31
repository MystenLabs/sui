// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import * as crypto from 'crypto';
import * as fs from 'fs';
import * as path from 'path';
import { ModuleInfo } from './utils';
import { JSON_FILE_EXT } from './utils';

// Data types corresponding to debug info file JSON schema.

interface JSONSrcDefinitionLocation {
    file_hash: number[];
    start: number;
    end: number;
}

interface JSONSrcStructSourceMapEntry {
    definition_location: JSONSrcDefinitionLocation;
    type_parameters: [string, JSONSrcDefinitionLocation][];
    fields: JSONSrcDefinitionLocation[];
}

interface JSONSrcEnumSourceMapEntry {
    definition_location: JSONSrcDefinitionLocation;
    type_parameters: [string, JSONSrcDefinitionLocation][];
    variants: [[string, JSONSrcDefinitionLocation], JSONSrcDefinitionLocation[]][];
}

interface JSONSrcFunctionMapEntry {
    location: JSONSrcDefinitionLocation;
    definition_location: JSONSrcDefinitionLocation;
    type_parameters: [string, JSONSrcDefinitionLocation][];
    parameters: [string, JSONSrcDefinitionLocation][];
    locals: [string, JSONSrcDefinitionLocation][];
    nops: Record<string, any>;
    code_map: Record<string, JSONSrcDefinitionLocation>;
    is_native: boolean;
}

interface JSONSrcRootObject {
    version?: number; // introduced in debug info v1
    from_file_path?: string; // introduced in debug info v2
    definition_location: JSONSrcDefinitionLocation;
    module_name: string[];
    struct_map: Record<string, JSONSrcStructSourceMapEntry>;
    enum_map: Record<string, JSONSrcEnumSourceMapEntry>;
    function_map: Record<string, JSONSrcFunctionMapEntry>;
    constant_map: Record<string, string>;
}

// Runtime data types.

/**
 * Describes a location in terms of line/column.
 */
export interface ILoc {
    line: number;
    column: number;
}

/**
 * Describes a location in the source file.
 */
export interface IFileLoc {
    fileHash: string;
    loc: ILoc;
}

/**
 * Describes a local variable (or a parameter).
 */
export interface ILocalInfo {
    /**
     * Name as seen in the source code.
     */
    name: string;
    /**
     * Name as seen in the bytecode (internal compiler name).
     */
    internalName: string;
}

/**
 * Describes a function in debug info.
 */
export interface IDebugInfoFunction {
    /**
     * Locations indexed with PC values.
     */
    pcLocs: IFileLoc[],
    /**
     * Local variables info by their index in the frame
     * (parameters first, then actual locals).
     */
    localsInfo: ILocalInfo[],
    /**
     * Location of function definition start.
     */
    startLoc: ILoc,
    /**
     * Location of function definition start.
     */
    endLoc: ILoc
}

/**
 * Information about a Move source file.
 */
export interface IFileInfo {
    // File path.
    path: string;
    // File content as bytes.
    content: Uint8Array;
    // File content split into lines (for efficient line/column calculations).
    lines: string[];
}

/**
 * Debug info for a Move module, including source map
 * and optionally additional debugging information,
 * depending on debug info version.
 */
export interface IDebugInfo {
    filePath: string
    fileHash: string
    modInfo: ModuleInfo,
    functions: Map<string, IDebugInfoFunction>,
    /**
     * Lines that are not present in debug info's source map portion.
     */
    optimizedLines: number[]
}

/**
 * Reads all debug infos from the given directory. If `mustHaveSourceFile` flag
 * is true, only debug infos whose respective source files are present in the filesMap
 * are included in the result.
 * @param directory directory containing debug info files.
 * @param filesMap map from file hash to file information.
 * @param mustHaveSourceFile indicates whether resulting debug infos must have their
 * respective source files present in the filesMap.
 * @returns map from stringified module info to debug info.
 */
export function readAllDebugInfos(
    directory: string,
    filesMap: Map<string, IFileInfo>,
    mustHaveSourceFile: boolean,
): Map<string, IDebugInfo> {
    const debugInfosMap = new Map<string, IDebugInfo>();
    const allDebugInfoLinesMap = new Map<string, Set<number>>;

    const processDirectory = (dir: string) => {
        const files = fs.readdirSync(dir);
        for (const f of files) {
            const filePath = path.join(dir, f);
            const stats = fs.statSync(filePath);
            if (stats.isDirectory()) {
                processDirectory(filePath);
            } else if (path.extname(f) === JSON_FILE_EXT) {
                const debugInfo =
                    readDebugInfo(filePath, filesMap, allDebugInfoLinesMap, mustHaveSourceFile);
                if (debugInfo) {
                    debugInfosMap.set(JSON.stringify(debugInfo.modInfo), debugInfo);
                }
            }
        }
    };

    processDirectory(directory);

    for (const debugInfo of debugInfosMap.values()) {
        const fileHash = debugInfo.fileHash;
        const debugInfoLines = allDebugInfoLinesMap.get(fileHash);
        const fileInfo = filesMap.get(fileHash);
        if (debugInfoLines && fileInfo) {
            for (let i = 0; i < fileInfo.lines.length; i++) {
                if (!debugInfoLines.has(i + 1)) { // allDebugInfoLines is 1-based
                    debugInfo.optimizedLines.push(i); // result must be 0-based
                }
            }
        }
    }


    return debugInfosMap;
}

/**
 * Reads debug info from a JSON file. If `failOnNoSourceFile` is true,
 * the function throws an error if the source file is not present in the filesMap.
 *
 * @param debugInfoPath path to the debug info JSON file.
 * @param filesMap map from file hash to file information.
 * @param debugInfoLinesMap map from file hash to set of lines present
 * in all debug infos for a given file (a given debug info may contain
 * source lines for different files due to inlining).
 * @param failOnNoSourceFile indicates if debug info retrieval should fail if the
 * source file is not present in the filesMap or if it should return `undefined`.
 *
 * @returns debug info or `undefined` if `failOnNoSourceFile` is true and the source file
 * is not present in the filesMap.
 * @throws Error if with a descriptive error message if the source map cannot be read.
 */
function readDebugInfo(
    debugInfoPath: string,
    filesMap: Map<string, IFileInfo>,
    debugInfoLinesMap: Map<string, Set<number>>,
    failOnNoSourceFile: boolean,
): IDebugInfo | undefined {
    const debugInfoJSON: JSONSrcRootObject = JSON.parse(fs.readFileSync(debugInfoPath, 'utf8'));

    let fileHash = Buffer.from(debugInfoJSON.definition_location.file_hash).toString('base64');
    let fileInfo = filesMap.get(fileHash);
    if (!fileInfo) {
        if (failOnNoSourceFile) {
            throw new Error('Could not find file with hash: '
                + fileHash
                + ' when processing debug info at: '
                + debugInfoPath);
        } else {
            return undefined;
        }
    }

    /// If the actual file for which debug information was generated
    /// still exists, use it as it will likely be "buildable" (after all
    /// debug info was genrated at build time) and thus will work better
    /// in the IDE setting (e.g., with IDE's code inspection features).
    if (debugInfoJSON.from_file_path !== undefined &&
        fs.existsSync(debugInfoJSON.from_file_path)) {
        [fileHash, fileInfo] = createFileInfo(debugInfoJSON.from_file_path);
        filesMap.set(fileHash, fileInfo);
    }

    const modInfo: ModuleInfo = {
        addr: debugInfoJSON.module_name[0],
        name: debugInfoJSON.module_name[1]
    };
    const functions = new Map<string, IDebugInfoFunction>();
    const debugInfoLines = debugInfoLinesMap.get(fileHash) ?? new Set<number>;
    prePopulateDebugInfoLines(debugInfoJSON, fileInfo, debugInfoLines);
    debugInfoLinesMap.set(fileHash, debugInfoLines);
    const functionMap = debugInfoJSON.function_map;
    for (const funEntry of Object.values(functionMap)) {
        let nameStart = funEntry.definition_location.start;
        let nameEnd = funEntry.definition_location.end;
        const nameBytes = fileInfo.content.slice(nameStart, nameEnd);
        const funName = Buffer.from(nameBytes).toString('utf8');
        const pcLocs: IFileLoc[] = [];
        let prevPC = 0;
        // we need to initialize `prevFileLoc` to make the compiler happy but it's never
        // going to be used as the first PC in the frame is always 0 so the inner
        // loop never gets executed during first iteration of the outer loop
        let prevLoc: IFileLoc = {
            fileHash: "",
            loc: { line: -1, column: -1 }
        };
        // create a list of locations for each PC, even those not explicitly listed
        // in the source map
        for (const [pc, defLocation] of Object.entries(funEntry.code_map)) {
            const currentPC = parseInt(pc);
            const defLocFileHash = Buffer.from(defLocation.file_hash).toString('base64');
            const fileInfo = filesMap.get(defLocFileHash);
            if (!fileInfo) {
                throw new Error('Could not find file with hash: '
                    + fileHash
                    + ' when processing debug info at: '
                    + debugInfoPath);
            }
            const currentStartLoc = byteOffsetToLineColumn(fileInfo, defLocation.start);
            const currentFileStartLoc: IFileLoc = {
                fileHash: defLocFileHash,
                loc: currentStartLoc
            };
            const debugInfoLines = debugInfoLinesMap.get(defLocFileHash) ?? new Set<number>;
            debugInfoLines.add(currentStartLoc.line);
            // add the end line to the set as well even if we don't need it for pcLocs
            const currentEndLoc = byteOffsetToLineColumn(fileInfo, defLocation.end);
            debugInfoLines.add(currentEndLoc.line);
            debugInfoLinesMap.set(defLocFileHash, debugInfoLines);
            for (let i = prevPC + 1; i < currentPC; i++) {
                pcLocs.push(prevLoc);
            }
            pcLocs.push(currentFileStartLoc);
            prevPC = currentPC;
            prevLoc = currentFileStartLoc;
        }

        const localsNames: ILocalInfo[] = [];
        for (const param of funEntry.parameters) {
            let paramName = param[0].split("#")[0];
            if (!paramName) {
                paramName = param[0];
            }
            localsNames.push({ name: paramName, internalName: param[0] });
        }

        for (const local of funEntry.locals) {
            let localsName = local[0].split("#")[0];
            if (!localsName) {
                localsName = local[0];
            }
            localsNames.push({ name: localsName, internalName: local[0] });
        }
        // compute start and end of function definition
        const startLoc = byteOffsetToLineColumn(fileInfo, funEntry.location.start);
        const endLoc = byteOffsetToLineColumn(fileInfo, funEntry.location.end);
        functions.set(funName, { pcLocs, localsInfo: localsNames, startLoc, endLoc });
    }
    return { filePath: fileInfo.path, fileHash, modInfo, functions, optimizedLines: [] };
}


/**
 * Creates IFileInfo for a file on a given path and returns it along with
 * the file hash.
 *
 * @param filePath path to the file.
 * @returns a tuple with the file hash and the file info.
 */
export function createFileInfo(filePath: string): [string, IFileInfo] {
    const content = new Uint8Array(fs.readFileSync(filePath));
    const numFileHash = computeFileHash(content);
    const contentString = Buffer.from(content).toString('utf8');
    const lines = contentString.split('\n');
    const fileInfo = { path: filePath, content, lines };
    const fileHash = Buffer.from(numFileHash).toString('base64');
    return [fileHash, fileInfo];
}

/**
 * Computes the SHA-256 hash of a file's contents.
 *
 * @param fileContents contents of the file.
 */
function computeFileHash(fileContents: Uint8Array): Uint8Array {
    const hash = crypto.createHash('sha256').update(fileContents).digest();
    return new Uint8Array(hash);
}

/**
 * Pre-populates the set of source file lines that are present in the debug info
 * with lines corresponding to the definitions of module, structs, enums, and functions
 * (excluding location of instructions in the function body which are handled elsewhere).
 * Constants do not have location information in the source map and must be handled separately.
 *
 * @param sourceMapJSON
 * @param fileInfo
 * @param debugInfoLines
 */
function prePopulateDebugInfoLines(
    sourceMapJSON: JSONSrcRootObject,
    fileInfo: IFileInfo,
    debugInfoLines: Set<number>
): void {
    addLinesForLocation(sourceMapJSON.definition_location, fileInfo, debugInfoLines);
    const structMap = sourceMapJSON.struct_map;
    for (const structEntry of Object.values(structMap)) {
        addLinesForLocation(structEntry.definition_location, fileInfo, debugInfoLines);
        for (const typeParam of structEntry.type_parameters) {
            addLinesForLocation(typeParam[1], fileInfo, debugInfoLines);
        }
        for (const fieldDef of structEntry.fields) {
            addLinesForLocation(fieldDef, fileInfo, debugInfoLines);
        }
    }

    const enumMap = sourceMapJSON.enum_map;
    for (const enumEntry of Object.values(enumMap)) {
        addLinesForLocation(enumEntry.definition_location, fileInfo, debugInfoLines);
        for (const typeParam of enumEntry.type_parameters) {
            addLinesForLocation(typeParam[1], fileInfo, debugInfoLines);
        }
        for (const variant of enumEntry.variants) {
            addLinesForLocation(variant[0][1], fileInfo, debugInfoLines);
            for (const fieldDef of variant[1]) {
                addLinesForLocation(fieldDef, fileInfo, debugInfoLines);
            }
        }
    }

    const functionMap = sourceMapJSON.function_map;
    for (const funEntry of Object.values(functionMap)) {
        addLinesForLocation(funEntry.definition_location, fileInfo, debugInfoLines);
        for (const typeParam of funEntry.type_parameters) {
            addLinesForLocation(typeParam[1], fileInfo, debugInfoLines);
        }
        for (const param of funEntry.parameters) {
            addLinesForLocation(param[1], fileInfo, debugInfoLines);
        }
        for (const local of funEntry.locals) {
            addLinesForLocation(local[1], fileInfo, debugInfoLines);
        }
    }
}

/**
 * Adds source file lines for the given location to the set.
 *
 * @param loc  location in the source file.
 * @param fileInfo  source file information.
 * @param debugInfoLines  set of source file lines.
 */
function addLinesForLocation(
    loc: JSONSrcDefinitionLocation,
    fileInfo: IFileInfo,
    debugInfoLines: Set<number>
): void {
    const startLine = byteOffsetToLineColumn(fileInfo, loc.start).line;
    debugInfoLines.add(startLine);
    const endLine = byteOffsetToLineColumn(fileInfo, loc.end).line;
    debugInfoLines.add(endLine);
}


/**
 * Computes source file location (line/colum) from the byte offset
 * (assumes that lines and columns are 1-based).
 *
 * @param fileInfo  source file information.
 * @param offset  byte offset in the source file.
 * @returns Source file location (line/column).
 */
function byteOffsetToLineColumn(
    fileInfo: IFileInfo,
    offset: number,
): ILoc {
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
