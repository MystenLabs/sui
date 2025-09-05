// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '../..';
import { MoveOptions, printFn, treeFn } from '../../printer';
import { AstPath, Doc, doc } from 'prettier';
import { printLeadingComment, printTrailingComment } from '../../utilities';
const { group, indent, ifBreak, breakParent, lineSuffix, softline } = doc.builders;

/** The type of the node implemented in this file */
export const NODE_TYPE = 'dot_expression';

export default function (path: AstPath<Node>): treeFn | null {
    if (path.node.type === NODE_TYPE) {
        return printDotExpression;
    }

    return null;
}

/**
 * Print `dot_expression` node.
 *
 * Note, that it's intentional, that the return value is not a `group`. For more info,
 * @see [This Issue](https://github.com/prettier/prettier/issues/15710#issuecomment-1836701758)
 */
function printDotExpression(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    if (path.node.nonFormattingChildren.length < 2) {
        throw new Error('`dot_expression` node should have at least 2 children');
    }

    // chain is a `dot_expression` that is a child of another `dot_expression`
    // or which has a `dot_expression` as a child
    const isChain =
        path.node.nonFormattingChildren[0]!.type === NODE_TYPE ||
        path.node.parent?.type === NODE_TYPE;

    const isParentList = path.node.parent?.isList;

    // if dot expression has a trailing comment and it breaks, we need to
    // print it manually after the rhs
    const trailing = lineSuffix(printTrailingComment(path));

    const lhs = path.call(
        (path) => printNode(path, options, print, false),
        'nonFormattingChildren',
        0,
    );
    const rhs = path.call(
        (path) => printNode(path, options, print, true),
        'nonFormattingChildren',
        1,
    );

    // if it's a single expression, we don't need to group it
    // and optionally no need to break it; no need to special
    // print it in this case
    if (!isChain) {
        const right = path.node.nonFormattingChildren[1]!;
        if (right.leadingComment.length > 0) {
            path.node.disableTrailingComment();
            return [lhs, indent(softline), indent(rhs), trailing];
        }

        return [lhs, rhs];
    }

    path.node.disableTrailingComment();
    const parts = [lhs, ifBreak(indent(softline), ''), ifBreak(indent(rhs), rhs), trailing];

    // group if parent is not `dot_expression`
    if (isChain && path.node.parent?.type !== NODE_TYPE) {
        return group(parts);
    }

    return parts;
}

// In `dot_expression` we need to keep the `.` in the same line as the `rhs`,
// so we need to prevent automatic printing of comments in the `print` call, and
// perform it manually.
function printNode(path: AstPath<Node>, options: MoveOptions, print: printFn, insertDot = false) {
    const leading = printLeadingComment(path, options);
    const shouldBreak =
        path.node.leadingComment.length > 0 || path.node.trailingComment?.type === 'line_comment';

    path.node.disableLeadingComment();
    return [leading, shouldBreak ? breakParent : '', insertDot ? '.' : '', print(path)];
}
