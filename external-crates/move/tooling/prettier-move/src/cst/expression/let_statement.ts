// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '../..';
import { MoveOptions, printFn, treeFn } from '../../printer';
import { AstPath, Doc, doc } from 'prettier';
import { printTrailingComment } from '../../utilities';
const { group, indent, line, indentIfBreak } = doc.builders;

/** The type of the node implemented in this file */
const NODE_TYPE = 'let_statement';

export default function (path: AstPath<Node>): treeFn | null {
    if (path.node.type === NODE_TYPE) {
        return printLetStatement;
    }

    return null;
}

/**
 * Print `let_statement` node.
 */
function printLetStatement(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    const nodes = path.node.nonFormattingChildren;

    if (nodes.length === 1) {
        return group(['let', ' ', path.call(print, 'nonFormattingChildren', 0)]);
    }

    function printWithTrailing(path: AstPath<Node>): Doc {
        let trailingComment: Doc = '';
        if (path.node.trailingComment?.type == 'line_comment') {
            trailingComment = printTrailingComment(path, true);
            path.node.disableTrailingComment();
        }
        return [print(path), trailingComment];
    }

    const printed = path.map(printWithTrailing, 'nonFormattingChildren');
    const rhsNode = path.node.nonFormattingChildren.slice(-1)[0]!;

    if (nodes.length === 2 && nodes[1]!.isTypeParam) {
        const [bind, type] = printed;
        return group(['let ', bind!, ': ', type!]);
    }

    if (nodes.length === 2) {
        const [bind, expr] = printed;
        const result =
            rhsNode.isBreakableExpression || rhsNode.isFunctionCall || rhsNode.isControlFlow
                ? ['let ', bind!, ' = ', expr!]
                : ['let ', bind!, ' =', printLetExpression(expr!, rhsNode)];

        return group(result, { shouldBreak: false });
    }

    const [bind, type, expr] = printed;
    const result =
        rhsNode.isBreakableExpression || rhsNode.isFunctionCall
            ? ['let ', bind!, ': ', type!, ' = ', expr!]
            : ['let ', bind!, ': ', type!, ' =', printLetExpression(expr!, rhsNode)];

    return result;
}

function printLetExpression(expression: Doc, node: Node) {
    const groupId = Symbol('let_expression');
    return group([indentIfBreak(line, { groupId }), indentIfBreak(expression, { groupId })], {
        shouldBreak: false,
        id: groupId,
    });
}
