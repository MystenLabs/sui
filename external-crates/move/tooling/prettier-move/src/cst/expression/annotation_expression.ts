// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '../..';
import { MoveOptions, printFn, treeFn } from '../../printer';
import { AstPath, Doc, doc } from 'prettier';
const { group, indent, softline } = doc.builders;

/** The type of the node implemented in this file */
export const NODE_TYPE = 'annotation_expression';

export default function (path: AstPath<Node>): treeFn | null {
    if (path.node.type === NODE_TYPE) {
        return printAnnotationExpression;
    }

    return null;
}

/**
 * Print `annotation_expression` node.
 */
function printAnnotationExpression(path: AstPath<Node>, _opt: MoveOptions, print: printFn): Doc {
    const children = path.map(print, 'nonFormattingChildren');
    if (children.length !== 2) {
        throw new Error('`annotation_expression` node should have 2 children');
    }

    return group([
        '(',
        indent(softline),
        indent(children[0]!), // expression
        ': ',
        children[1]!, // type
        softline,
        ')',
    ]);
}
