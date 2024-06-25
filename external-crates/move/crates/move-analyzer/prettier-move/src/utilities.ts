// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '.';
import { AstPath, Doc, ParserOptions, doc } from 'prettier';
import { printFn } from './printer';

const { hardline, indent, join, softline, group, ifBreak } = doc.builders;

/**
 * Returns `true` if the first non-formatting child of the path starts on a new line.
 * This function is useful for respecting developer formatting if they choose to break
 * the list.
 *
 * ```move
 * // input
 * fun args(a: u8) {} // no break
 * fun args(
 *   a: u8 // first child starts on a new line
 * ) {}
 *
 * // output
 * fun args(a: u8) {} // no break
 * fun args(
 *  a: u8 // respect developer formatting
 * ) {}
 * ```
 *
 * @param path
 * @returns
 */
export function shouldBreakFirstChild(path: AstPath<Node>): boolean {
	return path.node.nonFormattingChildren[0]?.startsOnNewLine || false;
}

/**
 * Prints all comments that are leading the node. This function is injected into
 * the `printFn` to print comments before the node. See the `print` function in
 * `printer.ts` for more information.
 *
 * @param path
 * @returns
 */
export function printLeadingComment(path: AstPath<Node>): Doc[] {
	const comments = path.node.leadingComment;
	if (!comments || !comments.length) return [];
	return [join(hardline, comments), hardline];
}

/**
 * Prints the trailing comments of the node. Currently, we only allow a single line
 * comment to be printed. This function is injected into the `printFn` to print
 * comments after the node. See the `print` function in `printer.ts` for more information.
 *
 * @param path
 * @returns
 */
export function printTrailingComment(path: AstPath<Node>): Doc {
	// we do not allow comments on empty lines
	if (path.node.isEmptyLine) return '';
	const comment = path.node.trailingComment;
	if (!comment) return '';
	return [' ', comment];
}

/**
 * TODO: use this type for the `block()` function.
 */
export type BlockOptions = {
	path: AstPath<Node>;
	print: printFn;
	options: ParserOptions;
	breakDependency?: Symbol;

	lastLine?: boolean;
	lineEnding?: Doc;
	skipChildren?: number;
	shouldBreak?: boolean;
};

/**
 * @param path
 * @param node
 * @param print
 * @param lineEnding
 * @param line
 * @returns
 */
export function block({ path, print, options, shouldBreak, skipChildren }: BlockOptions) {
	const length = path.node.nonFormattingChildren.length;
	const firstNonEmpty = path.node.namedAndEmptyLineChildren.findIndex((e) => !e.isEmptyLine);
	const children = path
		.map(print, 'namedAndEmptyLineChildren')
		.slice(firstNonEmpty)
		.slice(skipChildren);

	if (length == 0) {
		return '{}';
	}

	return group(
		[
			'{',
			options.bracketSpacing ? ifBreak('', ' ') : '',
			indent(softline),
			indent(join(softline, children)),
			softline,
			options.bracketSpacing ? ifBreak('', ' ') : '',
			'}',
		],
		{ shouldBreak },
	);
}
