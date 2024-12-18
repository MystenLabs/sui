// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '../..';
import { MoveOptions, printFn, treeFn } from '../../printer';
import { AstPath, Doc, doc } from 'prettier';
const { group, indent, line } = doc.builders;

/** The type of the node implemented in this file */
const NODE_TYPE = 'let_statement';

export default function (path: AstPath<Node>): treeFn | null {
	if (path.node.type === NODE_TYPE) {
		return printLetStatement;
	}

	return null;
}

/**
 * Print `let_statement` node.
 */
function printLetStatement(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
	const nodes = path.node.nonFormattingChildren;

	if (nodes.length === 1) {
		return group(['let', ' ', path.call(print, 'nonFormattingChildren', 0)]);
	}

	const printed = path.map(print, 'nonFormattingChildren');
	const rhsNode = path.node.nonFormattingChildren.slice(-1)[0]!;

	if (nodes.length === 2 && nodes[1]!.isTypeParam) {
		const [bind, type] = printed;
		return group(['let ', bind!, ': ', type!]);
	}

	if (nodes.length === 2) {
		const [bind, expr] = printed;
		const result =
			rhsNode.isBreakableExpression || rhsNode.isFunctionCall
				? ['let ', bind!, ' = ', expr!]
				: ['let ', bind!, ' =', indent(group([line, expr!], { shouldBreak: false }))];

		return group(result, { shouldBreak: false });
	}

	const [bind, type, expr] = printed;
	const result =
		rhsNode.isBreakableExpression || rhsNode.isFunctionCall
			? ['let ', bind!, ': ', type!, ' = ', expr!]
			: [
					'let ',
					bind!,
					': ',
					type!,
					' =',
					indent(group([line, expr!], { shouldBreak: false })),
				];

	return result;
}
