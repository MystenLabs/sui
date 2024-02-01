"use strict";
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0
Object.defineProperty(exports, "__esModule", { value: true });
exports.defaultOptions = exports.printers = exports.parsers = exports.languages = void 0;
const path = require("path");
const Parser = require("web-tree-sitter");
const printer_1 = require("./printer");
exports.languages = [
    {
        name: 'move',
        extensions: ['.move'],
        parsers: ['move-parse'],
    },
];
exports.parsers = {
    'move-parse': {
        parse: (text) => {
            return (async () => {
                await Parser.init();
                const parser = new Parser();
                const Lang = await Parser.Language.load(path.join(__dirname, '..', 'tree-sitter-move.wasm'));
                parser.setLanguage(Lang);
                return parser.parse(text).rootNode;
            })();
        },
        astFormat: 'move-format',
        locStart: () => -1,
        locEnd: () => -1,
    },
};
exports.printers = {
    'move-format': {
        print: printer_1.print
    },
};
exports.defaultOptions = {
    tabWidth: 4,
};
//# sourceMappingURL=index.js.map