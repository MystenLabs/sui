// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '../..';
import { MoveOptions, printFn, treeFn } from '../../printer';
import { AstPath, Doc, doc } from 'prettier';
const { group, line } = doc.builders;

/** The type of the node implemented in this file */
export const NODE_TYPE = 'binary_expression';

// TODO: re-enable binary expression once we figure out how to achieve it.
export default function (path: AstPath<Node>): treeFn | null {
    if (path.node.type === NODE_TYPE) {
        return () => path.node.text;
        // TODO: re-enable binary expression once we figure out how to achieve it.
        // return printBinaryExpression;
    } else if (path.node.type === 'binary_operator') {
        // return printBinaryOperator;
        // TODO: re-enable binary expression once we figure out how to achieve it.
        return () => path.node.text;
    }

    return null;
}

/**
 * Print `binary_expression` node.
 * (Currently disabled)
 */
function printBinaryExpression(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    if (path.node.nonFormattingChildren.length != 3) {
        throw new Error('`binary_expression` node should have 3 children');
    }

    const [one, two, three] = path.map(print, 'nonFormattingChildren');
    const rhs = path.node.nonFormattingChildren[2];

    if (rhs?.type === 'block' || rhs?.type === 'expression_list') {
        return [one!, ' ', two!, ' ', three!];
    }

    return [one!, ' ', two!, group([line, three!], { shouldBreak: false })];
}

/**
 * Print `binary_operator` node.
 */
export function printBinaryOperator(path: AstPath<Node>, _opt: MoveOptions, _p: printFn): Doc {
    return path.node.text;
}

/**

 or: '||',
 and:  '&&',
 eq: '==',
 neq:  '!=',
 lt: '<',
 gt: '>',
 le: '<=',
 ge: '>=',
 bit_or: '|',
 xor:  '^',
 bit_and: '&',
 shl:  '<<',
 shr:  '>>',
 add:  '+',
 sub:  '-',
 mul:  '*',
 div:  '/',
 mod:  '%'


*/
