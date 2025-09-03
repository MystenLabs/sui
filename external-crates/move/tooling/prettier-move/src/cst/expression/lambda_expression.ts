// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '../..';
import { MoveOptions, printFn, treeFn } from '../../printer';
import { AstPath, Doc, doc } from 'prettier';
import { printBreakableBlock, printNonBreakingBlock } from './block';
import { list } from '../../utilities';
const { group, join, conditionalGroup } = doc.builders;

/** The type of the node implemented in this file */
const NODE_TYPE = 'lambda_expression';

export default function (path: AstPath<Node>): treeFn | null {
    switch (path.node.type) {
        case NODE_TYPE:
            return printLambdaExpression;
        case 'lambda_bindings':
            return printLambdaBindings;
        case 'lambda_binding':
            return printLambdaBinding;
    }

    return null;
}

/**
 * Print `labda_expression` node.
 * Inside:
 * - `lambda_bindings`
 * - `_bind`
 */
function printLambdaExpression(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    const children = path.node.nonFormattingChildren;

    // just bindings
    if (children.length === 1) {
        return path.call(print, 'nonFormattingChildren', 0);
    }

    // bindings, expression or bindings, return type
    if (children.length === 2) {
        return join(' ', path.map(print, 'nonFormattingChildren'));
    }

    // bindings, return type, expression
    if (children.length === 3) {
        return [
            path.call(print, 'nonFormattingChildren', 0), // bindings
            ' -> ',
            path.call(print, 'nonFormattingChildren', 1), // return type
            ' ',
            path.call(print, 'nonFormattingChildren', 2), // expression
        ];
    }

    throw new Error('`lambda_expression` node should have 1, 2 or 3 children');
}

/**
 * Print `lambda_bindings` node, contains comma-separated list of `lambda_binding` nodes.
 */
function printLambdaBindings(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    return group(list({ path, print, options, open: '|', close: '|' }));
}

/**
 * Print `lambda_binding` node.
 * It can be either type annotated or just a variable binding, we know it by the number
 * of non-formatting children.
 */
function printLambdaBinding(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    // simple bind, will be handled by its function
    if (path.node.nonFormattingChildren.length === 1) {
        return path.call(print, 'nonFormattingChildren', 0);
    }

    // with type annotation
    if (path.node.nonFormattingChildren.length === 2) {
        return join(': ', path.map(print, 'nonFormattingChildren'));
    }

    throw new Error('`lambda_binding` node should have 1 or 2 children');
}
