import Parser = require('web-tree-sitter');
import { isEmptyLine, isFormatting, isNewline } from './cst/Formatting';

export class Tree {
	public type: string;
	public text: string;
	public isNamed: boolean;
	public children: Tree[];
	public leadingComment: string[];
	public trailingComment: string | null;

	/**
	 * A reference lock to the parent node. This is a function that returns the
	 * parent node. This way we remove the duplicate reference to the parent node
	 * and avoid circular references.
	 */
	private getParent: () => Tree | null;

	/**
	 * Marks if the comment has been used. This is useful to avoid using the same
	 * comment multiple times + filter out comments that are already used.
	 */
	private isUsedComment: boolean = false;

	/**
	 * Construct the `Tree` node from the `Parser.SyntaxNode`, additionally, run
	 * some passes to clean-up the tree and make the structure more manageable and
	 * easier to work with.
	 *
	 * Passes:
	 * - Sum-up pairs of newlines into a single empty line.
	 * - Filter out sequential empty lines.
	 * - Filter out leading and trailing empty lines.
	 * - Assign trailing comments to the node.
	 * - Assign leading comments to the node.
	 * - Filter out all assigned comments.
	 *
	 * @param node
	 * @param parent
	 */
	constructor(node: Parser.SyntaxNode, parent: Tree | null = null) {
		this.type = node.type;
		this.text = node.text;
		this.isNamed = node.isNamed();
		this.leadingComment = [];
		this.trailingComment = null;
		this.getParent = () => parent;

		// === Clean-up passes ===

		// turn every node into a `Tree` node.
		this.children = node.children.map((child) => new Tree(child, this));

		// sum-up pairs of newlines into a single empty line.
		this.children = this.children.reduce((acc, node) => {
			if (node.isNewline && node.nextSibling?.isNewline) node.type = 'empty_line';
			if (node.isNewline && acc[acc.length - 1]?.isEmptyLine) return acc;
			return [...acc, node];
		}, [] as Tree[]);

		// filter out sequential empty lines.
		this.children = this.children.filter((node) => {
			return !node.isEmptyLine || !node.previousNamedSibling?.isEmptyLine;
		});

		// filter out leading and trailing empty lines.
		this.children = this.children.filter((node) => {
			if (!node.isEmptyLine) return true; // we only filter out empty lines
			if (!node.previousNamedSibling) return false; // remove leading empty lines
			if (!node.nextNamedSibling) return false; // remove trailing empty lines
			return true;
		});

		// assign leading comments to the node. modifies the tree in place.
		this.children.forEach((child) => child.assignLeadingComments());

		// assign trailing comments to the node. modifies the tree in place.
		this.children.forEach((child) => child.assignTrailingComments());

		// filter out all leading comments.
		this.children = this.children.filter((child) => !child.isUsedComment);
	}

	get namedChildCount(): number {
		return this.namedChildren.length;
	}

	get isBreakableExpression(): boolean {
		return [
			// TODO: consider revisiting `call_expression` and `macro_call_expression`
			// 'call_expression',
			// 'macro_call_expression',
			'dot_expression',
			'vector_expression',
			'expression_list',
			'if_expression',
			'pack_expression',
			'block',
		].includes(this.type);
	}

	get isList(): boolean {
		return ['vector_expression', 'expression_list', 'block'].includes(this.type);
	}

	get isControlFlow(): boolean {
		return [
			'if_expression',
			'while_expression',
			'loop_expression',
			'abort_expression',
			'return_expression',
		].includes(this.type);
	}

	/**
	 * Whether a node is a `Formatting` node, like `line_comment`, `block_comment`,
	 * `empty_line`, or `next_line`.
	 */
	get isFormatting(): boolean {
		return isFormatting(this);
	}

	child(index: number): Tree | null {
		return this.children[index] || null;
	}

	get isEmptyLine(): boolean {
		return this.type === 'empty_line';
	}

