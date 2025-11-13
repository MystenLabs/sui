// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { AstPath, Doc, doc } from 'prettier';
import { Node } from '..';
import { MoveOptions, printFn, treeFn } from '../printer';
import { list, printIdentifier, printTrailingComment } from '../utilities';
const { join, lineSuffix, group } = doc.builders;

export default function (path: AstPath<Node>): treeFn | null {
    switch (path.node.type) {
        case EnumDefinition.EnumDefinition:
            return printEnumDefinition;
        case EnumDefinition.EnumVariants:
            return printEnumVariants;
        case EnumDefinition.Variant:
            return printVariant;

        // identifiers
        case EnumDefinition.EnumIdentifier:
        case EnumDefinition.VariantIdentifier:
            return printIdentifier;
    }

    return null;
}

export enum EnumDefinition {
    EnumDefinition = 'enum_definition',
    EnumVariants = 'enum_variants',
    Variant = 'variant',

    EnumIdentifier = 'enum_identifier',
    VariantIdentifier = 'variant_identifier',
}

/**
 * Print `enum_definition` node.
 */
export function printEnumDefinition(path: AstPath<Node>, _opt: MoveOptions, print: printFn): Doc {
    const isPublic = path.node.child(0)?.type == 'public';

    return [isPublic ? 'public ' : '', 'enum ', path.map(print, 'nonFormattingChildren')];
}

/**
 * Print `enum_variants` node.
 * List of `variant` nodes.
 */
export function printEnumVariants(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    if (path.node.nonFormattingChildren.length === 0) {
        return ' {}';
    }

    return [
        ' ',
        group(list({ path, print, options, open: '{', close: '}', addWhitespace: true }), {
            shouldBreak: true,
        }),
    ];
}

/**
 * Print `variant` node.
 * Inside:
 * - `variant_identifier`
 * - `datatype_fields` (optional)
 */
export function printVariant(path: AstPath<Node>, _opt: MoveOptions, print: printFn): Doc {
    const trailing = lineSuffix(printTrailingComment(path, false));
    path.node.disableTrailingComment();
    return [path.map(print, 'nonFormattingChildren'), trailing];
}
