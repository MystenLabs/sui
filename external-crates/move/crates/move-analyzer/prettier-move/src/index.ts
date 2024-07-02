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
import { SyntaxNode } from 'web-tree-sitter';
import { print } from './printer';
import { FormattedNode, preprocess } from './preprocess';
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
export type Node = FormattedNode;

export const languages: SupportLanguage[] = [
	{
		name: 'move',
		extensions: ['.move'],
		parsers: ['move-parse'],
	},
];

export const parsers: { [key: string]: PrettierParser } = {
	'move-parse': {
		parse: (text: string): Promise<SyntaxNode> => {
			return (async (): Promise<SyntaxNode> => {
				await Parser.init();
				const parser = new Parser();
				const Lang = await Parser.Language.load(
					path.join(__dirname, '..', 'tree-sitter-move.wasm'),
				);
				parser.setLanguage(Lang);
				return parser.parse(text).rootNode;
			})();
		},

		astFormat: 'move-format',
		locStart: () => -1,
		locEnd: () => -1,
	},
};

export const printers: { [key: string]: Printer } = {
	'move-format': {
		print,
		preprocess,
	},
};

export const options: Record<string, SupportOption> = {
	wrapComments: {
		type: 'boolean',
		category: 'Global',
		default: true,
		description: 'Wrap comments to the next line if the line is too long.',
	},
	alwaysBreakFunctions: {
		type: 'boolean',
		category: 'Global',
		default: true,
		description: 'Always break function body into multiple lines.',
	},
	alwaysBreakConditionals: {
		type: 'boolean',
		category: 'Global',
		default: true,
		description: 'Always break conditional body into multiple lines.',
	},
};

export const defaultOptions = {
	tabWidth: 4,
	useTabs: false,
	printWidth: 100,
};

export default {
	languages,
	parsers,
	printers,
	options,
	defaultOptions,
} as Plugin;
