// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '../..';
import { MoveOptions, printFn, treeFn } from '../../printer';
import { AstPath, Doc, doc } from 'prettier';
import { list } from '../../utilities';
const { group, indent, softline } = doc.builders;

/** The type of the node implemented in this file */
export const NODE_TYPE = 'expression_list';

export default function (path: AstPath<Node>): treeFn | null {
    if (path.node.type === NODE_TYPE) {
        return printExpressionList;
    }

    return null;
}

/**
 * Print `expression_list` node.
 *
 * A single-element list is grouping parentheses, not a tuple — it must not
 * get a trailing comma when broken.
 */
function printExpressionList(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    if (path.node.nonFormattingChildren.length === 1 && !path.node.containsComments) {
        return group([
            '(',
            indent([softline, path.call(print, 'nonFormattingChildren', 0)]),
            softline,
            ')',
        ]);
    }

    return group(list({ path, print, options, open: '(', close: ')' }), {
        shouldBreak: false,
    });
}
