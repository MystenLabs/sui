// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '../..';
import { MoveOptions, printFn, treeFn } from '../../printer';
import { AstPath, Doc, doc } from 'prettier';
import { inlineTrailingComment, printTrailingComment } from '../../utilities';
const { group, hardline, indent, line, lineSuffix, indentIfBreak } = doc.builders;

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

    // trailing line comments on the bind or type are printed after the `=`
    // (`let x = // comment`), so they cannot swallow it; the value is then
    // forced onto the next line
    let eqComment: Doc = '';
    const isValueIndex = nodes.length - 1;

    const printed = path.map((path, i) => {
        if (path.node.trailingComment?.type != 'line_comment') {
            return print(path);
        }

        if (i === isValueIndex) {
            const trailingComment = printTrailingComment(path, true);
            path.node.disableTrailingComment();
            return [print(path), trailingComment];
        }

        eqComment = lineSuffix(inlineTrailingComment(path));
        path.node.disableTrailingComment();
        return print(path);
    }, 'nonFormattingChildren');

    const hasEqComment = eqComment !== '';
    const rhsNode = path.node.nonFormattingChildren.slice(-1)[0]!;

    if (nodes.length === 2 && nodes[1]!.isTypeParam) {
        const [bind, type] = printed;
        return group(['let ', bind!, ': ', type!, eqComment]);
    }

    // with a comment after the `=`, the value always goes to the next line
    const valueSep = hasEqComment ? hardline : ' ';

    if (nodes.length === 2) {
        const [bind, expr] = printed;
        const result =
            rhsNode.isBreakableExpression || rhsNode.isFunctionCall || rhsNode.isControlFlow
                ? ['let ', bind!, ' =', eqComment, valueSep, expr!]
                : ['let ', bind!, ' =', eqComment, printLetExpression(expr!, hasEqComment)];

        return group(result, { shouldBreak: false });
    }

    const [bind, type, expr] = printed;
    const result =
        rhsNode.isBreakableExpression || rhsNode.isFunctionCall
            ? ['let ', bind!, ': ', type!, ' =', eqComment, valueSep, expr!]
            : [
                  'let ',
                  bind!,
                  ': ',
                  type!,
                  ' =',
                  eqComment,
                  printLetExpression(expr!, hasEqComment),
              ];

    return result;
}

function printLetExpression(expression: Doc, shouldBreak: boolean = false) {
    const groupId = Symbol('let_expression');
    return group([indentIfBreak(line, { groupId }), indentIfBreak(expression, { groupId })], {
        shouldBreak,
        id: groupId,
    });
}
