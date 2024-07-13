import Parser = require('web-tree-sitter');
import { isEmptyLine, isFormatting, isNextLine } from './cst/Formatting';

export class Tree {
	public type: string;
	public text: string;
	public isNamed: boolean;
	public children: Tree[];

	/**
	 * A reference lock to the parent node. This is a function that returns the
	 * parent node. This way we remove the duplicate reference to the parent node
	 * and avoid circular references.
	 */
	private getParent: () => Tree | null;

	constructor(node: Parser.SyntaxNode, parent: Tree | null = null) {
		this.type = node.type;
		this.text = node.text;
		this.isNamed = node.isNamed();
		this.getParent = () => parent;
		this.children = node.children.map((child) => new Tree(child, this));
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

	get isNextLine(): boolean {
		return this.type === 'next_line';
	}

	child(index: number): Tree | null {
		return this.children[index] || null;
	}

	get leadingComment(): string[] {
		const comments: string[] = [];
		let node: Tree = this;

		// leading comment must have a newline character before it
		// except for the case of an empty line (it's a special case)
		if (
			!node.isEmptyLine &&
			(!node.previousSibling?.isNextLine || !node.previousSibling.previousSibling?.isComment)
		) {
			return comments;
		}

		while (
			(node.previousSibling && node.previousSibling.isComment) ||
			isNextLine(node.previousSibling)
		) {
			node = node.previousSibling!;
			if (node.isComment) comments.unshift(node.text);
		}

		// Leading comment is a comment that either is the first node,
		// or starts with a newline character, or the previous node is
		// an empty line node.
		return isNextLine(node) || isEmptyLine(node.previousSibling) || !node.previousSibling
			? comments
			: [];
	}

	get trailingComment(): string | null {
        // Adding a `trailingComment` property to the `SyntaxNode`. When
        // `trailingComment` is accessed, it will return the first comment
        // that is after the node. It is important to note that a traling
        // comment must not have a new line before it!
        return this.nextSibling?.type === 'line_comment' ? this.nextSibling.text : null;
	}

	get isEmptyLine(): boolean {
		return this.type === 'empty_line';
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

	get startsOnNewLine(): boolean {
		return this.previousSibling?.isNextLine || false;
	}

	get nonFormattingChildren(): Tree[] {
		return this.namedChildren.filter((e) => !isFormatting(e));
	}

	get namedChildren(): Tree[] {
		return this.children.filter((child) => child.isNamed);
	}

	get firstNamedChild(): Tree | null {
		return this.namedChildren[0] || null;
	}

	get namedAndEmptyLineChildren(): Tree[] {
		return this.namedChildren.filter((e) => !isFormatting(e) || e.isEmptyLine);
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
}
