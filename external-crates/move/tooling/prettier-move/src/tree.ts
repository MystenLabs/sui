// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import Parser = require('web-tree-sitter');
import { isFormatting } from './cst/formatting';
import { isUseImport } from './cst/use_declaration';

export interface Comment {
    type: 'line_comment' | 'block_comment';
    text: string;
    newline: boolean;
}

export class Tree {
    public type: string;
    public text: string;
    public isNamed: boolean;
    public children: Tree[];
    public leadingComment: Comment[];
    public trailingComment: Comment | null;
    public enableLeadingComment: boolean = true;
    public enableTrailingComment: boolean = true;

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

        // assign trailing comments to the node. modifies the tree in place.
        this.children.forEach((child) => child.assignTrailingComments());

        // assign leading comments to the node. modifies the tree in place.
        this.children.forEach((child) => child.assignLeadingComments());

        // filter out all leading comments.
        this.children = this.children.filter((child) => !child.isUsedComment);
    }

    /**
     * Disable the trailing comment for the current node.
     */
    disableTrailingComment() {
        this.enableTrailingComment = false;
    }

    /**
     * Special case for lists, where we want to print the leading character (eg `dot_expression`).
     */
    disableLeadingComment() {
        this.enableLeadingComment = false;
    }

    /**
     * Find the parent node of a specific type. Optionally, break on a specific type.
     */
    findParentUntil(type: string, breakOn: string[]): Tree | null {
        let parent = this.parent;
        while (parent) {
            if (parent.type === type) return parent;
            if (breakOn.includes(parent.type)) return null;
            parent = parent.parent;
        }

        return null;
    }

    /**
     * Check if the previous sibling is an annotation node. Ignore formatting nodes.
     */
    get hasAnnotation(): boolean {
        let prev = this.previousNamedSibling;
        while (prev) {
            if (prev.type === 'annotation') return true;
            if (!prev.isFormatting) return false;
            prev = prev.previousNamedSibling;
        }
        return false;
    }

    /**
     * A flag to skip formatting for a specific node. A manual instruction from
     * the user is `prettier-ignore`. When placed above (leading comment) a node,
     * it will skip formatting for that node.
     */
    get skipFormattingNode(): boolean {
        return (
            !!this.leadingComment.find((comment) => comment.text.includes('prettier-ignore')) ||
            false
        );
    }

    /**
     * Get the number of named children.
     */
    get namedChildCount(): number {
        return this.namedChildren.length;
    }

    get isFunctionCall(): boolean {
        return this.type === 'call_expression' || this.type === 'macro_call_expression';
    }

    /**
     * Tells whether a `Node` knows how to break itself.
     * Nodes that match the following types are considered breakable:
     * - `dot_expression`
     * - `vector_expression`
     * - `expression_list`
     * - `if_expression` (?)
     * - `pack_expression`
     * - `match_expression`
     * - `block`
     */
    get isBreakableExpression(): boolean {
        return [
            // TODO: consider revisiting `call_expression` and `macro_call_expression`
            // 'call_expression',
            // 'macro_call_expression',
            'dot_expression',
            'index_expression',
            'vector_expression',
            'expression_list',
            'if_expression',
            'pack_expression',
            'match_expression',
            'block',
        ].includes(this.type);
    }

    /**
     * Whether a node is a list node, like `vector_expression`, `expression_list`, or `block`.
     * Lists are typical breakable nodes, where each element is separated by a newline.
     */
    get isList(): boolean {
        return ['vector_expression', 'expression_list', 'block'].includes(this.type);
    }

    /**
     * Whether a node is a control flow node, like `if_expression`, `while_expression`,
     * `loop_expression`, `abort_expression`, or `return_expression`.
     */
    get isControlFlow(): boolean {
        return [
            'identified_expression',
            'if_expression',
            'while_expression',
            'loop_expression',
            'abort_expression',
            'return_expression',
        ].includes(this.type);
    }

    /**
     * Important part of the `imports-grouping` functionality. This flag is used to
     * determine whether a node is an `use_module`, `use_module_members` or
     * `use_module_member` node to skip their printing if they're printed as grouped.
     */
    get isGroupedImport(): boolean {
        return isUseImport(this) && !this.hasAnnotation;
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

    get isTypeParam(): boolean {
        return ['apply_type', 'ref_type', 'tuple_type', 'function_type', 'primitive_type'].includes(
            this.type,
        );
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
            leadingComment: this.leadingComment,
            trailingComment: this.trailingComment,
        };
    }

    /**
     * Checks the following node and assigns it as a trailing comment if it is a comment.
     * The comment is then marked as used and will not be used again.
     */
    private assignTrailingComments(): Tree {
        if (!this.isNamed) return this;
        if (this.isFormatting) return this;

        const nextNamed = this.nextNamedSibling;

        if (!nextNamed?.isComment) return this;
        if (nextNamed.isUsedComment) return this;

        const comment = nextNamed;

        // if it's a block comment, we need to make sure there's nothing in
        // between the current node and the comment, otherwise block comment is
        // associated with the next node.
        if (comment.type == 'block_comment') {
            // any non-named node between the current node and the comment
            // breaks the association.
            if (comment.previousSibling != this && !!comment.nextNamedSibling) return this;
            const addSpace = !!comment.nextNamedSibling ? ' ' : '';

            this.trailingComment = {
                type: comment.type as 'line_comment' | 'block_comment',
                text: comment.text + addSpace,
                newline: false,
            };
            this.nextNamedSibling!.isUsedComment = true;
        }

        if (comment.type == 'line_comment') {
            this.trailingComment = {
                type: comment.type as 'line_comment' | 'block_comment',
                text: comment.text,
                newline: false,
            };

            this.nextNamedSibling!.isUsedComment = true;
        }

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
        let newline = false;

        if (!this.isNamed) return this;
        if (this.isFormatting) return this;

        while (prev?.isNewline) {
            newline = true;
            prev = prev.previousNamedSibling;
        }

        if (prev?.type == 'block_comment') {
            if (prev.isUsedComment) return this;

            comments.unshift({
                type: prev.type as 'line_comment' | 'block_comment',
                text: prev.text,
                newline,
            });

            prev.isUsedComment = true;
            this.leadingComment = comments;
            return this;
        }

        while (prev?.isComment || (prev?.isNewline && !prev?.isUsedComment)) {
            if (prev.isUsedComment) break;
            if (prev.isComment) {
                comments.unshift({
                    type: prev.type as 'line_comment' | 'block_comment',
                    text: prev.text,
                    newline: true,
                });
                prev.isUsedComment = true;
            }

            prev = prev.previousNamedSibling; // move to the previous comment
        }

        this.leadingComment = comments;

        return this;
    }
}
