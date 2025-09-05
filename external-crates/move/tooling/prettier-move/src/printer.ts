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
import Common from './cst/common';
import Formatting from './cst/formatting';
import Module from './cst/module';
import UseDeclaration from './cst/use_declaration';
import Constant from './cst/constant';
import StructDefinition from './cst/struct_definition';
import FunctionDefinition from './cst/function_definition';
import SourceFile from './cst/source_file';
import Expression from './cst/expression';
import Literal from './cst/literal';
import { printLeadingComment, printTrailingComment } from './utilities';
import EnumDefinition from './cst/enum_definition';
import Annotation from './cst/annotation';

export type MoveOptions = ParserOptions & {
    wrapComments: boolean;
    alwaysBreakConditionals: boolean;
    alwaysBreakStructDefinition: boolean;
    useModuleLabel: boolean;
    enableErrorDebug: boolean;
    autoGroupImports: 'package' | 'module';
};

export type printFn = (path: AstPath) => Doc;
export type treeFn = (path: AstPath<Node>, options: MoveOptions, print: printFn) => Doc;

/**
 * Print the AST node at the given path.
 */
export function print(path: AstPath<Node>, options: MoveOptions, print: printFn) {
    // check if the node has an error child, if so, we throw an error or return the error text
    const checkErrorsCb = (path: AstPath<Node>) => {
        if (path.node.children.some((n) => n.type === 'ERROR')) {
            if (options.enableErrorDebug) {
                return ((path, options, print) => ['/* ERROR: */', path.node.text]) as treeFn;
            } else {
                throw new Error('tree-sitter failure on \n```\n' + path.node.text + '\n```');
            }
        }

        if (path.node.children.some((n) => n.type === 'MISSING')) {
            if (options.enableErrorDebug) {
                return ((path, options, print) => ['/* MISSING: */', path.node.text]) as treeFn;
            } else {
                throw new Error('tree-sitter failure on \n```\n' + path.node.text + '\n```');
            }
        }

        return null;
    };

    // for unimplemented / not yet implemented nodes, we just return the node type
    const defautCb: treeFn = (path, options, print) => {
        return path.node.type;
    };

    const fn =
        checkErrorsCb(path) ||
        SourceFile(path) ||
        Annotation(path) ||
        Formatting(path) ||
        Common(path) ||
        Module(path) ||
        UseDeclaration(path) ||
        Constant(path) ||
        EnumDefinition(path) ||
        StructDefinition(path) ||
        FunctionDefinition(path) ||
        Expression(path) ||
        Literal(path) ||
        defautCb;

    return [
        printLeadingComment(path, options),
        // if the node has a `skipFormattingNode` property, we just return
        // the text without formatting it
        path.node.skipFormattingNode ? path.node.text : fn(path, options, print),
        printTrailingComment(path),
    ];
}
