// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '..';
import { MoveOptions, printFn, treeFn } from '../printer';
import { AstPath, doc, Doc } from 'prettier';
import { list } from '../utilities';
const { group, join } = doc.builders;

export enum Annotation {
    Annotation = 'annotation',
    AnnotationItem = 'annotation_item',
    AnnotationList = 'annotation_list',
    AnnotationExpr = 'annotation_expr',
}

export default function (path: AstPath<Node>): treeFn | null {
    switch (path.node.type) {
        case Annotation.Annotation:
            return printAnnotation;
        case Annotation.AnnotationItem:
            return printAnnotationItem;
        case Annotation.AnnotationList:
            return printAnnotationList;
        case Annotation.AnnotationExpr:
            return printAnnotationExpr;
    }

    return null;
}

/**
 * Print `annotation` node.
 */
export function printAnnotation(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    return group(['#', list({ path, print, options, open: '[', close: ']' })]);
}

/**
 * Print `annotation_item` node.
 */
export function printAnnotationItem(path: AstPath<Node>, _opt: MoveOptions, print: printFn): Doc {
    return path.map(print, 'nonFormattingChildren');
}

export function printAnnotationList(
    path: AstPath<Node>,
    options: MoveOptions,
    print: printFn,
): Doc {
    return [
        path.call(print, 'nonFormattingChildren', 0),
        list({ path, print, options, open: '(', close: ')', skipChildren: 1 }),
    ];
}

/**
 * Print `annotation_expr` node.
 */
export function printAnnotationExpr(path: AstPath<Node>, _opt: MoveOptions, print: printFn): Doc {
    // allow `::module::Expression` in annotations
    return join(
        ' = ',
        path.map((path) => {
            if (path.node.type === 'module_access' && path.node.previousSibling?.type == '::') {
                return ['::', path.call(print)];
            }
            return path.call(print);
        }, 'nonFormattingChildren'),
    );
}
