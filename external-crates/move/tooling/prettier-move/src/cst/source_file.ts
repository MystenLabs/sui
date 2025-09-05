// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '..';
import { MoveOptions, printFn, treeFn } from '../printer';
import { AstPath, Doc, doc } from 'prettier';
const { hardline, join } = doc.builders;

/**
 * Print a source node.
 */
export default function (path: AstPath<Node>): treeFn | null {
    switch (path.node.type) {
        case SourceFile.SourceFile:
            return printSourceFile;
    }
    return null;
}

export enum SourceFile {
    SourceFile = 'source_file',
}

/**
 * Print `source_file` node.
 *
 * Print all non-formatting children separated by a hardline.
 * Also print empty lines with leading comments, this allows us to maintain structure like this:
 * ```
 * // Copyright
 * `empty_line`
 * // module comment
 * module book::book { ... }
 * ```
 */
function printSourceFile(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    return [join(hardline, path.map(print, 'namedAndEmptyLineChildren')), hardline];
}
