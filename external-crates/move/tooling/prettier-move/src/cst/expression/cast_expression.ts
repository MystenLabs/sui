// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '../..';
import { MoveOptions, printFn, treeFn } from '../../printer';
import { AstPath, Doc, doc } from 'prettier';
const { join } = doc.builders;

/** The type of the node implemented in this file */
export const NODE_TYPE = 'cast_expression';

export default function (path: AstPath<Node>): treeFn | null {
    if (path.node.type === NODE_TYPE) {
        return printCastExpression;
    }

    return null;
}

/**
 * Print `cast_expression` node.
 * Inside:
 * - `expression`
 * - `type`
 */
function printCastExpression(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    const parens = path.node.child(0)?.text == '(';
    const children = path.map(print, 'nonFormattingChildren');

    if (parens) {
        return ['(', join(' as ', children), ')'];
    }

    return join(' as ', children);
}
