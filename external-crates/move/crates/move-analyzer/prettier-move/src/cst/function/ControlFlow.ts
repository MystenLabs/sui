// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

/**
 * Contains rules for printing control flow expressions.
 * @module
 *
 * - IfExpression
 * - WhileExpression
 * - LoopExpression
 * - ReturnExpression
 * - AbortExpression
 */

import { Node } from '../..';
import { printFn, treeFn } from '../../printer';
import { AstPath, Doc, ParserOptions, doc } from 'prettier';
const { group, line, ifBreak, join, indentIfBreak, indent, softline } = doc.builders;

export default function (path: AstPath<Node>): treeFn | null {
	switch (path.node.type) {
		case ControlFlow.IfExpression:
			return printIfExpression;
		case ControlFlow.WhileExpression:
			return printWhileExpression;
		case ControlFlow.LoopExpression:
			return printLoopExpression;
		case ControlFlow.ReturnExpression:
			return printReturnExpression;
		case ControlFlow.AbortExpression:
			return printAbortExpression;
	}

	return null;
}

export enum ControlFlow {
	IfExpression = 'if_expression',
	WhileExpression = 'while_expression',
	LoopExpression = 'loop_expression',
	ReturnExpression = 'return_expression',
	AbortExpression = 'abort_expression',
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
function printIfExpression(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	const groupId = Symbol('if_expression');
	const length = path.node.nonFormattingChildren.length;
	const condition = path.node.nonFormattingChildren[0];
	const trueBranch = path.node.nonFormattingChildren[1];
	const result = [
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
				trueBranch?.isList // || trueBranch?.isControlFlow
					? [' ', path.call(print, 'nonFormattingChildren', 1)]
					: [
							ifBreak([' {'], ' '),
							indent(softline),
							indentIfBreak(path.call(print, 'nonFormattingChildren', 1), {
								groupId,
							}),
							ifBreak([softline, '}']),
						],
			],
			{ id: groupId },
		),
	];

	// else block
	if (length === 3) {
		// whether developer has put a newline character before `else` keyword
		// if they did - we respect it and break the line intentionally
		const shouldBreak = !!path.node.children.find((e) => e.type === 'else')?.startsOnNewLine;
		const elseBranch = path.node.nonFormattingChildren[2];
		const printed = path.call(print, 'nonFormattingChildren', 2);

		result.push(
			group(
				elseBranch?.isList || trueBranch?.type === 'block'
					? [' else ', printed]
					: [ifBreak(' ', line, { groupId }), 'else ', printed],
				{ shouldBreak },
			),
		);
	}

	return result;
}

/**
 * Print `return_expression` node.
 */
function printReturnExpression(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	const nodes = path.node.nonFormattingChildren;

	if (nodes.length === 0) {
		return 'return';
	}

	// either label or expression
	if (nodes.length === 1) {
		const expression = nodes[0]!;
		const printed = path.call(print, 'nonFormattingChildren', 0);
		return ['return ', expression.isList ? printed : indent(printed)];
	}

	// labeled expression
	if (nodes.length === 2) {
		const expression = nodes[1]!;
		const printedLabel = path.call(print, 'nonFormattingChildren', 0);
		const printedExpression = path.call(print, 'nonFormattingChildren', 1);
		return join(' ', [
			'return',
			printedLabel,
			expression.isList ? printedExpression : indent(printedExpression),
		]);
	}

	throw new Error('Invalid return expression');
}

/**
 * Print `abort_expression` node.
 */
function printAbortExpression(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	const expression = path.node.nonFormattingChildren[0];
	const printed = path.call(print, 'nonFormattingChildren', 0);

	return group([
		'abort',
		expression?.isList || expression?.isControlFlow
			? [' ', printed]
			: [indent(line), indent(printed)],
	]);
}

/**
 * Print `loop_expression` node.
 *
 * ```
 * loop <expr>
 * ```
 */
function printLoopExpression(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	return [`loop `, path.call(print, 'nonFormattingChildren', 0)];
}

/**
 * Print `while_expression` node.
 *
 * ```
 * // single line
 * while (bool_expr) expr
 *
 * // break condition
 * while (
 *	  very_long_expr &&
 *	  very_long_expr
 * ) {
 *   expr;
 * }
 * ```
 */
function printWhileExpression(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	const condition = path.node.nonFormattingChildren[0]!.isList
		? [indent(softline), path.call(print, 'nonFormattingChildren', 0), softline]
		: [indent(softline), indent(path.call(print, 'nonFormattingChildren', 0)), softline];

	return [
		group(['while (', condition, ') ']),
		path.call(print, 'nonFormattingChildren', 1), // body
	];
}
