// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '../..';
import { MoveOptions, printFn, treeFn } from '../../printer';
import { AstPath, Doc, doc } from 'prettier';
import { printTrailingComment } from '../../utilities';
const { group, indent, line } = doc.builders;

/** The type of the node implemented in this file */
export const NODE_TYPE = 'assign_expression';

export default function (path: AstPath<Node>): treeFn | null {
    if (path.node.type === NODE_TYPE) {
        return printAssignExpression;
    }

    return null;
}

/**
 * Print `assign_expression` node.
 */
function printAssignExpression(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    if (path.node.nonFormattingChildren.length !== 2) {
        throw new Error('`assign_expression` must have 2 children');
    }

    const result: Doc[] = [];
    let shouldBreak = false;

    // together with the LHS we print trailing comment if there is one
    result.push(
        path.call(
            (lhs) => {
                const hasComment = !!lhs.node.trailingComment;

                if (lhs.node.trailingComment?.type == 'line_comment') {
                    shouldBreak = true;
                    const trailingLineComment = printTrailingComment(lhs, true);
                    lhs.node.disableTrailingComment();
                    return [print(lhs), ' =', indent(trailingLineComment)];
                }

                return [print(lhs), hasComment ? '=' : ' ='];
            },
            'nonFormattingChildren',
            0,
        ),
    );

    const rhs = path.node.nonFormattingChildren[1]!;
    if ((rhs.isControlFlow || rhs.isList) && !shouldBreak) {
        result.push(group([' ', path.call(print, 'nonFormattingChildren', 1)]));
    } else {
        // then print the rhs
        result.push(
            group([
                shouldBreak ? '' : indent(line),
                indent(path.call(print, 'nonFormattingChildren', 1)),
            ]),
        );
    }

    return result;
}
