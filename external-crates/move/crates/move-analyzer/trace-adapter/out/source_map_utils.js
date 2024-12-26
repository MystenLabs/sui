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
Object.defineProperty(exports, "__esModule", { value: true });
exports.readAllSourceMaps = readAllSourceMaps;
const fs = __importStar(require("fs"));
const path = __importStar(require("path"));
function readAllSourceMaps(directory, filesMap) {
    const sourceMapsMap = new Map();
    const allSourceMapLinesMap = new Map;
    const processDirectory = (dir) => {
        const files = fs.readdirSync(dir);
        for (const f of files) {
            const filePath = path.join(dir, f);
            const stats = fs.statSync(filePath);
            if (stats.isDirectory()) {
                processDirectory(filePath);
            }
            else if (path.extname(f) === '.json') {
                const sourceMap = readSourceMap(filePath, filesMap, allSourceMapLinesMap);
                sourceMapsMap.set(JSON.stringify(sourceMap.modInfo), sourceMap);
            }
        }
    };
    processDirectory(directory);
    for (const sourceMap of sourceMapsMap.values()) {
        const fileHash = sourceMap.fileHash;
        const sourceMapLines = allSourceMapLinesMap.get(fileHash);
        const fileInfo = filesMap.get(fileHash);
        if (sourceMapLines && fileInfo) {
            for (let i = 0; i < fileInfo.lines.length; i++) {
                if (!sourceMapLines.has(i + 1)) { // allSourceMapLines is 1-based
                    sourceMap.optimizedLines.push(i); // result must be 0-based
                }
            }
        }
    }
    return sourceMapsMap;
}
/**
 * Reads a Move VM source map from a JSON file.
 *
 * @param sourceMapPath path to the source map JSON file.
 * @param filesMap map from file hash to file information.
 * @param sourceMapLinesMap map from file hash to set of lines present
 * in all source maps for a given file (a given source map may contain
 * source lines for different files due to inlining).
 * @returns source map.
 * @throws Error if with a descriptive error message if the source map cannot be read.
 */
function readSourceMap(sourceMapPath, filesMap, sourceMapLinesMap) {
    const sourceMapJSON = JSON.parse(fs.readFileSync(sourceMapPath, 'utf8'));
    const fileHash = Buffer.from(sourceMapJSON.definition_location.file_hash).toString('base64');
    const modInfo = {
        addr: sourceMapJSON.module_name[0],
        name: sourceMapJSON.module_name[1]
    };
    const functions = new Map();
    const fileInfo = filesMap.get(fileHash);
    if (!fileInfo) {
        throw new Error('Could not find file with hash: '
            + fileHash
            + ' when processing source map at: '
            + sourceMapPath);
    }
    const sourceMapLines = sourceMapLinesMap.get(fileHash) ?? new Set;
    prePopulateSourceMapLines(sourceMapJSON, fileInfo, sourceMapLines);
    sourceMapLinesMap.set(fileHash, sourceMapLines);
    const functionMap = sourceMapJSON.function_map;
    for (const funEntry of Object.values(functionMap)) {
        let nameStart = funEntry.definition_location.start;
        let nameEnd = funEntry.definition_location.end;
        const funName = fileInfo.content.slice(nameStart, nameEnd);
        const pcLocs = [];
        let prevPC = 0;
        // we need to initialize `prevFileLoc` to make the compiler happy but it's never
        // going to be used as the first PC in the frame is always 0 so the inner
        // loop never gets executed during first iteration of the outer loop
        let prevLoc = {
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
                    + ' when processing source map at: '
                    + sourceMapPath);
            }
            const currentStartLoc = byteOffsetToLineColumn(fileInfo, defLocation.start);
            const currentFileStartLoc = {
                fileHash: defLocFileHash,
                loc: currentStartLoc
            };
            const sourceMapLines = sourceMapLinesMap.get(defLocFileHash) ?? new Set;
            sourceMapLines.add(currentStartLoc.line);
            // add the end line to the set as well even if we don't need it for pcLocs
            const currentEndLoc = byteOffsetToLineColumn(fileInfo, defLocation.end);
            sourceMapLines.add(currentEndLoc.line);
            sourceMapLinesMap.set(defLocFileHash, sourceMapLines);
            for (let i = prevPC + 1; i < currentPC; i++) {
                pcLocs.push(prevLoc);
            }
            pcLocs.push(currentFileStartLoc);
            prevPC = currentPC;
            prevLoc = currentFileStartLoc;
        }
        const localsNames = [];
        for (const param of funEntry.parameters) {
            const paramName = param[0].split("#")[0];
            if (!paramName) {
                localsNames.push(param[0]);
            }
            else {
                localsNames.push(paramName);
            }
        }
        for (const local of funEntry.locals) {
            const localsName = local[0].split("#")[0];
            if (!localsName) {
                localsNames.push(local[0]);
            }
            else {
                localsNames.push(localsName);
            }
        }
        // compute start and end of function definition
        const startLoc = byteOffsetToLineColumn(fileInfo, funEntry.location.start);
        const endLoc = byteOffsetToLineColumn(fileInfo, funEntry.location.end);
        functions.set(funName, { pcLocs, localsNames, startLoc, endLoc });
    }
    return { filePath: fileInfo.path, fileHash, modInfo, functions, optimizedLines: [] };
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
function prePopulateSourceMapLines(sourceMapJSON, fileInfo, sourceMapLines) {
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
function addLinesForLocation(loc, fileInfo, sourceMapLines) {
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
function byteOffsetToLineColumn(fileInfo, offset) {
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
//# sourceMappingURL=source_map_utils.js.map