// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '../..';
import { MoveOptions, printFn, treeFn } from '../../printer';
import { AstPath, Doc, doc } from 'prettier';
const { group, softline, ifBreak, indentIfBreak, line, indent } = doc.builders;

/** The type of the node implemented in this file */
export const NODE_TYPE = 'if_expression';

export default function (path: AstPath<Node>): treeFn | null {
	if (path.node.type === NODE_TYPE) {
		return printIfExpression;
	}

	return null;
}

/**
 * Print `if_expression` node.
 *
 * ```
 * // single line
 * if (cond || cond) {}
 *
 * // multi line + block
 * if (
 *  long_cond ||
 *  long_cond
 * ) {
 *    long_expr;
 *    long_expr;
 * }
 *
 * // multi line + single line
 * if (cond)
 * 	return long_expr;
 *
 * // multi line + single line + long expr
 * if (
 * 	long_cond ||
 *  long_cond
 * )
 * 	return long_expr &&
 * 		long_expr;
 * ```
 */
function printIfExpression(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
	const groupId = Symbol('if_expression');
	const hasElse = path.node.children.some((e) => e.type == 'else');

	const condition = path.node.nonFormattingChildren[0];
	const trueBranch = path.node.nonFormattingChildren[1];
	const result: Doc[] = [
		// condition group
		group([
			'if (',
			condition?.isList
				? [indent(softline), path.call(print, 'nonFormattingChildren', 0), softline]
				: [
						indent(softline),
						indent(path.call(print, 'nonFormattingChildren', 0)),
						softline,
					],
			')',
		]),
		// body group
		group(
			[
				trueBranch?.isList
					? [' ', path.call(print, 'nonFormattingChildren', 1)]
					: [indent(line), path.call(print, 'nonFormattingChildren', 1)],
			],
			{ id: groupId },
		),
	];

	// else block
	if (hasElse) {
		const isTrueBlock = trueBranch?.type === 'block';
		const elseNode = path.node.nonFormattingChildren[2]!;
		const printed = path.call(print, 'nonFormattingChildren', 2);
		const shouldBreak =
			elseNode.leadingComment.some((e) => e.type == 'line_comment') ||
			elseNode.trailingComment?.type == 'line_comment';

		if (elseNode.isList || elseNode.isControlFlow) {
			result.push(group([
				isTrueBlock ? ' ' : line,
				'else ',
				printed,
			]))
		} else {
			result.push(
				group([
					isTrueBlock ? ' ' : line,
					'else',
					indent(line),
					indent(printed),
				], { shouldBreak }),
			);
		}
	}

	return result;
}
