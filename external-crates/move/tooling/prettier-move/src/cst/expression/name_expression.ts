// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '../..';
import { MoveOptions, printFn, treeFn } from '../../printer';
import { AstPath, Doc, doc } from 'prettier';
const {} = doc.builders;

/** The type of the node implemented in this file */
export const NODE_TYPE = 'name_expression';

export default function (path: AstPath<Node>): treeFn | null {
    if (path.node.type === NODE_TYPE) {
        return printNameExpression;
    }

    return null;
}

/**
 * Print `name_expression` node.
 * Inside:
 * - `module_access`
 * - `type_arguments`
 */
function printNameExpression(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    return path.map(print, 'nonFormattingChildren');
}
