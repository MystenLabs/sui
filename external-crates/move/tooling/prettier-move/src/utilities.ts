// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '.';
import { AstPath, Doc, ParserOptions, doc } from 'prettier';
import { MoveOptions, printFn } from './printer';

const {
    indent,
    join,
    fill,
    softline,
    dedent,
    hardline,
    line,
    lineSuffix,
    group,
    indentIfBreak,
    hardlineWithoutBreakParent,
    breakParent,
    ifBreak,
} = doc.builders;

/**
 * Prints an `identifier` node.
 */
export function printIdentifier(path: AstPath<Node>): Doc {
    return path.node.text;
}

/**
 * Returns `true` if the first non-formatting child of the path starts on a new line.
 * This function is useful for respecting developer formatting if they choose to break
 * the list.
 *
 * ```move
 * // input
 * fun args(a: u8) {} // no break
 * fun args(
 *   a: u8 // first child starts on a new line
 * ) {}
 *
 * // output
 * fun args(a: u8) {} // no break
 * fun args(
 *  a: u8 // respect developer formatting
 * ) {}
 * ```
 *
 * @param path
 * @returns
 */
export function shouldBreakFirstChild(path: AstPath<Node>): boolean {
    return path.node.nonFormattingChildren[0]?.startsOnNewLine || false;
}

/**
 * Prints all comments that are leading the node. This function is injected into
 * the `printFn` to print comments before the node. See the `print` function in
 * `printer.ts` for more information.
 *
 * @param path
 * @returns
 */
export function printLeadingComment(path: AstPath<Node>, options: MoveOptions): Doc[] {
    const comments = path.node.leadingComment;
    if (!comments || !comments.length) return [];
    if (!path.node.enableLeadingComment) return [];

    if (comments.length == 1 && comments[0]!.type == 'block_comment') {
        return [comments[0]!.text, comments[0]!.newline ? hardlineWithoutBreakParent : ' '];
    }

    if (options.wrapComments == false) {
        // each comment's `newline` flag says whether a line break separated it
        // from what follows — inline block comments keep their spacing
        return comments.map((c) => [c.text, c.newline ? hardlineWithoutBreakParent : (' ' as Doc)]);
    }

    // we do not concatenate the comments into a single string, and treat each
    // line separately.
    return comments.map((comment) => {
        if (comment.type == 'line_comment') {
            const prefix = comment.text.startsWith('///') ? '///' : '//';
            const parts = comment.text.slice(prefix.length).trimStart().split(' ');

            return [
                prefix + ' ',
                fill(join(ifBreak([softline, prefix + ' '], ' '), parts)),
                hardlineWithoutBreakParent,
            ];
        }

        return [comment.text, hardlineWithoutBreakParent];
    });
}

/**
 * Prints the trailing comments of the node. Currently, we only allow a single line
 * comment to be printed. This function is injected into the `printFn` to print
 * comments after the node. See the `print` function in `printer.ts` for more information.
 *
 * @param path
 * @returns
 */
export function printTrailingComment(path: AstPath<Node>, shouldBreak: boolean = false): Doc {
    // we do not allow comments on empty lines
    if (path.node.isEmptyLine) return '';
    if (!path.node.enableTrailingComment) return '';
    const comment = path.node.trailingComment;
    if (!comment) return '';
    if (comment.type == 'line_comment') {
        if (shouldBreak) {
            return [' ', comment.text, hardline];
        }

        // a line comment printed inline would swallow any tokens the parent
        // prints after it on the same line; defer it to the end of the line
        // and break the enclosing group so the comment stays near its node
        return [lineSuffix([' ', comment.text]), breakParent];
    }

    return [' ', comment.text];
}

/**
 * The raw inline form of the trailing comment (`' ' + text`), with no line
 * break protection. Callers own the placement and must guarantee that a line
 * break follows before any sibling token is printed on the same line —
 * typically by wrapping the result in `lineSuffix` themselves.
 */
