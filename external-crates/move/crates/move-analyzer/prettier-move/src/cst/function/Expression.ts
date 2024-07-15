// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '../..';
import { MoveOptions, printFn, treeFn } from '../../printer';
import { AstPath, Doc, ParserOptions, doc } from 'prettier';
import { block, list, printLeadingComment, shouldBreakFirstChild } from '../../utilities';
const { group, join, line, softline, ifBreak, indent } = doc.builders;

// + sign marks nodes that have tests
/**
 * Marked as "_expression" group in the grammar.
 */
export enum Expression {
	LambdaExpression = 'lambda_expression',
	AssignExpression = 'assign_expression', // +
	BinaryExpression = 'binary_expression', // +
	IdentifiedExpression = 'identified_expression', // +
	MatchExpression = 'match_expression',
	BreakExpression = 'break_expression', // +
	ContinueExpression = 'continue_expression', // +
	NameExpression = 'name_expression', // +
	CallExpression = 'call_expression', // +
	MacroCallExpression = 'macro_call_expression',
	PackExpression = 'pack_expression', // +
	UnitExpression = 'unit_expression', // +
	ExpressionList = 'expression_list', // +
	AnnotateExpression = 'annotate_expression', // +
	CastExpression = 'cast_expression', // +
	Block = 'block', // +
	DotExpression = 'dot_expression', // +
	IndexExpression = 'index_expression', // +
	VectorExpression = 'vector_expression', // +

	// Misc
	ArgList = 'arg_list', // + trasitively via `call_expression`
	MacroModuleAccess = 'macro_module_access',

	// Unary
	UnaryExpression = 'unary_expression', // +
	BorrowExpression = 'borrow_expression', // +
	DereferenceExpression = 'dereference_expression', // +
	MoveOrCopyExpression = 'move_or_copy_expression', // +
}

export default function (path: AstPath<Node>): treeFn | null {
	switch (path.node.type) {
		case Expression.LambdaExpression:
			return printLambdaExpression;
		case Expression.AssignExpression:
			return printAssignExpression;
		case Expression.UnaryExpression:
			return printUnaryExpression;
		case Expression.BinaryExpression:
			return printBinaryExpression;

		case Expression.BreakExpression:
			return printBreakExpression;
		case Expression.ContinueExpression:
			return printContinueExpression;
		case Expression.NameExpression:
			return printNameExpression;
		case Expression.CallExpression:
			return printCallExpression;
		case Expression.MacroCallExpression:
			return printMacroCallExpression;
		case Expression.PackExpression:
			return printPackExpression;
		case Expression.UnitExpression:
			return printUnitExpression;
		case Expression.ExpressionList:
			return printExpressionList;
		case Expression.AnnotateExpression:
			return printAnnotateExpression;
		case Expression.CastExpression:
			return printCastExpression;
		case Expression.Block:
			return printBlock;
		case Expression.DotExpression:
			return printDotExpression;
		case Expression.IndexExpression:
			return printIndexExpression;
		case Expression.VectorExpression:
			return printVectorExpression;

		// === Misc ===

		case Expression.ArgList:
			return printArgList;
		case Expression.MacroModuleAccess:
			return printMacroModuleAccess;
		case Expression.IdentifiedExpression:
			return printIdentifiedExpression;

		// === Unary ===

		case Expression.BorrowExpression:
			return printBorrowExpression;
		case Expression.DereferenceExpression:
			return printDereferenceExpression;
		case Expression.MoveOrCopyExpression:
			return printMoveOrCopyExpression;
	}
	return null;
}

/**
 * Print `labda_expression` node.
 * Inside:
 * - `lambda_bindings`
 * - `_bind`
 */
function printLambdaExpression(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	const children = path.map(print, 'nonFormattingChildren');

	if (children.length === 1) {
		return children[0]!;
	}

	if (children.length === 2) {
		return join(' ', children);
	}

	// bindings, return type, expression
	if (children.length === 3) {
		return [
			children[0]!, // bindings
			' -> ',
			children[1]!, // return type
			' ',
			children[2]!, // expression
		];
	}

	throw new Error('`lambda_expression` node should have 1, 2 or 3 children');
}

/**
 * Print `assign_expression` node.
 */
function printAssignExpression(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	return [
		path.call(print, 'nonFormattingChildren', 0), // lhs
		' =',
		group([
			indent(line),
			indent(path.call(print, 'nonFormattingChildren', 1)), // rhs
		]),
	];
}

