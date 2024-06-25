// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '../..';
import { MoveOptions, printFn, treeFn } from '../../printer';
import { AstPath, Doc, ParserOptions, doc } from 'prettier';
const { join, softline, group, indent, line, ifBreak } = doc.builders;

export default function (path: AstPath<Node>): treeFn | null {
	switch (path.node.type) {
		case FunctionDefinition.NativeFunctionDefinition:
			return printNativeFunctionDefinition;
		case FunctionDefinition.FunctionDefinition:
			return printFunctionDefinition;
		case FunctionDefinition.MacroFunctionDefinition:
			return printMacroFunctionDefinition;
		case FunctionDefinition.FunctionIdentifier:
			return printFunctionIdentifier;
		case FunctionDefinition.VisibilityModifier:
			return printVisibilityModifier;
		case FunctionDefinition.FunctionParameters:
			return printFunctionParameters;
		case FunctionDefinition.FunctionParameter:
			return printFunctionParameter;
		case FunctionDefinition.ReturnType:
			return printReturnType;
		case FunctionDefinition.TypeArguments:
			return printTypeArguments;
		case FunctionDefinition.TypeParameters:
			return printTypeParameters;
		case FunctionDefinition.TypeParameter:
			return printTypeParameter;
		case FunctionDefinition.TypeParameterIdentifier:
			return printTypeParameterIdentifier;
	}

	return null;
}

/**
 * Function Definition, contains the following:
 * ```
 * <visibility> fun <identifier> (<parameters>) <return_type> <body>
 * ```
 */
export enum FunctionDefinition {
	FunctionDefinition = 'function_definition',
	FunctionIdentifier = 'function_identifier',
	NativeFunctionDefinition = 'native_function_definition',
	MacroFunctionDefinition = 'macro_function_definition',
	/**
	 * - `public`, `public(package)`
	 * - TODO: `native`
	 */
	VisibilityModifier = 'visibility_modifier',
	FunctionParameters = 'function_parameters',
	FunctionParameter = 'function_parameter',
	Block = 'block',
	ReturnType = 'ret_type',
	TypeArguments = 'type_arguments',
	TypeParameters = 'type_parameters',
	TypeParameter = 'type_parameter',
	TypeParameterIdentifier = 'type_parameter_identifier',
}

export type Modifiers = {
	native?: boolean;
	public?: boolean;
	entry?: boolean;
	['public(package)']?: boolean;
};

/**
 * Print `function_definition` node.
 */
export function printFunctionDefinition(
	path: AstPath<Node>,
	options: ParserOptions & MoveOptions,
	print: printFn,
): Doc {
	const nodes = path.node.nonFormattingChildren;
	const retIndex = nodes.findIndex((e) => e.type == FunctionDefinition.ReturnType);
	const modifiers = getModifiers(path);

	const signature = [
		printModifiers(modifiers),
		'fun ',
		path.map((path) => {
			// We already added modifiers in the previous step
			if (path.node.type == 'modifier') return '';
			if (path.node.type == 'block') return '';
			if (path.node.type == 'ret_type') return '';
			if (path.node.isFormatting) return '';

			return print(path);
		}, 'nonFormattingChildren'),
	];

	return [
		group([signature, path.call(print, 'nonFormattingChildren', retIndex)]),
		' ',
		path.call(print, 'nonFormattingChildren', nodes.length - 1),
	];
}

export function printNativeFunctionDefinition(
	path: AstPath<Node>,
	options: ParserOptions,
	print: printFn,
): Doc {
	const modifiers = getModifiers(path);

	return [
		printModifiers(modifiers),
		'fun ',
		group(
			path.map((path) => {
				if (path.node.type == 'modifier') return '';
				return print(path);
			}, 'nonFormattingChildren'),
		),
		';',
	];
}

/**
 * Print `macro_function_definition` node.
 */
export function printMacroFunctionDefinition(
	path: AstPath<Node>,
	options: ParserOptions,
	print: printFn,
): Doc {
	const modifiers = getModifiers(path);

	return [
		printModifiers(modifiers),
		'macro fun ',
		group(
			path.map((path) => {
				if (path.node.type == 'modifier') return '';
				if (path.node.type == 'block') return '';
				return print(path);
			}, 'nonFormattingChildren'),
		),
		' ',
		path.call(print, 'nonFormattingChildren', path.node.nonFormattingChildren.length - 1),
	];
}

