// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '../..';
import { MoveOptions, printFn, treeFn } from '../../printer';
import { AstPath, Doc, doc } from 'prettier';
import { printLeadingComment, printTrailingComment } from '../../utilities';
const { breakParent, group, indent, line } = doc.builders;

/** The type of the node implemented in this file */
export const NODE_TYPE = 'binary_expression';

/**
 * Precedence classes of the binary operators. Only used to decide which
 * nested chains are flattened and printed as a single breakable unit;
 * the parse tree already encodes the actual evaluation order.
 */
const PRECEDENCE: Record<string, number> = {
    '||': 1,
    '&&': 2,
    '==': 3,
    '!=': 3,
    '<': 3,
    '>': 3,
    '<=': 3,
    '>=': 3,
    '|': 4,
    '^': 5,
    '&': 6,
    '<<': 7,
    '>>': 7,
    '+': 8,
    '-': 8,
    '*': 9,
    '/': 9,
    '%': 9,
};

export default function (path: AstPath<Node>): treeFn | null {
    if (path.node.type === NODE_TYPE) {
        return printBinaryExpression;
    } else if (path.node.type === 'binary_operator') {
        return () => path.node.text;
    }

    return null;
}

function operatorPrecedence(node: Node): number {
    const op = node.nonFormattingChildren[1]!.text;
    return PRECEDENCE[op] || 0;
}

/**
 * Print `binary_expression` node.
 *
 * Chains of operators with the same precedence (`a + b - c`) are flattened
 * and break as one unit, rustfmt-style: the operator starts the continuation
 * line, indented one level:
 * ```
 * bytes == &b"bool"
 *     || bytes == &b"u8"
 *     || bytes == &b"u16"
 * ```
 * Sub-expressions of a different precedence form their own group and may
 * break independently.
 */
function printBinaryExpression(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    if (path.node.nonFormattingChildren.length != 3) {
        throw new Error('`binary_expression` node should have 3 children');
    }

    // a comment above the operator cannot render on a flat line — force the
    // chain to break (the comment then naturally sits above the operator)
    const opNode = path.node.nonFormattingChildren[1]!;
    const opBreak = opNode.leadingComment.length > 0 ? breakParent : '';

    // a line comment trailing the operator (`a && // comment`) is emitted at
    // the end of the lhs line — in the leading-operator style the operator's
    // own line also holds the rhs, where the comment would end up misplaced
    let opTrailing: Doc = '';
    const opDoc = path.call(
        (path) => {
            if (path.node.trailingComment?.type == 'line_comment') {
                opTrailing = printTrailingComment(path);
                path.node.disableTrailingComment();
            }
            return print(path);
        },
        'nonFormattingChildren',
        1,
    );

    // an own-line comment above the rhs operand moves with it: it is printed
    // on its own line above the operator's continuation line
    let rhsComment: Doc = '';
    const rhs = path.call(
        (path) => {
            const comments = path.node.leadingComment;
            if (comments.some((c) => c.type == 'line_comment' || c.newline)) {
                rhsComment = [printLeadingComment(path, options), breakParent];
                path.node.disableLeadingComment();
            }
            return print(path);
        },
        'nonFormattingChildren',
        2,
    );

    const rhsBreaksItself = breaksItself(path.node.nonFormattingChildren[2]!);
    const head = [
        path.call(print, 'nonFormattingChildren', 0), // lhs (flattened if same precedence)
        opTrailing,
        opBreak,
        indent([rhsBreaksItself ? (' ' as Doc) : line, rhsComment, opDoc]),
        ' ',
    ];

    // a same-precedence chain is grouped once, at its top; the parts of the
    // nested lhs are spliced into the parent flat
    const parent = path.node.parent;
    if (
        parent?.type === NODE_TYPE &&
        operatorPrecedence(parent) === operatorPrecedence(path.node)
    ) {
        return [head, rhs];
    }

    // a self-breaking rhs (block, parens, vector) is kept out of the chain
    // group and breaks independently, so the chain's own break decision does
    // not depend on the rhs size
    if (rhsBreaksItself) {
        return [group(head), rhs];
    }

    return group([head, rhs]);
}

/**
 * Nodes that manage their own line breaking stay on the operator's line
 * instead of moving to the next one (`x == vector[` + break inside).
 */
function breaksItself(node: Node): boolean {
    return ['block', 'expression_list', 'vector_expression'].includes(node.type);
}
