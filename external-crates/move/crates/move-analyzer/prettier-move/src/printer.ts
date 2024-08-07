// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

/**
 * Implements the printing logic for the Move language. Takes a syntax tree and
 * returns a formatted string.
 *
 * @see [Prettier Plugin API](https://prettier.io/docs/en/dev/plugins.html)
 * @module printer
 */

import { AstPath, Doc, ParserOptions } from 'prettier';
import { Node } from '.';
import Common from './cst/Common';
import Formatting from './cst/Formatting';
import Module from './cst/Module';
import UseDeclaration from './cst/UseDeclaration';
import Constant from './cst/Constant';
import StructDefinition from './cst/StructDefinition';
import FunctionDefinition from './cst/function/FunctionDefinition';
import BlockItem from './cst/function/BlockItem';
import SourceFile from './cst/SourceFile';
import Expression from './cst/function/Expression';
import Literal from './cst/Literal';
import ControlFlow from './cst/function/ControlFlow';
import { printLeadingComment, printTrailingComment } from './utilities';
import EnumDefinition from './cst/EnumDefinition';

export type MoveOptions = {
	wrapComments: boolean;
	alwaysBreakFunctions: boolean;
	alwaysBreakConditionals: boolean;
	alwaysBreakStructDefinition: boolean;
	useModuleLabel: boolean;
	autoGroupImports: 'package' | 'module';
};

export type printFn = (path: AstPath) => Doc;
export type treeFn = (
	path: AstPath<Node>,
	options: ParserOptions & MoveOptions,
	print: printFn,
) => Doc;

/**
 * Print the AST node at the given path.
 */
export function print(path: AstPath<Node>, options: ParserOptions & MoveOptions, print: printFn) {
	const defautCb: treeFn = (path, options, print) => {
		return path.node.type;
	};

	const fn =
		SourceFile(path) ||
		BlockItem(path) ||
		Formatting(path) ||
		Common(path) ||
		Module(path) ||
		UseDeclaration(path) ||
		Constant(path) ||
		ControlFlow(path) ||
		EnumDefinition(path) ||
		StructDefinition(path) ||
		FunctionDefinition(path) ||
		Expression(path) ||
		Literal(path) ||
		defautCb;

	return [
		printLeadingComment(path),
		// if the node has a `skipFormattingNode` property, we just return
		// the text without formatting it
		path.node.skipFormattingNode ? path.node.text : fn(path, options, print),
		printTrailingComment(path),
	];
}
