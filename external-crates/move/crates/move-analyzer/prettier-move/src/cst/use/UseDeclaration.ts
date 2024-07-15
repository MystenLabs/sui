// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '../..';
import { AstPath, Doc, ParserOptions, doc } from 'prettier';
import { printFn, treeFn } from '../../printer';
const { join, group, line, indent, softline, ifBreak } = doc.builders;

export default function (path: AstPath<Node>): treeFn | null {
	switch (path.node.type) {
		case UseDeclaration.UseDeclaration:
			return printUseDeclaration;
		case UseDeclaration.UseModule:
			return printUseModule;
		case UseDeclaration.UseMember:
			return printUseMember;
		case UseDeclaration.UseModuleMember:
			return printUseModuleMember;
		case UseDeclaration.UseModuleMembers:
			return printUseModuleMembers;
		case UseDeclaration.UseFun:
			return printUseFun;
		case UseDeclaration.ModuleIdentity:
			return printModuleIdentity;
		case UseDeclaration.FriendDeclaration:
			return printFriendDeclaration;
		case UseDeclaration.FriendAccess:
			return printFriendAccess;
		default:
			return null;
	}
}

/**
 * Use Declaration
 *
 * Contains one of the following:
 *
 * `use_declaration` (
 * - use `use_module` <as `alias`>;
 * - use `use_module_member` <as `use_member`>;
 * - use `use_module_members`;
 * - use `use_fun`;
 * )
 *
 * `use_member` (
 * - `identifier` <as `alias`>;
 * )
 */
export enum UseDeclaration {
	/**
	 * Module-level definition
	 * ```
	 * `<public> use ...;
	 * ```
	 */
	UseDeclaration = 'use_declaration',
	UseModule = 'use_module',
	UseMember = 'use_member',
	UseModuleMember = 'use_module_member',
	UseModuleMembers = 'use_module_members',
	ModuleIdentity = 'module_identity',
	FriendDeclaration = 'friend_declaration',
	FriendAccess = 'friend_access',
	UseFun = 'use_fun',
}

/**
 * Print @see `UseDeclaration.UseDeclaration` node.
 */
export function printUseDeclaration(
	path: AstPath<Node>,
	options: ParserOptions,
	print: printFn,
): Doc {
	const firstChild = path.node.child(0);
	const isPublic = firstChild && firstChild.type === 'public' ? ['public', ' '] : [];
	return group([
		...isPublic, // insert `public` keyword if present
		'use',
		' ',
		path.call(print, 'nonFormattingChildren', 0),
		';',
	]);
}

/**
 * Print `use_module` node. `module_name`
 */
export function printUseModule(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	return path.map((e) => {
		if (e.node.type == 'as') return ' as ';
		return print(e);
	}, 'children');
}

/**
 * Print `use_member` node. `member_name`
 * TODO: finish wrapping 2nd level nested grouping
 */
export function printUseMember(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	const isGroup = path.node.children.findIndex((e) => e.type == '{');

	// not found `::{...}`
	if (isGroup == -1) {
		return group(
			path.map((e) => {
				if (e.node.type == 'as') return ' as ';
				if (e.node.type == ',') return [',', line];
				return print(e);
			}, 'children'),
		);
	}

	const children = path.map(print, 'nonFormattingChildren');

	return group([
		children[0]!,
		'::{',
		indent(softline),
		indent(join([',', line], children.slice(1))),
		ifBreak(','), // trailing comma
		softline,
		'}',
	]);
}

/**
 * Print `use_module_member` node. `module_name::member_name`
 * Single statement of direct import;
 * `use address::module_name::member_name;`
 */
export function printUseModuleMember(
	path: AstPath<Node>,
	options: ParserOptions,
	print: printFn,
): Doc {
	return path.map(print, 'children');
}

/**
 * Print `use_module_members` node. `module_name::{member_name, member_name}`
 */
export function printUseModuleMembers(
	path: AstPath<Node>,
	options: ParserOptions,
	print: printFn,
): Doc {
	const children = path.map(print, 'nonFormattingChildren');

	return group([
		children[0]!,
		'::{',
		indent(softline),
		indent(join([',', line], children.slice(1))),
		ifBreak(','), // trailing comma
		softline,
		'}',
	]);
}

export function printUseFun(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
	return [
		'fun',
		' ',
		path.call(print, 'nonFormattingChildren', 0), // module_access
		' ',
		'as',
		' ',
		path.call(print, 'nonFormattingChildren', 1), // module_access
		'.',
		path.call(print, 'nonFormattingChildren', 2), // function_identifier
	];
}

export function printModuleIdentity(
	path: AstPath<Node>,
	options: ParserOptions,
	print: printFn,
): Doc {
	return join('::', path.map(print, 'nonFormattingChildren'));
}

export function printFriendDeclaration(
	path: AstPath<Node>,
	options: ParserOptions,
	print: printFn,
): Doc {
	return group([
		'friend',
		' ',
		path.call(print, 'nonFormattingChildren', 0), // module_access
		';',
	]);
}

export function printFriendAccess(
	path: AstPath<Node>,
	options: ParserOptions,
	print: printFn,
): Doc {
	return path.map(print, 'nonFormattingChildren');
}
