// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '../..';
import { MoveOptions, printFn, treeFn } from '../../printer';
import { AstPath, Doc, doc } from 'prettier';
import { block, shouldBreakFirstChild } from '../../utilities';
const { group, indent, join, conditionalGroup, hardlineWithoutBreakParent } = doc.builders;

/** The type of the node implemented in this file */
const NODE_TYPE = 'block';

export default function (path: AstPath<Node>): treeFn | null {
    if (path.node.type === NODE_TYPE) {
        return printBlock;
    }

    return null;
}

/**
 * Special case of `block` node, that does not break the parent. A must-have for
 * lambda expressions.
 */
export function printNonBreakingBlock(
    path: AstPath<Node>,
    options: MoveOptions,
    print: printFn,
): Doc {
    const length = path.node.nonFormattingChildren.length;

    if (length == 0) {
        return '{}';
    }

    return group([
        '{',
        indent(hardlineWithoutBreakParent),
        indent(join(hardlineWithoutBreakParent, path.map(print, 'namedAndEmptyLineChildren'))),
        hardlineWithoutBreakParent,
        '}',
    ]);
}

export function printBreakableBlock(
    path: AstPath<Node>,
    options: MoveOptions,
    print: printFn,
): Doc {
    const length = path.node.nonFormattingChildren.length;

    if (length == 0) {
        return '{}';
    }

    return block({
        options,
        print,
        path,
        shouldBreak: shouldBreakFirstChild(path),
    });
}

/**
 * Print `block` node.
 */
export function printBlock(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    return conditionalGroup([
        printBreakableBlock(path, options, print),
        printNonBreakingBlock(path, options, print),
    ]);
}
