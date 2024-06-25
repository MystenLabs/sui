// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '../..';
import { MoveOptions, printFn, treeFn } from '../../printer';
import { AstPath, Doc, ParserOptions, doc } from 'prettier';
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
	const children = path.node.namedAndEmptyLineChildren;
	const firstNonEmpty = children.findIndex((e) => !e.isEmptyLine);
	const lastNonEmpty = children.reverse().findIndex((e) => !e.isEmptyLine);
	const printed = path
		.map(print, 'namedAndEmptyLineChildren')
		.slice(
			firstNonEmpty !== -1 ? firstNonEmpty : 0,
			lastNonEmpty !== 0 ? -lastNonEmpty : children.length,
		);

	return ['{', indent(hardline), indent(join(hardline, printed)), hardline, '}'];
}
