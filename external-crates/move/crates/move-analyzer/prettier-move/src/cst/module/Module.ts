// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '../..';
import { MoveOptions, printFn, treeFn } from '../../printer';
import { AstPath, Doc, ParserOptions, doc } from 'prettier';
import { FunctionDefinition } from '../function/FunctionDefinition';
import { StructDefinition } from '../struct/StructDefinition';
import { Constant } from '../constant/Constant';
import { UseDeclaration } from '../use/UseDeclaration';
import { EnumDefinition } from '../EnumDefinition';
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
 * Members that must be separated by an empty line if they are next to each other.
 * For example, a function definition followed by a struct definition.
 */
const separatedMembers = [
	FunctionDefinition.FunctionDefinition,
	StructDefinition.StructDefinition,
	Constant.Constant,
	UseDeclaration.UseDeclaration,
	UseDeclaration.FriendDeclaration,
	EnumDefinition.EnumDefinition,
	,
] as string[];

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
	const nodes = path.node.namedAndEmptyLineChildren;
	const printed = path.map((path, i) => {
		const next = nodes[i + 1];

		if (
			separatedMembers.includes(path.node.type) &&
			separatedMembers.includes(next?.type || '') &&
			path.node.type !== next?.type
		) {
			return [path.call(print), hardline];
		}

		// force add empty line after function definitions
		if (
			path.node.type === FunctionDefinition.FunctionDefinition &&
			next?.type === FunctionDefinition.FunctionDefinition
		) {
			return [path.call(print), hardline];
		}

		return path.call(print);
	}, 'namedAndEmptyLineChildren');

	return ['{', indent(hardline), indent(join(hardline, printed)), hardline, '}'];
}
