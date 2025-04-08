// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '../..';
import { MoveOptions, printFn, treeFn } from '../../printer';
import { AstPath, Doc, doc } from 'prettier';
const { group, indent, line } = doc.builders;

/** The type of the node implemented in this file */
export const NODE_TYPE = 'assign_expression';

export default function (path: AstPath<Node>): treeFn | null {
	if (path.node.type === NODE_TYPE) {
		return printAssignExpression;
	}

	return null;
}

/**
 * Print `assign_expression` node.
 */
function printAssignExpression(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
	return [
		path.call(print, 'nonFormattingChildren', 0), // lhs
		' =',
		group([
			indent(line),
			indent(path.call(print, 'nonFormattingChildren', 1)), // rhs
		]),
	];
}
