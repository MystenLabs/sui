// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '..';
import { MoveOptions, printFn, treeFn } from '../printer';
import { AstPath, Doc, doc } from 'prettier';
import { inlineTrailingComment, printIdentifier } from '../utilities';
import * as VectorExpression from './expression/vector_expression';
import { printBreakableBlock } from './expression/block';
const { line, group, hardline, join, fill, ifBreak, softline, indent, lineSuffix } = doc.builders;

/** The type of the node implemented in this file */
export const NODE_TYPE = 'constant';

/**
 * Prints:
 * - `constant`
 * - `constant_identifier`
 */
export default function (path: AstPath<Node>): treeFn | null {
    if (path.node.type === NODE_TYPE) {
        return printConstant;
    } else if (path.node.type === 'constant_identifier') {
        return printIdentifier;
    }

    return null;
}

/**
 * Print `constant` node.
 *
 * See `module-members/constant.move` for tests.
 */
function printConstant(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    const expression = path.node.nonFormattingChildren[2];
    const typeNode = path.node.nonFormattingChildren[1];
    const trailing = lineSuffix(inlineTrailingComment(path));
    path.node.disableTrailingComment();

    // a trailing line comment on the type is printed after the `=`
    // (`const C: u8 = // comment`), and the value moves to the next line
    let eqComment: Doc = '';
    const printCb = (path: AstPath<Node>) => {
        if (path.node === typeNode && path.node.trailingComment?.type == 'line_comment') {
            eqComment = lineSuffix(inlineTrailingComment(path));
            path.node.disableTrailingComment();
        }
        return printConstExpression(path, options, print);
    };
    const groupId = Symbol('type_group');

    if (path.node.nonFormattingChildren.length !== 3) {
        throw new Error('`constant` expects 3 children');
    }

    const [identDoc, typeDoc, exprDoc] = path.map(printCb, 'nonFormattingChildren');
    const parts = [] as Doc[];

    // const <ident> : <type> = <expr>;
    parts.push('const ', identDoc!);
    parts.push(': ', group(typeDoc!, { id: groupId }), ' =', eqComment);

    if (eqComment !== '') {
        parts.push(indent(hardline), indent(exprDoc!));
    } else if (expression?.isList) {
        parts.push(
            group([
                ifBreak(indent(line), ' ', { groupId }),
                ifBreak(indent(exprDoc!), exprDoc, { groupId }),
            ]),
        );
    } else {
        parts.push(group([indent(line), indent(exprDoc!)]));
    }

    return parts.concat([';', trailing]);
}

// Sub-router for expressions in the const declaration. Special cases are:
//
// - for vectors with `num` and `bool` literals, we want to fill single line
// - for blocks we want breakability
function printConstExpression(path: AstPath<Node>, options: MoveOptions, print: printFn) {
    if (path.node.type === VectorExpression.NODE_TYPE) {
        return prettyNumVector(path, options, print);
    }

    if (path.node.type === 'block') {
        return printBreakableBlock(path, options, print);
    }

    return print(path);
}

// TODO: optionally move this to `VectorExpression`
function prettyNumVector(path: AstPath<Node>, options: MoveOptions, print: printFn) {
    let elType = path.node.nonFormattingChildren[0]?.type;
    if (elType && ['num_literal', 'bool_literal'].includes(elType)) {
        let allSameType = !path.node.nonFormattingChildren.some((e) => e.type !== elType);
        let hasComments = path.node.namedChildren.some(
            (e) => e.trailingComment || e.leadingComment.length > 0,
        );

        if (allSameType && !hasComments) {
            const literals = path.map(print, 'nonFormattingChildren');

            if (literals.length == 0) {
                return 'vector[]';
            }

            const elements = join([',', line], literals);
            return [
                'vector[',
                group([indent(softline), indent(fill(elements)), ifBreak(','), softline]),
                ']',
            ];
        }
    }

    return print(path);
}
