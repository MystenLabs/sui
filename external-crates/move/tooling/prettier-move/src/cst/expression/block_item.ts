// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '../..';
import { MoveOptions, printFn, treeFn } from '../../printer';
import { AstPath, Doc, doc } from 'prettier';
import { printTrailingComment } from '../../utilities';
const { group, lineSuffix } = doc.builders;

/** The type of the node implemented in this file */
const NODE_TYPE = 'block_item';

export default function (path: AstPath<Node>): treeFn | null {
    if (path.node.type === NODE_TYPE) {
        return printBlockItem;
    }

    return null;
}

/**
 * Print `block_item` node.
 */
function printBlockItem(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    const trailing = lineSuffix(printTrailingComment(path));
    path.node.disableTrailingComment();

    return [group([path.call(print, 'nonFormattingChildren', 0), ';', trailing])];
}
