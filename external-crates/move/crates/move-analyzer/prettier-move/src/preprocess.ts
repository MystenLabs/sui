// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

/**
 * Defines the `FormattedNode` proxy for the `SyntaxNode`. The proxy adds additional
 * properties to the node, such as `isFormatting`, `isEmptyLine`, `isNextLine`, etc.
 *
 * See the `newProxy` function for more information. For type safety, every property
 * added to the Proxy must be defined in the `FormattedNode` interface.
 *
 * @module preprocess
 */

import { SyntaxNode } from 'web-tree-sitter';
import { ParserOptions } from 'prettier';
import { isNextLine, isFormatting, isComment, isEmptyLine } from './cst/Formatting';

export interface FormattedNode extends SyntaxNode {
	/**
	 * Whether a node is `line_comment`, `block_comment`, `empty_line`, or `next_line`.
	 */
	isFormatting: boolean;
	/** Whether a node is a `next_line` node. */
	isEmptyLine: boolean;
	/** Whether a node is a `next_line` node. */
	isNextLine: boolean;
	/** Whether a node is a `line_comment` or `block_comment`. */
	isComment: boolean;
	/** Whether a node is a `block_comment`. */
	isBlockComment: boolean;
	/** Whether a node is a `line_comment`. */
	isLineComment: boolean;
	/** If the previous node is `next_line` */
	startsOnNewLine: boolean;
	/** If the next node is `next_line` */
	shouldNewLine: boolean;
	/** Prints all line / block comments before this node */
	leadingComment: string[];
	/** Prints trailing comments for this node */
	trailingComment: string;
	/**
	 * Whether a node is a "list" like, such as `expression_list` or `vector_literal`.
	 * Meaning that it knows how to break itself into multiple lines. The reason for
	 * this property is to not break parents if the child is breakable (eg. a constant
	 * with a vector literal value).
	 */
	isList: boolean;

    /**
     * Whether a node is a control flow node, such as `if_statement`, `while_statement`,
     * `loop_statement`, etc. This is useful to know to not break the parent node if the
     * control flow node is the first child of the parent.
     */
    isControlFlow: boolean;

    /**
     * Whether an expression is breakable. This is useful to know if the expression
     * can break itself into multiple lines, and if the parent node should break
     * itself as well.
     */
    isBreakableExpression: boolean;

	parent: FormattedNode;
	firstNamedChild: SyntaxNode;
	children: FormattedNode[];
	namedChildren: FormattedNode[];
	previousSibling: FormattedNode;
	namedChild(index: number): FormattedNode;

	/** Return all nodes that return `false` for `isFormatting` */
	nonFormattingChildren: FormattedNode[];
	/** Return all named children + empty_line node */
	namedAndEmptyLineChildren: FormattedNode[];
}

/**
 * The `SyntaxNode` -> `FormattedNode` magic. Proxy approach allows us to extend
 * the native class `SyntaxNode` with additional properties. An attempt to rebuild
 * the `SyntaxNode` class with additional properties would result in a lot of
 * memory and performance overhead.
 *
 * @param ast
 * @param options
 * @returns
 */
export function preprocess(ast: SyntaxNode, options: ParserOptions): FormattedNode {
	return newProxy(ast as FormattedNode) as FormattedNode;
}

/**
 * Create a new `SyntaxNode` proxy that adds additional properties to the node.
 * Due to `SyntaxNode` not being editable, and not being a class, we use a `Proxy`
 * to add additional properties to the node. Most importantly - we limit formatting
 * nodes from being included in the `children` and `namedChildren` properties.
 *
 * @param node
 * @returns
 */
