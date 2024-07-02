// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { AstPath, Doc, ParserOptions, doc } from "prettier";
import { Node } from "..";
import { printFn, treeFn } from "../printer";
const { join, indent, hardline, line, ifBreak } = doc.builders;


export default function (path: AstPath<Node>): treeFn | null {
    switch (path.node.type) {
        case EnumDefinition.EnumDefinition:
            return printEnumDefinition;
        case EnumDefinition.EnumIdentifier:
            return printEnumIdentifier;
        case EnumDefinition.EnumVariants:
            return printEnumVariants;
        case EnumDefinition.Variant:
            return printVariant;
        case EnumDefinition.VariantIdentifier:
            return printVariantIdentifier;
    }

    return null;
}

export enum EnumDefinition {
    EnumDefinition = 'enum_definition',
    EnumIdentifier = 'enum_identifier',
    EnumVariants = 'enum_variants',
    Variant = 'variant',
    VariantIdentifier = 'variant_identifier',
}

/**
 * Print `enum_definition` node.
 */
export function printEnumDefinition(
	path: AstPath<Node>,
	options: ParserOptions,
	print: printFn,
): Doc {
    const isPublic = path.node.child(0)?.type == 'public';

	return [
        isPublic ? 'public ' : '',
        'enum ',
        path.map(print, 'nonFormattingChildren'),
    ];
}

/**
 * Print `enum_identifier` node.
 */
export function printEnumIdentifier(
    path: AstPath<Node>,
    options: ParserOptions,
    print: printFn,
): Doc {
    return path.node.text;
}

/**
 * Print `enum_variants` node.
 */
export function printEnumVariants(
    path: AstPath<Node>,
    options: ParserOptions,
    print: printFn,
): Doc {
    const children = path.map(print, 'nonFormattingChildren');

    return [
        ' {',
        indent(hardline),
        indent(join([',', line], children)),
        ifBreak(','),
        hardline,
        '}',
    ];
}

/**
 * Print `variant` node.
 */
export function printVariant(
    path: AstPath<Node>,
    options: ParserOptions,
    print: printFn,
): Doc {
    return path.map(print, 'nonFormattingChildren');
}
/**
 * Prints `variant_identifier` node
 */
export function printVariantIdentifier(
    path: AstPath<Node>,
    options: ParserOptions,
    print: printFn,
): Doc {
    return path.node.text;
}
