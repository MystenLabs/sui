// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '../..';
import { printFn, treeFn } from '../../printer';
import { AstPath, Doc, ParserOptions, doc } from 'prettier';
const { line, group, indent } = doc.builders;

/**
 * Prints:
 * - `constant`
 * - `constant_identifier`
 */
export default function (path: AstPath<Node>): treeFn | null {
	switch (path.node.type) {
		case Constant.Constant:
			return printConstant;
		case Constant.ConstantIdentifier:
			return printConstantIdentifier;
	}

	return null;
}

/**
 * Constant Declaration
 * Very straghtforward, just print the constant identifier, type and expression
 *
 * `const <identifier>: <type> = <expression>;`
 */
export enum Constant {
	/**
	 * Module-level definition
	 * ```
	 * const identifier: <type> = expression;
	 * ```
	 */
	Constant = 'constant',
	/**
	 * Identifier of the constant
	 */
	ConstantIdentifier = 'constant_identifier',
}

/**
 * Print `constant` node.
 * Breaks on the `=` sign and indents the expression.
 */
export function printConstant(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	const expression = path.node.nonFormattingChildren[2];
	return group([
		'const ',
		path.call(print, 'nonFormattingChildren', 0), // identifier
		': ',
		path.call(print, 'nonFormattingChildren', 1), // type
		' =',
		// if the expression is a list, we don't want to break the line, the expression
		// will break itself and indent the children.
		// alternatively, if the expression is a single node, we want to break and indent
		expression?.isList
			? [' ', path.call(print, 'nonFormattingChildren', 2)]
			: [indent(line), indent(path.call(print, 'nonFormattingChildren', 2))],
		';',
	]);
}

/**
 * Print `constant_identifier` node.
 */
export function printConstantIdentifier(
	path: AstPath<Node>,
	options: ParserOptions,
	print: printFn,
): Doc {
	return path.node.text;
}