	get isNewline(): boolean {
		return this.type === 'newline';
	}

	get isComment(): boolean {
		return this.type === 'line_comment' || this.type === 'block_comment';
	}

	get previousSibling(): Tree | null {
		const parent = this.getParent();
		if (!parent) {
			return null;
		}

		const index = parent.children.indexOf(this);
		if (index === 0) {
			return null;
		}

		return parent.children[index - 1] || null;
	}

	get previousNamedSibling(): Tree | null {
		let node = this.previousSibling;
		while (node && !node.isNamed) {
			node = node.previousSibling;
		}
		return node;
	}

	get startsOnNewLine(): boolean {
		return this.previousSibling?.isNewline || false;
	}

	get nonFormattingChildren(): Tree[] {
		return this.namedChildren.filter((child) => !child.isFormatting);
	}

	get namedChildren(): Tree[] {
		return this.children.filter((child) => child.isNamed);
	}

	get firstNamedChild(): Tree | null {
		return this.namedChildren[0] || null;
	}

	get namedAndEmptyLineChildren(): Tree[] {
		return this.namedChildren.filter((child) => {
			return (
				child.isNamed &&
				(child.isEmptyLine ||
					(child.isComment && !child.isUsedComment) ||
					!child.isFormatting)
			);
		});
	}

	get nextSibling(): Tree | null {
		const parent = this.getParent();
		if (!parent) {
			return null;
		}

		const index = parent.children.indexOf(this);
		if (index === parent.children.length - 1) {
			return null;
		}

		return parent.children[index + 1] || null;
	}

	get nextNamedSibling(): Tree | null {
		let node = this.nextSibling;
		while (node && !node.isNamed) {
			node = node.nextSibling;
		}
		return node;
	}

	get parent() {
		return this.getParent();
	}

	/**
	 * Print the Node as a JSON object. Remove the fields that are not necessary
	 * for printing. May be extended shall one need to debug deeper.
	 */
	toJSON(): any {
		return {
			type: this.type,
			isNamed: this.isNamed,
			children: this.children.map((child) => child.toJSON()),
		};
	}

	/**
	 * Checks the following node and assigns it as a trailing comment if it is a comment.
	 * The comment is then marked as used and will not be used again.
	 */
	private assignTrailingComments(): Tree {
		if (!this.isNamed) return this;
		if (this.isFormatting) return this;
		if (!this.nextNamedSibling?.isComment) return this;
		if (this.nextNamedSibling.isUsedComment) return this;

		this.trailingComment = this.nextNamedSibling.text;
		this.nextNamedSibling.isUsedComment = true;

		return this;
	}

	/**
	 * Walks backwards through the siblings and searches for comments preceding
	 * the current node. If a comment is found, it is assigned to the `leadingComment`
	 * property of the node, and the comment is marked as used.
	 *
	 * Used comments are filtered out and not used again.
	 *
	 * Motivation for this is to avoid duplicate association of a comment both as
	 * a trailing comment and a leading comment.
	 */
	private assignLeadingComments(): Tree {
		let comments = [];
		let prev = this.previousNamedSibling;

		if (!this.isNamed) return this;
		if (this.isFormatting) return this;
		if (!prev?.isNewline) return this;

		prev = prev.previousNamedSibling;

		while (prev?.isComment || (prev?.isNewline && !prev?.isUsedComment)) {
			if (prev.isUsedComment) break;
			if (prev.isComment) {
				comments.unshift(prev.text);
				prev.isUsedComment = true;
			}

			prev = prev.previousNamedSibling; // move to the next comment
		}

		// promote trailing comments to leading comments
		// TODO: once we have a better comment linking mechanism, we can remove this
		// otherwise trailing line comments break lists and other formatting
		if (this.nextNamedSibling?.type === 'line_comment') {
			comments.push(this.nextNamedSibling.text);
			this.nextNamedSibling.isUsedComment = true;
		}

		this.leadingComment = comments;

		return this;
	}
}
