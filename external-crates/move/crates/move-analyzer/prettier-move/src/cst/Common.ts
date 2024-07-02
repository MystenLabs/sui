// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '..';
import { printFn, treeFn } from '../printer';
import { AstPath, ParserOptions, doc, Doc } from 'prettier';
import { shouldBreakFirstChild } from '../utilities';
import { builders } from 'prettier/doc';
const { group, ifBreak, softline, indent, join, line } = doc.builders;

/**
 * Creates a callback function to print common nodes.
 */
export default function (path: AstPath<Node>): treeFn | null {
	switch (path.node.type) {
		case Common.PrimitiveType:
			return printPrimitiveType;
		case Common.VariableIdentifier:
			return printVariableIdentifier;
		case Common.ModuleAccess:
			return printModuleAccess;
		case Common.Identifier:
			return printIdentifier;

		case Common.RefType:
			return printRefType;
		case Common.FunctionType:
			return printFunctionType;
		case Common.FunctionTypeParameters:
			return printFunctionTypeParameters;
		case Common.BinaryOperator:
			return printBinaryOperator;
		case Common.FieldIdentifier:
			return printFieldIdentifier;
		case Common.Annotation:
			return printAnnotation;
		case Common.Ability:
			return printAbility;

		case Common.TupleType:
			return printTupleType;

		// === Bindings ===

		case Common.BindUnpack:
			return printBindUnpack;
		case Common.BindFields:
			return printBindFields;
		case Common.BindField:
			return printBindField;
		case Common.BindList:
			return printBindList;
		case Common.BindNamedFields:
			return printBindNamedFields;
		case Common.BindPositionalFields:
			return printBindPositionalFields;
		case Common.BindVar:
			return printBindVar;

		//

		case Common.Label:
			return printLabel;
		case Common.Alias:
			return printAlias;
		case Common.BlockIdentifier:
			return printBlockIdentifier;
		case Common.UnaryOperator:
			return printUnaryOperator;
		case Common.FieldInitializeList:
			return printFieldInitializeList;
		case Common.ExpressionField:
			return printExpressionField;
		case Common.LambdaBindings:
			return printLambdaBindings;
	}

	return null;
}

export enum Common {
	PrimitiveType = 'primitive_type',
	VariableIdentifier = 'variable_identifier',
	ModuleAccess = 'module_access',
	Identifier = 'identifier',
	RefType = 'ref_type',
	FunctionType = 'function_type',
	FunctionTypeParameters = 'function_type_parameters',
	BinaryOperator = 'binary_operator',
	FieldIdentifier = 'field_identifier',
	Annotation = 'annotation',
	BlockIdentifier = 'block_identifier',

	Ability = 'ability',
	TupleType = 'tuple_type',

	// === Bindings ===

	BindUnpack = 'bind_unpack',
	BindFields = 'bind_fields',
	BindField = 'bind_field',
	BindList = 'bind_list',
	BindNamedFields = 'bind_named_fields',
	BindPositionalFields = 'bind_positional_fields',
	BindVar = 'bind_var',
	LambdaBindings = 'lambda_bindings',

	// ===

	Label = 'label',
	Alias = 'alias',
	UnaryOperator = 'unary_op',
	FieldInitializeList = 'field_initialize_list',
	ExpressionField = 'exp_field',

}

/**
 * Print `primitive_type` node.
 */
export function printPrimitiveType(
	path: AstPath<Node>,
	options: ParserOptions,
	print: printFn,
): Doc {
	return path.node.text;
}

/**
 * Print `variable_identifier` node.
 */
export function printVariableIdentifier(
	path: AstPath<Node>,
	options: ParserOptions,
	print: printFn,
): Doc {
	return path.node.text;
}

/**
 * Print `module_access` node.
 */
export function printModuleAccess(
	path: AstPath<Node>,
	options: ParserOptions,
	print: printFn,
): Doc {
	return path.map(print, 'children');
}

/**
 * Print `identifier` node.
 */
export function printIdentifier(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	return path.node.text;
}

/**
 * Print `ref_type` node.
 */
export function printRefType(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	const ref = path.node.child(0)!.text == '&' ? ['&'] : ['&mut '];
	return group([
		...ref,
		path.call(print, 'nonFormattingChildren', 0), // type
	]);
}

/**
 * Print `binary_operator` node.
 */
export function printBinaryOperator(
	path: AstPath<Node>,
	options: ParserOptions,
	print: printFn,
): Doc {
	return path.node.text;
}

/**
 * Print `field_identifier` node.
 */
export function printFieldIdentifier(
	path: AstPath<Node>,
	options: ParserOptions,
	print: printFn,
): Doc {
	return path.node.text;
}

/**
 * Print `annotation` node.
 */
export function printAnnotation(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	return path.node.text;
}

/**
 * Print `ability` node.
 */
export function printAbility(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	return path.node.text;
}

/**
 * Print `tuple_type` node.
 */
export function printTupleType(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	return group(
		[
			'(',
			indent(softline),
			indent(join([',', line], path.map(print, 'nonFormattingChildren'))),
			ifBreak(','), // trailing comma
			softline,
			')',
		],
		{ shouldBreak: false },
	);
}

// === Bindings ===

