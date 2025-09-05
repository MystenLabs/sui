// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '../..';
import { MoveOptions, printFn, treeFn } from '../../printer';
import { AstPath, Doc, doc } from 'prettier';
import { list } from '../../utilities';
const { group, indentIfBreak, line, softline, ifBreak, join } = doc.builders;

/** The type of the node implemented in this file */
const NODE_TYPE = 'macro_call_expression';

export default function (path: AstPath<Node>): treeFn | null {
    if (path.node.type === NODE_TYPE) {
        return printMacroCallExpression;
    }

    return null;
}

/**
 * Print `macro_call_expression` node.
 * Inside:
 * - `macro_module_access`
 * - `type_arguments`
 * - `arg_list`
 */
function printMacroCallExpression(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    return path.map((path) => {
        if (path.node.type === 'macro_module_access') {
            return printMacroModuleAccess(path, options, print);
        }

        if (path.node.type === 'arg_list') {
            return printMacroArgsList(path, options, print);
        }

        return print(path);
    }, 'nonFormattingChildren');
}

/**
 * Print `macro_module_access` node.
 * Inside:
 * - `module_access`
 * - `!`
 */
function printMacroModuleAccess(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    return [path.call(print, 'nonFormattingChildren', 0), '!'];
}

/**
 * Special function to print macro arguments list instead of `arg_list`.
 */
function printMacroArgsList(path: AstPath<Node>, options: MoveOptions, print: printFn) {
    if (path.node.type !== 'arg_list') {
        throw new Error('Expected `arg_list` node');
    }

    if (path.node.namedChildCount === 0) {
        return '()';
    }

    const groupId = Symbol('macro_args_list');

    return group(
        list({
            path,
            options,
            print,
            open: '(',
            close: ')',
            addWhitespace: false,
            shouldBreak: false,
            indentGroup: groupId,
        }),
        { id: groupId },
    );
}
