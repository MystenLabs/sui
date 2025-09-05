// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '..';
import { MoveOptions, printFn, treeFn } from '../printer';
import { AstPath, Doc, ParserOptions, doc } from 'prettier';
import {
    emptyBlockOrList,
    list,
    printIdentifier,
    printLeadingComment,
    printTrailingComment,
    shouldBreakFirstChild,
} from '../utilities';
const { group, join } = doc.builders;

export default function (path: AstPath<Node>): treeFn | null {
    switch (path.node.type) {
        case StructDefinition.StructDefinition:
            return printStructDefinition;
        case StructDefinition.NativeStructDefinition:
            return printNativeStructDefinition;
        case StructDefinition.AbilityDeclarations:
            return printAbilityDeclarations;
        case StructDefinition.PostfixAbilityDeclarations:
            return printPostfixAbilityDeclarations;
        case StructDefinition.DatatypeFields:
            return printDatatypeFields;
        case StructDefinition.NamedFields:
            return printNamedFields;
        case StructDefinition.PositionalFields:
            return printPositionalFields;
        case StructDefinition.FieldAnnotation:
            return printFieldAnnotation;
        case StructDefinition.ApplyType:
            return printApplyType;
        case StructDefinition.StructIdentifier:
            return printIdentifier;
    }

    return null;
}

export enum StructDefinition {
    /**
     * Module-level definition
     * ```
     * public struct identifier ...
     * ```
     */
    StructDefinition = 'struct_definition',
    /**
     * Module-level definition (features `native` keyword and has no fields)
     * ```
     * native struct identifier ... ;
     * ```
     */
    NativeStructDefinition = 'native_struct_definition',
    AbilityDeclarations = 'ability_decls',
    /**
     * Postfix ability declarations must be printed after the fields
     * and be followed by a semicolon.
     * ```
     * struct ident {} has store;
     * struct Point(u8) has store, drop;
     * ```
     */
    PostfixAbilityDeclarations = 'postfix_ability_decls',
    DatatypeFields = 'datatype_fields',
    NamedFields = 'named_fields',
    PositionalFields = 'positional_fields',
    FieldAnnotation = 'field_annotation',
    ApplyType = 'apply_type',
    StructIdentifier = 'struct_identifier',
}

/**
 * Print `struct_definition` node.
 */
export function printNativeStructDefinition(
    path: AstPath<Node>,
    options: ParserOptions,
    print: printFn,
): Doc {
    const isPublic = path.node.child(0)!.type === 'public' ? ['public', ' '] : [];
    return group([
        ...isPublic, // insert `public` keyword if present
        'native',
        ' ',
        'struct',
        ' ',
        path.map(print, 'nonFormattingChildren'),
        ';',
    ]);
}

/**
 * Print `struct_definition` node.
 * Insert a newline before the comment if the previous node is not a line comment.
 */
export function printStructDefinition(
    path: AstPath<Node>,
    options: ParserOptions,
    print: printFn,
): Doc {
    const isPublic = path.node.child(0)!.type === 'public' ? ['public', ' '] : [];
    return group([
        ...isPublic, // insert `public` keyword if present
        'struct',
        ' ',
        path.map(print, 'nonFormattingChildren'),
    ]);
}

type Ability = { name: 'key' | 'store' | 'drop'; text: Doc };

/**
 * Print `ability_decls` node.
 */
export function printAbilityDeclarations(
    path: AstPath<Node>,
    options: MoveOptions,
    print: printFn,
): Doc {
    const abilities = formatAndSortAbilities(path, options, print);
    return [
        ' has ',
        join(
            ', ',
            abilities.map((ability) => ability.text),
        ),
        path.next?.namedChildren[0]?.type === StructDefinition.PositionalFields ? ' ' : '',
    ];
}

/**
 * Print `postfix_ability_decls` node.
 */
export function printPostfixAbilityDeclarations(
    path: AstPath<Node>,
    options: MoveOptions,
    print: printFn,
): Doc {
    const abilities = formatAndSortAbilities(path, options, print);
    return group([
        ' has ',
        join(
            ', ',
            abilities.map((ability) => ability.text),
        ),
        ';',
    ]);
}

/**
 * Print `datatype_fields` node.
 * Prints the underlying fields of a datatype.
 */
export function printDatatypeFields(
    path: AstPath<Node>,
    options: ParserOptions,
    print: printFn,
): Doc {
    return path.map(print, 'nonFormattingChildren');
}

/**
 * Print `named_fields` node.
 * Prints the underlying fields of a struct.
 */
export function printNamedFields(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    const children = path.map(print, 'nonFormattingChildren');

    if (children.length === 0) {
        return [' ', emptyBlockOrList(path, '{', '}', doc.builders.line)];
    }

    return [
        ' ',
        group(list({ path, print, options, open: '{', close: '}', addWhitespace: true }), {
            shouldBreak: shouldBreakFirstChild(path),
        }),
    ];
}

/**
 * Print `positional_fields` node.
 * Prints the underlying fields of a struct.
 */
export function printPositionalFields(
    path: AstPath<Node>,
    options: MoveOptions,
    print: printFn,
): Doc {
    const children = path.map(print, 'nonFormattingChildren');

    if (children.length === 0) {
        return emptyBlockOrList(path, '(', ')', doc.builders.line);
    }

    return group(list({ path, print, options, open: '(', close: ')' }), {
        shouldBreak: false,
    });
}

/**
 * Print `field_annotation` node.
 */
export function printFieldAnnotation(
    path: AstPath<Node>,
    options: ParserOptions,
    print: printFn,
): Doc {
    return group([
        path.call(print, 'nonFormattingChildren', 0), // field_identifier
        ':',
        ' ',
        path.call(print, 'nonFormattingChildren', 1), // type
    ]);
}

/**
 * Print `apply_type` node.
 */
export function printApplyType(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
    return path.map(print, 'nonFormattingChildren');
}

/**
 * Utility function which formats and sorts abilities in the following order::
 *
 * - key
 * - copy
 * - drop
 * - store
 *
 * Key always goes first, the rest are sorted alphabetically.
 */
function formatAndSortAbilities(
    path: AstPath<Node>,
    options: MoveOptions,
    print: printFn,
): Ability[] {
    const abilities: Ability[] = path.map(
        (path) => ({
            name: path.node.text as Ability['name'],
            text: [
                printLeadingComment(path, options),
                path.node.text,
                printTrailingComment(path, true),
            ] as Doc,
        }),
        'nonFormattingChildren',
    );

    // alphabetical but `key` always goes first
    const priority = {
        key: 0,
        copy: 1,
        drop: 2,
        store: 3,
    };

    abilities.sort((a, b) => priority[a.name] - priority[b.name]);

    return abilities;
}
