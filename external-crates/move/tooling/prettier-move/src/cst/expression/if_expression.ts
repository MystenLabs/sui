// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '../..';
import { MoveOptions, printFn, treeFn } from '../../printer';
import { AstPath, Doc, doc } from 'prettier';
const { group, softline, line, ifBreak, indent, indentIfBreak, hardlineWithoutBreakParent } =
    doc.builders;

/** The type of the node implemented in this file */
export const NODE_TYPE = 'if_expression';

export default function (path: AstPath<Node>): treeFn | null {
    if (path.node.type === NODE_TYPE) {
        return printIfExpression;
    }

    return null;
}

/**
 * Print `if_expression` node.
 *
 * ```
 * // single line
 * if (cond || cond) {}
 *
 * // multi line + block
 * if (
 *  long_cond ||
 *  long_cond
 * ) {
 *    long_expr;
 *    long_expr;
 * }
 *
 * // multi line + single line
 * if (cond) {
 * 	return long_expr;
 * }
 *
 * // multi line + single line + long expr
 * if (
 * 	long_cond ||
 *  long_cond
 * ) {
 * 	return long_expr &&
 * 		long_expr;
 * }
 * ```
 */
function printIfExpression(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    if (path.node.nonFormattingChildren.length < 2) {
        throw new Error('Invalid if_expression node');
    }

    const hasElse = path.node.children.some((e) => e.type == 'else');
    const condition = path.node.nonFormattingChildren[0]!;
    const trueBranch = path.node.nonFormattingChildren[1]!;
    const groupId = Symbol('if_expression_true');
    const result: Doc[] = [];

    const conditionGroup = group([
        'if (',
        condition?.isList
            ? [indent(softline), path.call(print, 'nonFormattingChildren', 0), softline]
            : [indent(softline), indent(path.call(print, 'nonFormattingChildren', 0)), softline],
        ')',
    ]);

    result.push(conditionGroup);

    const isTrueList = trueBranch?.isList || false;
    const truePrinted = path.call(print, 'nonFormattingChildren', 1);
    let trueHasComment = false;

    // true branch group
    if (isTrueList) {
        trueHasComment =
            trueBranch.leadingComment.some((e) => e.type == 'line_comment') ||
            trueBranch.trailingComment?.type == 'line_comment';

        result.push(group([' ', truePrinted], { shouldBreak: false }));
    } else {
        result.push(group([indent(line), indent(truePrinted)], { id: groupId }));
    }

    // early return if there's no else block
    if (!hasElse) {
        return result;
    }

    const elseNode = path.node.nonFormattingChildren[2]!;

    // modify the break condition for the else block
    const elseShouldBreak =
        elseNode.leadingComment.some((e) => e.type == 'line_comment') ||
        elseNode.trailingComment?.type == 'line_comment';

    // if true branch is a list, and there's no line comment, we add a space,
    // if there's a line comment, we add a line break
    //
    // also, if the else block is another `if_expression` we follow the same
    // logic as above
    if (isTrueList || elseNode.type == 'if_expression') {
        result.push(group([line, 'else'], { shouldBreak: trueHasComment }));
        result.push([' ', path.call(print, 'nonFormattingChildren', 2)]);
        return result;
    }

    const elseBranchPrinted = path.call(print, 'nonFormattingChildren', 2);

    // if true branch is not a list, and else is a list, we newline
    if (elseNode.isList && !isTrueList) {
        result.push([hardlineWithoutBreakParent, 'else ', elseBranchPrinted]);
        return result;
    }

    result.push(
        group([
            ifBreak(
                [
                    hardlineWithoutBreakParent,
                    'else',
                    group([indent(line), indent(elseBranchPrinted)]),
                ],
                [
                    line,
                    'else',
                    group([indent(line), elseBranchPrinted], { shouldBreak: elseShouldBreak }),
                ],
            ),
        ]),
    );

    return result;
}
