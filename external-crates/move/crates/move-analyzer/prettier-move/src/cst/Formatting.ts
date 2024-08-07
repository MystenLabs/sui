// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '..';
import { printFn, treeFn } from '../printer';
import { AstPath, Doc, ParserOptions, doc } from 'prettier';

/**
 * Creates a callback function to print commments and comment-related nodes.
 *
 * @param path
 * @returns
 */
export default function (path: AstPath<Node>): treeFn | null {
	switch (path.node.type) {
		case Formatting.LineComment:
			return printLineComment;
		case Formatting.BlockComment:
			return printBlockComment;
		case Formatting.EmptyLine:
			return printEmptyLine;
		case Formatting.Newline:
			return printNewline;
		default:
			return null;
	}
}

export enum Formatting {
	LineComment = 'line_comment',
	BlockComment = 'block_comment',
	EmptyLine = 'empty_line',
	/**
	 * Special node to insert a newline before the next node.
	 * We use it to make a call to hardline or not.
	 */
	Newline = 'newline',
}

export function startsOnNewLine(path: AstPath<Node>): boolean {
	return path.previous?.type == Formatting.EmptyLine;
}

export function shouldNewLine(path: AstPath<Node>): boolean {
	return path.node.nextNamedSibling?.type == Formatting.Newline;
}

/**
 * Test if a node is a formatting node.
 *
 * @param node
 * @returns
 */
export function isFormatting(node: Node): boolean {
	return [
		Formatting.LineComment,
		Formatting.BlockComment,
		Formatting.EmptyLine,
		Formatting.Newline,
	].includes(node.type as Formatting);
}

export function isComment(node: Node | null): boolean {
	return [Formatting.LineComment, Formatting.BlockComment].includes(node?.type as Formatting);
}

export function isEmptyLine(node: Node | null): boolean {
	return Formatting.EmptyLine == node?.type;
}

export function isNewline(node: Node | null): boolean {
	return Formatting.Newline == node?.type;
}

/**
 * Print `line_comment` node.
 * Comments are handled via the `addLeadingComments` function.
 */
export function printLineComment(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	return path.node.text;
}

/**
 * Print `block_comment` node.
 */
export function printBlockComment(
	path: AstPath<Node>,
	options: ParserOptions,
	print: printFn,
): Doc {
	return [
		// startsOnNewLine(path) ? hardline : '',
		path.node.text,
		// shouldNewLine(path) ? hardline : '',
	];
}

export function printEmptyLine(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	return ''; // should not be printed directly, used in `join(hardline)` to act as an extra newline
}

export function printNewline(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	return ''; // should not be printed, ever
}
