// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import * as path from 'path';

import Parser = require("web-tree-sitter");
import { SyntaxNode } from 'web-tree-sitter'

import { print } from './printer'

export const languages = [
    {
        name: 'move',
        extensions: ['.move'],
        parsers: ['move-parse'],
    },
]

export const parsers = {
    'move-parse': {
        parse: (text: string): Promise<SyntaxNode> => {
            return (async (): Promise<SyntaxNode> => {
                await Parser.init();
                const parser = new Parser();
                const Lang = await Parser.Language.load(
                    path.join(__dirname, '..', 'tree-sitter-move.wasm')
                );
                parser.setLanguage(Lang);
                return parser.parse(text).rootNode;
            })();
        },

        astFormat: 'move-format',
        locStart: () => -1,
        locEnd: () => -1,
    },
}

export const printers = {
    'move-format': {
        print
    },
}

export const defaultOptions = {
    tabWidth: 4,
}