export function inlineTrailingComment(path: AstPath<Node>): Doc {
    if (path.node.isEmptyLine) return '';
    if (!path.node.enableTrailingComment) return '';
    const comment = path.node.trailingComment;
    if (!comment) return '';

    return [' ', comment.text];
}

export function emptyBlockOrList(
    path: AstPath<Node>,
    open: string,
    close: string,
    line: Doc = hardline,
): Doc {
    const length = path.node.nonFormattingChildren.length;
    const comments = path.node.namedChildren.filter((e) => e.isComment);

    if (length != 0) {
        throw new Error('The list is not empty');
    }

    if (comments.length == 0) {
        return [open, close];
    }

    if (comments.length == 1 && comments[0]!.type == 'block_comment') {
        return group([open, indent(line), indent(comments[0]!.text), line, close]);
    }

    return group(
        [
            open,
            indent(line),
            indent(
                join(
                    line,
                    comments.map((c) => c.text),
                ),
            ),
            line,
            close,
        ],
        { shouldBreak: true },
    );
}

export type BlockOptions = {
    path: AstPath<Node>;
    print: printFn;
    options: ParserOptions;
    skipChildren?: number;
    shouldBreak?: boolean;
};

/**
 */
export function block({ path, print, options, shouldBreak, skipChildren }: BlockOptions) {
    const length = path.node.nonFormattingChildren.length;

    if (length == 0) {
        return emptyBlockOrList(path, '{', '}', hardline);
    }

    return group(
        [
            '{',
            options.bracketSpacing ? ifBreak('', ' ') : '',
            indent(softline),
            indent(join(line, path.map(print, 'namedAndEmptyLineChildren').slice(skipChildren))),
            softline,
            options.bracketSpacing ? ifBreak('', ' ') : '',
            '}',
        ],
        { shouldBreak },
    );
}

export function nonBreakingBlock({
    path,
    print,
    options,
    shouldBreak, // always breaks
    skipChildren,
}: BlockOptions) {
    const length = path.node.nonFormattingChildren.length;

    if (length == 0) {
        return emptyBlockOrList(path, '{', '}', hardlineWithoutBreakParent);
    }

    return group([
        '{',
        indent(hardlineWithoutBreakParent),
        indent(
            join(
                hardlineWithoutBreakParent,
                path.map(print, 'namedAndEmptyLineChildren').slice(skipChildren || 0),
            ),
        ),
        hardlineWithoutBreakParent,
        '}',
    ]);
}

export type ListOptions = {
    path: AstPath<Node>;
    print: printFn;
    options: MoveOptions;
    /** Opening bracket. */
    open: string;
    /** Closing bracket. */
    close: string;
    /**
     * The number of children to skip when printing the list.
     */
    skipChildren?: number;
    /**
     * Whether to add a whitespace after the open bracket and before the close bracket.
     * ```
     * { a, b, c } // addWhitespace = true
     * {a, b, c}   // addWhitespace = false
     * ```
     */
    addWhitespace?: boolean;
    /**
     * Whether to break the list.
     */
    shouldBreak?: boolean;
    /**
     * Group ID for `indentIfBreak` to break the list.
     */
    indentGroup?: symbol | null;
};

/**
 * Prints a list of non-formatting children. Handles commas and trailing comments.
 * TODO: keep trailing comments after the last element of the list.
 */
