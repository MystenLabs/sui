// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '../..';
import { MoveOptions, printFn, treeFn } from '../../printer';
import { AstPath, Doc, ParserOptions, doc } from 'prettier';
import { FunctionDefinition } from '../function/FunctionDefinition';
const { join, hardline, indent } = doc.builders;

/**
 * Creates a callback function to print modules and module-related nodes.
 */
export default function (path: AstPath<Node>): treeFn | null {
	switch (path.node.type) {
		case Module.ModuleDefinition:
			return printModuleDefinition;
		case Module.ModuleIdentity:
			return printModuleIdentity;
		case Module.ModuleIdentifier:
			return printModuleIdentifier;
		case Module.ModuleBody:
			return printModuleBody;
		default:
			return null;
	}
}

/**
 * Module - top-level definition in a Move source file.
 */
export enum Module {
	ModuleDefinition = 'module_definition',
	BlockComment = 'block_comment',
	ModuleIdentity = 'module_identity',
	ModuleIdentifier = 'module_identifier',
	ModuleBody = 'module_body',
}

/**
 * Print `module_definition` node.
 */
export function printModuleDefinition(
	path: AstPath<Node>,
	options: ParserOptions,
	print: printFn,
): Doc {
	return join(' ', ['module', ...path.map(print, 'nonFormattingChildren')]);
}

/**
 * Print `module_identifier` node.
 */
function printModuleIdentifier(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	return path.node.text;
}

/**
 * Print `module_identity` node.
 */
function printModuleIdentity(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	return join('::', path.map(print, 'nonFormattingChildren'));
}

/**
 * Print `module_body` node.
 *
 * We need to preserve spacing between members (functions, structs, constants, etc.).
 * We need to only allow a single empty line (if there are more than one, we should remove them).
 */
function printModuleBody(
	path: AstPath<Node>,
	options: ParserOptions & MoveOptions,
	print: printFn,
): Doc {
	// add empty line between members if it's not already there
	const nodes = path.node.namedAndEmptyLineChildren;
	const printed = path.map((path, i) => {
		const next = nodes[i + 1];

		if (path.node.type === 'empty_line') {
			return print(path);
		}

		if (path.node.type === 'annotation') {
			return print(path);
		}

		// add empty line between members of different types (e.g., function and struct)
		// if it's not already there
		if (next && !next?.isEmptyLine && next?.type != path.node.type) {
			return [path.call(print), hardline];
		}

		// force add empty line after function definitions
		if (next && path.node.type === FunctionDefinition.FunctionDefinition && next?.type !== 'empty_line') {
			return [path.call(print), hardline];
		}

		return path.call(print);
	}, 'namedAndEmptyLineChildren');

	return ['{', indent(hardline), indent(join(hardline, printed)), hardline, '}'];
}