/**
 * Print `unary_expression` node.
 */
function printUnaryExpression(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	return [
		path.call(print, 'nonFormattingChildren', 0),
		path.call(print, 'nonFormattingChildren', 1),
	];
}

/**
 * Print `binary_expression` node.
 */
function printBinaryExpression(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	const rhs = path.node.nonFormattingChildren[2];
	const shouldBreak = rhs!.startsOnNewLine || false;
	const breakSymbol: Doc = rhs?.isBreakableExpression ? ' ' : line;

	return group(
		[
			path.call(print, 'nonFormattingChildren', 0), // lhs
			' ',
			path.call(print, 'nonFormattingChildren', 1), // operator
			breakSymbol,
			path.call(print, 'nonFormattingChildren', 2), // rhs
		],
		{ shouldBreak },
	);
}

/**
 * Print `break_expression` node.
 */
export function printBreakExpression(
	path: AstPath<Node>,
	options: ParserOptions,
	print: printFn,
): Doc {
	if (path.node.nonFormattingChildren.length > 0) {
		return join(' ', ['break', ...path.map(print, 'nonFormattingChildren')]);
	}

	return 'break';
}

/**
 * Print `continue_expression` node.
 */
function printContinueExpression(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	if (path.node.nonFormattingChildren.length === 1) {
		return ['continue ', path.call(print, 'nonFormattingChildren', 0)];
	}

	return 'continue';
}

/**
 * Print `name_expression` node.
 * Inside:
 * - `module_access`
 * - `type_arguments`
 */
function printNameExpression(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	return path.map(print, 'nonFormattingChildren');
}

/**
 * Print `call_expression` node.
 * Inside:
 * - `module_access`
 * - `type_arguments`
 * - `arg_list`
 */
function printCallExpression(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	return path.map(print, 'nonFormattingChildren');
}

/**
 * Print `macro_call_expression` node.
 * Inside:
 * - `macro_module_access`
 * - `type_arguments`
 * - `arg_list`
 */
function printMacroCallExpression(
	path: AstPath<Node>,
	options: ParserOptions,
	print: printFn,
): Doc {
	return path.map(print, 'nonFormattingChildren');
}

/**
 * Print `pack_expression` node.
 * Inside:
 * - `module_access`
 * - `type_arguments` (optional)
 * - `field_initialize_list`
 */
function printPackExpression(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	return path.map(print, 'nonFormattingChildren');
}

/**
 * Print `unit_expression` node.
 */
function printUnitExpression(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	return '()';
}

/**
 * Print `expression_list` node.
 */
function printExpressionList(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	return group(
		list({ path, print, options, open: '(', close: ')' }),
		{ shouldBreak: shouldBreakFirstChild(path) },
	);
}

/**
 * Print `annotate_expression` node.
 */
function printAnnotateExpression(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	const children = path.map(print, 'nonFormattingChildren');
	if (children.length !== 2) {
		throw new Error('`annotate_expression` node should have 2 children');
	}

	return group([
		'(',
		indent(softline),
		indent(children[0]!), // expression
		': ',
		children[1]!, // type
		softline,
		')',
	]);
}

/**
 * Print `cast_expression` node.
 * Inside:
 * - `expression`
 * - `type`
 */
function printCastExpression(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	const parens = path.node.child(0)?.text == '(';
	const children = path.map(print, 'nonFormattingChildren');

	if (parens) {
		return ['(', join(' as ', children), ')'];
	}

	return join(' as ', children);
}

/**
 * Print `index_expression` node.
 */
export function printIndexExpression(
	path: AstPath<Node>,
	options: ParserOptions,
	print: printFn,
): Doc {
	return group(
		[
			path.call(print, 'nonFormattingChildren', 0), // lhs
			'[',
			indent(softline),
			indent(path.call(print, 'nonFormattingChildren', 1)), // index
			softline,
			']',
		],
		{ shouldBreak: false },
	);
}

/**
 * Print `block` node.
 * Sequence expressions with semicolons, except for the last one if
 * there is no `ret_type` for the function or the parent is an
 * `assign_expression` or `let_statement`.
 */
export function printBlock(
	path: AstPath<Node>,
	options: ParserOptions & MoveOptions,
	print: printFn,
): Doc {
	const hasNonEmptyLine = path.node.namedChildren.some(
		(child) => !child.isEmptyLine && !child.isNewline,
	);

	if (!hasNonEmptyLine) {
		return '{}';
	}

	return block({
		options,
		print,
		path,
		shouldBreak: shouldBreakFirstChild(path),
	});
}