export function list({
    path,
    print,
    options,
    open,
    close,
    indentGroup = null,
    addWhitespace = false,
    skipChildren = 0,
    shouldBreak = false,
}: ListOptions) {
    const length = path.node.nonFormattingChildren.length;
    const indentCb: (el: Doc) => Doc = (el) =>
        indentGroup ? indentIfBreak(el, { groupId: indentGroup }) : indent(el);

    // if there's no children the list should print, we still look up for non-
    // formatting nodes, namely comments, to print them.
    if (length == skipChildren) {
        const lastNode = path.node.nonFormattingChildren[length - 1]!;
        const indexInNamedChildren = path.node.namedChildren.indexOf(lastNode);
        const otherNamedChildren = path.node.namedChildren
            .slice(indexInNamedChildren + 1)
            .filter((e) => e.isComment);

        if (!otherNamedChildren.length) {
            return [open, close];
        }

        return [
            open,
            indentCb(softline),
            indentCb(
                join(
                    hardline,
                    otherNamedChildren.map((c) => c.text),
                ),
            ),
            hardline,
            dedent(close),
        ];
    }

    const lastNode = path.node.nonFormattingChildren[length - 1]!;
    const indexInNamedChildren = path.node.namedChildren.indexOf(lastNode);

    // collect all trailing comments
    // after `nonFormattingChildren` and before end
    let trailingComments = [] as Doc[];
    if (indexInNamedChildren != -1) {
        path.each((path, idx) => {
            if (idx + 1 > indexInNamedChildren && path.node.isComment) {
                return trailingComments.push(path.node.text);
            }
            return;
        }, 'namedChildren');
    }

    // comments that were not claimed as leading/trailing by any element (e.g.
    // separated from the next element by an empty line) are still children of
    // the list node; collect them per-element so they are not dropped
    const danglingBefore: string[][] = path.node.nonFormattingChildren.map(() => []);
    {
        let elementIndex = 0;
        for (const child of path.node.namedChildren) {
            if (child === path.node.nonFormattingChildren[elementIndex]) {
                elementIndex++;
            } else if (elementIndex < length && child.isComment) {
                danglingBefore[elementIndex]!.push(child.text);
            }
        }
    }

    return [
        open,
        indentCb(addWhitespace ? line : softline),
        shouldBreak ? breakParent : '',
        indentCb(
            path
                .map((path, i) => {
                    const dangling = danglingBefore[i]!.map((text) => [text, hardline] as Doc);
                    const leading = printLeadingComment(path, options);
                    // the list provides its own separators after the comment,
                    // so the padding space baked into block-comment text (see
                    // `assignTrailingComments`) is stripped here
                    const trailingBlock =
                        path.node.enableTrailingComment &&
                        path.node.trailingComment?.type == 'block_comment'
                            ? ([' ', path.node.trailingComment.text.trimEnd()] as Doc)
                            : null;
                    const comment = trailingBlock ?? printTrailingComment(path, false);
                    let shouldBreak = false;

                    // if the node has a trailing comment, we should break
                    if (path.node.trailingComment?.type == 'line_comment') {
                        shouldBreak = true;
                    }

                    const leadComment = path.node.leadingComment;

                    if (leadComment.length > 0 && leadComment![0]!.type == 'line_comment') {
                        shouldBreak = true;
                    }

                    if (
                        leadComment.length > 0 &&
                        leadComment[0]!.type == 'block_comment' &&
                        leadComment[0]!.newline
                    ) {
                        shouldBreak = true;
                    }

                    path.node.disableTrailingComment();
                    path.node.disableLeadingComment();

                    const breakExpr = shouldBreak ? breakParent : '';
                    const shouldDedent = trailingComments.length == 0;
                    const endingExpr = addWhitespace ? line : softline;
                    const isLastChild = i == length - 1;

                    // trailing block comments print inline right after the
                    // element, BEFORE the comma, preserving the source shape
                    // (`x /* c */,`); trailing line comments are deferred to
                    // the end of the line by `lineSuffix`, so they still land
                    // after the comma
                    if (isLastChild) {
                        return [
                            dangling,
                            leading,
                            breakExpr,
                            print(path),
                            comment,
                            ifBreak(','),
                            shouldDedent ? dedent(endingExpr) : endingExpr,
                        ];
                    }

                    // if we are not at the last child, add a comma
                    return [dangling, leading, breakExpr, print(path), comment, ',', line];
                }, 'nonFormattingChildren')
                .slice(skipChildren)
                .concat(
                    trailingComments.length
                        ? [join(hardline, trailingComments), dedent(hardline)]
                        : [],
                ),
        ),
        dedent(close),
    ];
}