/**
 * Print `function_identifier` node.
 */
export function printFunctionIdentifier(
	path: AstPath<Node>,
	options: ParserOptions,
	print: printFn,
): Doc {
	return path.node.text;
}

/**
 * Print `visibility_modifier` node.
 * Always followed by a space.
 */
export function printVisibilityModifier(
	path: AstPath<Node>, //  | Node | null,
	options: ParserOptions,
	print: printFn,
): Doc {
	return [path.node.text, ' '];
}

/**
 * Print `function_parameters` node.
 */
export function printFunctionParameters(
	path: AstPath<Node>,
	options: ParserOptions,
	print: printFn,
): Doc {
	if (path.node.nonFormattingChildren.length == 0) {
		return '()';
	}

	return [
		'(',
		indent(softline),
		indent(join([',', line], path.map(print, 'nonFormattingChildren'))),
		ifBreak(','), // trailing comma
		softline,
		')',
	];
}

/**
 * Print `function_parameter` node.
 */
export function printFunctionParameter(
	path: AstPath<Node>,
	options: ParserOptions,
	print: printFn,
): Doc {
	const isMut = path.node.child(0)?.type == 'mut';
	const isDollar = path.node.children.find((c) => c.type == '$');

	return group([
		isMut ? 'mut ' : '',
		isDollar ? '$' : '',
		path.call(print, 'nonFormattingChildren', 0), // variable_identifier
		': ',
		path.call(print, 'nonFormattingChildren', 1), // type
	]);
}

/**
 * Print `ret_type` node.
 */
export function printReturnType(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	return [': ', path.call(print, 'nonFormattingChildren', 0)];
}

/**
 * Print `type_arguments` node.
 */
export function printTypeArguments(
	path: AstPath<Node>,
	options: ParserOptions,
	print: printFn,
): Doc {
	return group(
		[
			'<',
			indent(softline),
			indent(join([',', line], path.map(print, 'nonFormattingChildren'))),
			ifBreak(','), // trailing comma
			softline,
			'>',
		],
		{ shouldBreak: false },
	);
}

/**
 * Print `type_parameters` node.
 */
export function printTypeParameters(
	path: AstPath<Node>,
	options: ParserOptions,
	print: printFn,
): Doc {
	return group(
		[
			'<',
			indent(softline),
			indent(join([',', line], path.map(print, 'nonFormattingChildren'))),
			ifBreak(','), // trailing comma
			softline,
			'>',
		],
		{ shouldBreak: false },
	);
}

/**
 * Print `type_parameter` node.
 * TODO: Reimplement this function.
 */
export function printTypeParameter(
	path: AstPath<Node>,
	options: ParserOptions,
	print: printFn,
): Doc {
	const isDollar = path.node.child(0)?.type == '$';
	const isPhantom = path.node.child(0)?.type == 'phantom';
	const parameter = path.call(print, 'nonFormattingChildren', 0);
	const abilities = path.map(print, 'nonFormattingChildren').slice(1);

	return [
		isDollar ? '$' : '',
		isPhantom ? 'phantom ' : '',
		parameter,
		abilities.length > 0 ? ': ' : '',
		join(' + ', abilities),
	];
}

/**
 * Print `type_parameter_identifier` node.
 */
export function printTypeParameterIdentifier(
	path: AstPath<Node>,
	options: ParserOptions,
	print: printFn,
): Doc {
	return path.node.text;
}

/**
 * Helper function to get modifiers.
 */
function getModifiers(path: AstPath<Node>): Modifiers {
	const nodes = path.node.nonFormattingChildren;
	return nodes
		.filter((e) => e.type == 'modifier')
		.map((e) => e.text)
		.reduce((acc, e) => ({ ...acc, [e]: true }), {});
}

/**
 * Helper function to print modifiers.
 */
function printModifiers(modifiers: Modifiers): Doc {
	return [
		modifiers.public ? 'public ' : '',
		modifiers['public(package)'] ? 'public(package) ' : '',
		modifiers.entry ? 'entry ' : '',
		modifiers.native ? 'native ' : '',
	];
}
