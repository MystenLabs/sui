// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

/**
 * Contains the Prettier Plugin definition for the Move language.
 * For more information on Prettier plugins, see https://prettier.io/docs/en/plugins.html
 *
 * The printing logic is implemented in the `printer` module, which is routing the
 * specific node types defined in the `cst/*` modules.
 *
 * Additionally, `utilities` module contains helper functions for the printer.
 *
 * @module prettier-move
 */

import * as path from 'path';
import Parser = require('web-tree-sitter');
import { print } from './printer';
import { Tree } from './tree';
import {
    Parser as PrettierParser,
    Printer,
    Plugin,
    SupportOption,
    SupportLanguage,
} from 'prettier';

/**
 * Alias for easier refactoring if the SyntaxNode is changed.
 */
export type Node = Tree;

export const languages: SupportLanguage[] = [
    {
        name: 'move',
        extensions: ['.move'],
        parsers: ['move'],
    },
];

export const parsers: { [key: string]: PrettierParser } = {
    move: {
        parse: (text: string): Promise<Node> => {
            return (async (): Promise<Node> => {
                await Parser.init();
                const parser = new Parser();
                const Lang = await Parser.Language.load(
                    path.join(__dirname, '..', 'tree-sitter-move.wasm'),
                );
                parser.setLanguage(Lang);
                return new Tree(parser.parse(text).rootNode);
            })();
        },

        astFormat: 'move',
        locStart: () => -1,
        locEnd: () => -1,
    },
};

export const printers: { [key: string]: Printer } = {
    move: { print },
};

export const options: Record<string, SupportOption> = {
    autoGroupImports: {
        type: 'choice',
        category: 'Global',
        default: 'package',
        description: "Group all use imports by 'package', 'module' or 'none'.",
        choices: [
            {
                value: 'package',
                description:
                    'Group imports by package, eg `use sui::{balance::Balance, coin::Coin}',
            },
            {
                value: 'module',
                description:
                    'Group imports by module eg\n`use sui::balance::Balance;\nuse sui::coin::Coin`',
            },
        ],
    },
    wrapComments: {
        type: 'boolean',
        category: 'Global',
        default: false,
        description: 'Wrap comments to the next line if the line is too long.',
    },
    useModuleLabel: {
        type: 'boolean',
        category: 'Global',
        default: true,
        description:
            'Enable module labels instead of module with braces. This option will be ignored if there is more than one module in the file.',
    },
    enableErrorDebug: {
        type: 'boolean',
        category: 'Global',
        default: false,
        description: 'Print ERROR nodes instead of throwing an error.',
    },
};

export const defaultOptions = {
    tabWidth: 4,
    useTabs: false,
    printWidth: 100,
    useModuleLabel: false,
    groupImports: 'module',
};

export default {
    languages,
    parsers,
    printers,
    options,
    defaultOptions,
} as Plugin;