function newProxy(node: FormattedNode): FormattedNode {
	return new Proxy<FormattedNode>(node, {
		get(target, prop, receiver) {
			const result: any = Reflect.get(target, prop, receiver);

			// Adding a `startsOnNewLine` property to the `SyntaxNode`. When
			// `startsOnNewLine` is accessed, it will return `true` if the
			// previous sibling is a `next_line` node, otherwise it will return
			// `false`.
			if (prop === 'startsOnNewLine') {
				return target.previousSibling?.type === 'next_line';
			}

			// Adding a `shouldNewLine` property to the `SyntaxNode`. When
			// `shouldNewLine` is accessed, it will return `true` if the next
			// sibling is a `next_line` node, otherwise it will return `false`.
			if (prop === 'shouldNewLine') {
				return target.nextSibling?.type === 'next_line';
			}

			// Adding a `leadingComment` property to the `SyntaxNode`. When
			// `leadingComment` is accessed, it will return all the comments
			// that are before the node until the next non-comment node.
			if (prop === 'leadingComment') {
				const comments: string[] = [];
				let node = target;

				// leading comment must have a newline character before it
				// except for the case of an empty line (it's a special case)
				if (
					!isEmptyLine(node) &&
					(!isNextLine(node.previousSibling) ||
						!isComment(node.previousSibling.previousSibling))
				) {
					return comments;
				}

				while (
					(node.previousSibling && isComment(node.previousSibling)) ||
					isNextLine(node.previousSibling)
				) {
					node = node.previousSibling;
					if (isComment(node)) comments.unshift(node.text);
				}

				// Leading comment is a comment that either is the first node,
                // or starts with a newline character, or the previous node is
                // an empty line node.
				return isNextLine(node) ||
					isEmptyLine(node.previousSibling) ||
					!node.previousSibling
					? comments
					: [];
			}

			// Adding a `isFormatting` property to the `SyntaxNode`. When
			// `isFormatting` is accessed, it will return `true` if the node is
			// a formatting node, otherwise it will return `false`.
			if (prop === 'isFormatting') return isFormatting(target);
			if (prop === 'isEmptyLine') return isEmptyLine(target);
			if (prop === 'isNextLine') return isNextLine(target);
			if (prop === 'isComment') return isComment(target);
			if (prop === 'isBlockComment') return target.type === 'block_comment';
			if (prop === 'isLineComment') return target.type === 'line_comment';

            if (prop === 'isControlFlow') {
                return [
                    'if_expression',
                    'while_expression',
                    'loop_expression',
                    'abort_expression',
                    'return_expression',
                ].includes(target.type);
            }

			if (prop === 'isList') {
				return [
					'vector_expression',
					'expression_list',
					'block',
				].includes(target.type);
			}

            if (prop === 'isBreakableExpression') {
                return [
					// TODO: consider revisiting `call_expression` and `macro_call_expression`
                    // 'call_expression',
                    // 'macro_call_expression',
                    'vector_expression',
                    'expression_list',
                    'if_expression',
                    'pack_expression',
                    'block',
                ];
            }

			// Returns all the named children of the node that are not formatting.
			if (prop === 'nonFormattingChildren') {
				return (target.namedChildren as FormattedNode[])
					.filter((e) => !isFormatting(e))
					.map((e) => newProxy(e));
			}

			if (prop === 'namedAndEmptyLineChildren') {
				return (target.namedChildren as FormattedNode[])
					.filter((e) => !isFormatting(e) || isEmptyLine(e))
					.map((e) => newProxy(e));
			}

			// Adding a `trailingComment` property to the `SyntaxNode`. When
			// `trailingComment` is accessed, it will return the first comment
			// that is after the node. It is important to note that a traling
			// comment must not have a new line before it!
			if (prop === 'trailingComment') {
				return node.nextSibling?.type === 'line_comment' ? node.nextSibling.text : null;
			}

			// Adding a `firstNamedChild` property to the `SyntaxNode`. When
			// `firstNamedChild` is accessed, it will return the first named
			// child of the node. If the node has no named children, it will
			// return `null`.
			//
			// Uses the modified `namedChildren` getter to filter out any
			// formatting nodes.
			if (prop === 'firstNamedChild') {
				return (target.namedChildren.length && target.namedChildren[0]) || null;
			}

			// Read the `children` property of the `SyntaxNode` and filter out
			// any formatting nodes.
			if (prop === 'children') {
				return (result as FormattedNode[])
					.filter((e) => !isNextLine(e))
					.map((e) => newProxy(e));
			}

			// Read the `namedChildren` property of the `SyntaxNode` and filter
			// out any formatting nodes.
			if (prop === 'namedChildren') {
				return (result as FormattedNode[])
					.filter((e) => !isNextLine(e))
					.map((e) => newProxy(e));
			}

			// very-very weakly typed check for a `SyntaxNode`
			if (typeof result === 'object' && result.namedChildren) {
				return newProxy(result);
			}

			return result;
		},
	}) as FormattedNode;
}
