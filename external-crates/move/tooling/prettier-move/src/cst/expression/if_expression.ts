// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '../..';
import { MoveOptions, printFn, treeFn } from '../../printer';
import { AstPath, Doc, doc } from 'prettier';
const { group, softline, line, ifBreak, indent, indentIfBreak, hardlineWithoutBreakParent } = doc.builders;

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
 * if (cond) {
 * 	return long_expr;
 * }
 *
 * // multi line + single line + long expr
 * if (
 * 	long_cond ||
 *  long_cond
 * ) {
 * 	return long_expr &&
 * 		long_expr;
 * }
 * ```
 */
function printIfExpression(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
	const hasElse = path.node.children.some((e) => e.type == 'else');
	const condition = path.node.nonFormattingChildren[0];
	const trueBranch = path.node.nonFormattingChildren[1];
	const groupId = Symbol('if_expression_true');
	const result: Doc[] = [];

	// condition group
	result.push(
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
	);

	// true branch group
	if (trueBranch?.isList) {
		const shouldBreak =
			trueBranch.leadingComment.some((e) => e.type == 'line_comment') ||
			trueBranch.trailingComment?.type == 'line_comment';

		result.push(group([' ', path.call(print, 'nonFormattingChildren', 1)], { shouldBreak: false }));
		hasElse && result.push(group([line, 'else'], { shouldBreak }));
	} else {
		result.push(
			group(
				[
					indent(line),
					indent(path.call(print, 'nonFormattingChildren', 1)),
				],
				{ id: groupId },
			),
		);

		// link group breaking to the true branch, add `else` either with a
		// newline or without - depends on whether true branch is converted into
		// a block
		hasElse && result.push([ifBreak([hardlineWithoutBreakParent, 'else'], [line, 'else'], { groupId })]);
	}

	// else block
	if (hasElse) {
		const elseNode = path.node.nonFormattingChildren[2]!;
		const shouldBreak =
			elseNode.leadingComment.some((e) => e.type == 'line_comment') ||
			elseNode.trailingComment?.type == 'line_comment';

		// special casing chained `if_expression` with the expectation that the
		// expression can handle breaking / nesting itself.
		if (elseNode.isList || elseNode.type == 'if_expression') {
			result.push([' ', path.call(print, 'nonFormattingChildren', 2)]);
		} else {
			result.push(
				group(
					[
						ifBreak([' {', indent(hardlineWithoutBreakParent)], ' '),
						indent(path.call(print, 'nonFormattingChildren', 2)),
						ifBreak([hardlineWithoutBreakParent, '}'], ''),
					],
					{ shouldBreak },
				),
			);
		}
	}

	return result;
}
