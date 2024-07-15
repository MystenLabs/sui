// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '.';
import { AstPath, Doc, ParserOptions, doc } from 'prettier';
import { printFn } from './printer';

const { hardline, indent, join, softline, dedent, line, group, breakParent, ifBreak } =
	doc.builders;

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
	if (!path.node.enableLeadingComment) return [];
	return [
		join(
			hardline,
			comments.map((c) => (c.type == 'line_comment' ? [c.text, breakParent] : [c.text])),
		),
		hardline,
	];
}

/**
 * Prints the trailing comments of the node. Currently, we only allow a single line
 * comment to be printed. This function is injected into the `printFn` to print
 * comments after the node. See the `print` function in `printer.ts` for more information.
 *
 * @param path
 * @returns
 */
export function printTrailingComment(path: AstPath<Node>, shouldBreak: boolean = false): Doc {
	// we do not allow comments on empty lines
	if (path.node.isEmptyLine) return '';
	if (!path.node.enableTrailingComment) return '';
	const comment = path.node.trailingComment;
	if (!comment) return '';

	if (comment.type == 'line_comment') {
		return [' ', comment.text, shouldBreak ? breakParent : ''];
	}

	return [' ', comment.text];
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

	if (length == 0) {
		return '{}';
	}

	return group(
		[
			'{',
			options.bracketSpacing ? ifBreak('', ' ') : '',
			indent(softline),
			indent(
				join(softline, path.map(print, 'namedAndEmptyLineChildren').slice(skipChildren)),
			),
			softline,
			options.bracketSpacing ? ifBreak('', ' ') : '',
			'}',
		],
		{ shouldBreak },
	);
}

export type ListOptions = {
	path: AstPath<Node>;
	print: printFn;
	options: ParserOptions;
	/** Opening bracket. */
	open: string;
	/** Closing bracket. */
	close: string;
	/**
	 * The number of children to skip when printing the list.
	 */
	skipChildren?: number;
	/**
	 * Whether to add a whitespace after the open bracket and before the close bracket.
	 * ```
	 * { a, b, c } // addWhitespace = true
	 * {a, b, c}   // addWhitespace = false
	 * ```
	 */
	addWhitespace?: boolean;
};

/**
 * Prints a list of non-formatting children. Handles commas and trailing comments.
 *
 * @param param0
 * @returns
 */
export function list({
	path,
	print,
	options,
	open,
	close,
	addWhitespace = false,
	skipChildren = 0,
}: ListOptions) {
	const length = path.node.nonFormattingChildren.length;

	if (length == skipChildren) {
		return `${open}${close}`;
	}

	return [
		open,
		indent(addWhitespace ? line : softline),
		indent(
			path
				.map((path, i) => {
					const comment = printTrailingComment(path, true);
					path.node.disableTrailingComment();

					return i < length - 1
						? [print(path), ',', comment, line]
						: [
								print(path),
								ifBreak(','),
								comment,
								dedent(addWhitespace ? line : softline),
							];
				}, 'nonFormattingChildren')
				.slice(skipChildren),
		),
		close,
	];
}
