// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '../..';
import { MoveOptions, printFn, treeFn } from '../../printer';
import { AstPath, Doc, doc } from 'prettier';
import { list, printTrailingComment } from '../../utilities';
import { printBreakableBlock } from './block';
const { group, lineSuffix } = doc.builders;

/** The type of the node implemented in this file */
export const NODE_TYPE = 'vector_expression';

export default function (path: AstPath<Node>): treeFn | null {
    if (path.node.type === NODE_TYPE) {
        return printVectorExpression;
    }

    return null;
}

/**
 * Print `vector_expression` node.
 */
function printVectorExpression(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    if (path.node.namedChildCount === 0) {
        return 'vector[]';
    }

    // Injected print callback for elements in the vector
    const printCb = (path: AstPath<Node>) => printElement(path, options, print);
    const trailing = path.node.trailingComment;
    let trailingComment: Doc = '';

    if (trailing?.type === 'line_comment') {
        trailingComment = printTrailingComment(path, false);
        path.node.disableTrailingComment();
    }

    // Vector without type specified
    // Eg: `vector[....]`
    if (path.node.child(0)?.text == 'vector[') {
        return group(
            [
                'vector',
                list({ path, print: printCb, options, open: '[', close: ']' }) as Doc[],
                lineSuffix(trailingComment),
            ],
            { shouldBreak: false },
        );
    }

    if (!path.node.nonFormattingChildren[0]?.isTypeParam) {
        throw new Error(
            `Expected a type parameter in the \`vector_expression\`, got \`${path.node.nonFormattingChildren[0]?.type}\``,
        );
    }

    if (path.node.nonFormattingChildren.slice(1).some((child) => child.isTypeParam)) {
        throw new Error('Expected only one type parameter in the `vector_expression`');
    }

    // Vector with type
    // Eg: `vector<TYPE>[...]`
    return [
        'vector<',
        // do not break the type in vector literal
        // indent(softline),
        group(path.call(print, 'nonFormattingChildren', 0), { shouldBreak: false }),
        '>',
        group(
            list({
                path,
                print: printCb,
                options,
                open: '[',
                close: ']',
                skipChildren: 1,
                shouldBreak: false,
            }) as Doc[],
        ),
        lineSuffix(trailingComment),
    ];
}

/**
 * Special print elements in the `vector_expression`.
 *
 * - we want to use breakable blocks for `block` nodes;
 */
function printElement(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    if (path.node.type === 'block') {
        return printBreakableBlock(path, options, print);
    }

    return print(path);
}
