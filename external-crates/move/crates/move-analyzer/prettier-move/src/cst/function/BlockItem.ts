// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '../..';
import { printFn, treeFn } from '../../printer';
import { AstPath, Doc, ParserOptions, doc } from 'prettier';
const { group, line, indent, ifBreak } = doc.builders;

export enum BlockItem {
	/**
	 * Top-level block item (inside functions or other blocks)
	 */
	BlockItem = 'block_item',
	/**
	 * ```
	 * let idents: <type> = expression;
	 * ```
	 */
	LetStatement = 'let_statement',
}

/**
 * Creates a callback function to print block item nodes.
 *
 * @param path
 * @returns
 */
export default function (path: AstPath<Node>): treeFn | null {
	switch (path.node.type) {
		case BlockItem.BlockItem:
			return printBlockItem;
		case BlockItem.LetStatement:
			return printLetStatement;
	}

	return null;
}

/**
 * Print `block_item` node.
 */
function printBlockItem(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	return group([path.call(print, 'nonFormattingChildren', 0), ';']);
}

/**
 * Print `let_statement` node.
 */
function printLetStatement(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	if (path.node.namedChildCount === 1) {
		return group(['let', ' ', path.call(print, 'nonFormattingChildren', 0)]);
	}

	const printed = path.map(print, 'nonFormattingChildren');

	if (printed.length === 2) {
		return [
			'let ',
			printed[0]!,
			' = ',
			printed[1]!,
		];
	}

	return [
		'let ',
		printed[0]!,
		': ',
		printed[1]!,
		' = ',
		printed[2]!,
	];
}
