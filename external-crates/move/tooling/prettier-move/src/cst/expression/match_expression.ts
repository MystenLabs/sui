// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '../..';
import { MoveOptions, printFn, treeFn } from '../../printer';
import { AstPath, Doc } from 'prettier';
import { list } from '../../utilities';
import { builders } from 'prettier/doc';
const { join, indent, group, softline, line } = builders;

/** The type of the node implemented in this file */
export const NODE_TYPE = 'match_expression';

export default function (path: AstPath<Node>): treeFn | null {
    if (path.node.type === NODE_TYPE) {
        return printMatchExpression;
    } else if (path.node.type === 'match_arm') {
        return printMatchArm;
    } else if (path.node.type === 'match_condition') {
        return printMatchCondition;
    }

    return null;
}

/**
 * Print `match_expression` node.
 * Inside:
 * - `match`
 * - `(`
 * - `_expression`
 * - `)`
 * - `_match_body`
 */
function printMatchExpression(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    const condNode = path.node.nonFormattingChildren[0]!;
    const parts: Doc[] = ['match '];

    if (condNode.isBreakableExpression) {
        parts.push('(', path.call(print, 'nonFormattingChildren', 0), ')');
    } else {
        parts.push(
            group([
                '(',
                indent(softline),
                indent(path.call(print, 'nonFormattingChildren', 0)),
                softline,
                ')',
            ]),
        );
    }

    parts.push(
        ' ',
        list({
            path,
            print,
            options,
            open: '{',
            close: '}',
            skipChildren: 1,
            shouldBreak: true,
        }),
    );

    return parts;
}

/**
 * Print `match_arm` node.
 */
function printMatchArm(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    const children = path.map(print, 'nonFormattingChildren');

    if (children.length < 2) {
        throw new Error('`match_arm` node should have at least 2 children');
    }

    if (children.length == 2) {
        return group(join(' => ', children));
    }

    if (children.length == 3) {
        return [children[0]!, ' ', children[1]!, group([' =>', indent(line), children[2]!])];
    }

    throw new Error('`match_arm` node should have at most 3 children');
}

/**
 * Prints `match_condition` node in `match_arm`.
 * Example: `Enum if (x == 1) => 1,`, `if (...)` here is a `match_condition` node.
 */
function printMatchCondition(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    const children = path.node.nonFormattingChildren;

    if (children.length !== 1) {
        throw new Error('`match_condition` expects 1 child');
    }

    return ['if (', path.call(print, 'nonFormattingChildren', 0), ')'];
}