/**
 * Print `dot_expression` node.
 *
 * Note, that it's intentional, that the return value is not a `group`. For more info,
 * @see [This Issue](https://github.com/prettier/prettier/issues/15710#issuecomment-1836701758)
 */
function printDotExpression(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	const nodes = path.node.nonFormattingChildren;
	const children = path.map(print, 'nonFormattingChildren');
	const isChain =
		nodes[0]?.type === Expression.DotExpression ||
		path.node.parent?.type === Expression.DotExpression;

	if (children.length < 2) {
		return path.node.type;
	}

	if (isChain) {
		const parts = [
			path.call(print, 'nonFormattingChildren', 0), // lhs
			indent(softline),
			indent(path.call(printWithLeadingComment, 'nonFormattingChildren', 1)), // rhs
		];

		// start a group if the parent is not a `dot_expression`, this way we either break the
		// whole chain of `dot_expression` or none of them
		return path.node.parent?.type !== Expression.DotExpression
			? group(parts, { shouldBreak: false })
			: parts;
	}

	return [
		path.call(print, 'nonFormattingChildren', 0),
		path.call(printWithLeadingComment, 'nonFormattingChildren', 1),
	];

	/** Prints leading comment before the dot. And disables it. */
	function printWithLeadingComment(path: AstPath<Node>) {
		const comment = printLeadingComment(path);
		path.node.disableLeadingComment();
		return [comment, '.', print(path)];
	}
}

/**
 * Print `arg_list` node.
 */
function printArgList(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	const nodes = path.node.nonFormattingChildren;
	const children = path.map(print, 'nonFormattingChildren');

	if (nodes.length === 1 && nodes[0]!.isBreakableExpression) {
		return ['(', children[0]!, ')'];
	}

	return group(list({ path, print, options, open: '(', close: ')' }), {
		shouldBreak: shouldBreakFirstChild(path),
	});
}

/**
 * Print `macro_module_access` node.
 */
function printMacroModuleAccess(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	return [path.call(print, 'nonFormattingChildren', 0), '!'];
}

/**
 * Print `borrow_expression` node.
 */
function printBorrowExpression(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	const ref = path.node.child(0)!.text == '&mut' ? ['&mut', ' '] : ['&'];
	return [
		...ref,
		path.call(print, 'nonFormattingChildren', 0), // borrow type
	];
}

/**
 * Print `dereference_expression` node.
 */
function printDereferenceExpression(
	path: AstPath<Node>,
	options: ParserOptions,
	print: printFn,
): Doc {
	return ['*', path.call(print, 'nonFormattingChildren', 0)];
}

/**
 * Print `move_or_copy_expression` node.
 */
function printMoveOrCopyExpression(
	path: AstPath<Node>,
	options: ParserOptions,
	print: printFn,
): Doc {
	const ref = path.node.child(0)!.text == 'move' ? ['move', ' '] : ['copy', ' '];
	return [
		...ref,
		path.call(print, 'nonFormattingChildren', 0), // expression
	];
}

/**
 * Print `identified_expression` node.
 * Also known as `label` in the grammar or `labeled_expression`.
 */
function printIdentifiedExpression(
	path: AstPath<Node>,
	options: ParserOptions,
	print: printFn,
): Doc {
	return [
		path.call(print, 'nonFormattingChildren', 0), // identifier
		' ',
		path.call(print, 'nonFormattingChildren', 1), // expression
	];
}

/**
 * Print `vector_expression` node.
 */
function printVectorExpression(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	if (path.node.namedChildCount === 0) {
		return 'vector[]';
	}

	// Vector without type specified
	// Eg: `vector[....]`
	if (path.node.child(0)?.text == 'vector[') {
		return group(['vector', list({ path, print, options, open: '[', close: ']' })], {
			shouldBreak: shouldBreakFirstChild(path),
		});
	}

	// Vector with type
	// Eg: `vector<TYPE>[...]`
	return group(
		[
			'vector<',
			// do not break the type in vector literal
			// indent(softline),
			path.call(print, 'nonFormattingChildren', 0),
			'>',
			list({ path, print, options, open: '[', close: ']', skipChildren: 1 }),
		],
		{ shouldBreak: shouldBreakFirstChild(path) },
	);
}