/**
 * Print `bind_unpack` node.
 * For easier seach: `unpack_expression`.
 *
 * Inside:
 * - `bind_var`
 * - `bind_fields`
 * - `bind_fields`
 *
 * `let Struct { field1, field2 } = ...;`
 */
function printBindUnpack(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	return path.map(print, 'nonFormattingChildren');
}

/**
 * Print `bind_fields` node.
 * Switch between `bind_named_fields` and `bind_positional_fields`.
 */
function printBindFields(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	return path.call(print, 'nonFormattingChildren', 0);
}

/**
 * Print `bind_field` node.
 * TODO: there's something off in the CST with this node, come back to it.
 */
function printBindField(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	const nonFormatting = path.node.nonFormattingChildren;
	const isMut = !!path.node.children.find((c) => c.text === 'mut');
	const rename = nonFormatting.length == 2;

	if (nonFormatting.length == 1) {
		return group([
			isMut ? 'mut ' : '',
			nonFormatting[0]!.text
		]);
	}

	return group([
		nonFormatting[0]!.text,
		isMut ? ': mut ' : ': ',
		nonFormatting[1]!.text
	]);
}

/**
 * Print `bind_list` node.
 */
function printBindList(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	if (path.node.nonFormattingChildren.length == 1) {
		return join(' ', path.map(print, 'children'));
	}

	return indent(
		group(
			path.map((path) => {
				if (path.node.type === '(') return ['(', ifBreak(builders.line, '')];
				if (path.node.type === ')')
					return builders.dedent([ifBreak([',', builders.line], ''), ')']);
				if (path.node.type === ',') return [',', line];
				if (path.node.type === 'mut') return ['mut '];
				return indent(path.call(print));
			}, 'children'),
		),
	);
}

/**
 * Print `bind_named_fields` node.
 */
function printBindNamedFields(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	const children = path.map(print, 'nonFormattingChildren');

	if (children.length === 0) {
		return ' {}';
	}

	return group(
		[
			' {',
			indent(line),
			indent(join([',', line], children)),
			ifBreak(','), // trailing comma
			line,
			'}',
		],
		{ shouldBreak: shouldBreakFirstChild(path) },
	);
}

/**
 * Print `bind_positional_fields` node.
 */
function printBindPositionalFields(
	path: AstPath<Node>,
	options: ParserOptions,
	print: printFn,
): Doc {
	const children = path.map(print, 'nonFormattingChildren');

	if (children.length === 0) {
		return '()';
	}

	return group(
		[
			'(',
			indent(softline),
			indent(join([',', line], children)),
			ifBreak(','), // trailing comma
			softline,
			')',
		],
		{ shouldBreak: shouldBreakFirstChild(path) },
	);
}

/**
 * Print `bind_var` node.
 */
function printBindVar(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	return path.call(print, 'nonFormattingChildren', 0);
}

/**
 * Print `alias` node. ...as `identifier`
 */
export function printAlias(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	return ['as ', path.call(print, 'nonFormattingChildren', 0)];
}

/**
 * Print `block_identifier` node.
 */
function printBlockIdentifier(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	return path.call(print, 'nonFormattingChildren', 0);
}

/**
 * Print `label` node.
 */
function printLabel(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	if (path.node.nextSibling?.type == ':') {
		return [path.node.text, ':'];
	}

	return path.node.text;
}

/**
 * Print `unary_op` node.
 */
function printUnaryOperator(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	return path.node.text;
}

/**
 * Print `field_initialize_list` node.
 */
function printFieldInitializeList(
	path: AstPath<Node>,
	options: ParserOptions,
	print: printFn,
): Doc {
	const children = path.map(print, 'nonFormattingChildren');

	if (children.length === 0) {
		return ' {}';
	}

	return group([
		` {`,
		indent(line),
		indent(join([',', line], children)),
		ifBreak(','), // trailing comma
		line,
		`}`,
	]);
}

/**
 * Print `expression_field` node.
 * Inside:
 * - `field_identifier`
 * - `expression`
 */
function printExpressionField(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	const children = path.map(print, 'nonFormattingChildren');

	if (children.length === 1) {
		return children[0]!;
	}

	return group([children[0]!, ': ', children[1]!]);
}


/**
 * Print `lambda_bindings` node
 */
function printLambdaBindings(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	const children = path.map(print, 'nonFormattingChildren');

	if (children.length === 0) {
		return '||';
	}

	return group([
		'|',
		indent(softline),
		indent(join([',', line], children)),
		ifBreak(','), // trailing comma
		softline,
		'|',
	]);
}


/**
 * Print `function_type` node.
 * Inside:
 * - `function_type_parameters`
 * - `return_type`
 */
function printFunctionType(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	const children = path.map(print, 'nonFormattingChildren');

	if (children.length === 0) {
		return '||';
	}

	if (children.length === 1) {
		return children[0]!;
	}

	return join(' -> ', children);
}

/**
 * Print `function_type_parameters` node.
 */
function printFunctionTypeParameters(
	path: AstPath<Node>,
	options: ParserOptions,
	print: printFn,
): Doc {
	const children = path.map(print, 'nonFormattingChildren');
	if (children.length === 0) {
		return '||';
	}

	return group([
		'|',
		indent(softline),
		indent(join([',', line], children)),
		ifBreak(','), // trailing comma
		softline,
		'|',
	]);
}
