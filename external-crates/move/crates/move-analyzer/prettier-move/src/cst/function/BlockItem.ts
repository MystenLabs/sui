// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '../..';
import { printFn, treeFn } from '../../printer';
import { AstPath, Doc, ParserOptions, doc } from 'prettier';
const { group, line, indent } = doc.builders;

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

	const children = path.node.nonFormattingChildren;
	const printed = path.map(print, 'nonFormattingChildren');
	const expression = children.length === 3 ? children[2] : children[1];

	if (printed.length === 2) {
		return group([
			'let ',
			printed[0]!,
			' =',
			expression?.isBreakableExpression || expression?.isControlFlow
				? [' ', printed[1]!]
				: [indent(line), indent(printed[1]!)],
		]);
	}

	return group([
		'let ',
		printed[0]!,
		': ',
		printed[1]!,
		' =',
		expression!.isBreakableExpression || expression!.isControlFlow
			? [' ', printed[2]!]
			: [indent(line), indent(printed[2]!)],
	]);
}
