// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '..';
import { MoveOptions, printFn, treeFn } from '../printer';
import { AstPath, doc, Doc } from 'prettier';
import { list, printIdentifier, shouldBreakFirstChild } from '../utilities';
const { group, join, line, indent, hardline } = doc.builders;

/**
 * Creates a callback function to print common nodes.
 */
export default function (path: AstPath<Node>): treeFn | null {
    switch (path.node.type) {
        case Common.PrimitiveType:
            return printPrimitiveType;
        case Common.ModuleAccess:
            return printModuleAccess;

        // identifiers
        case Common.Identifier:
        case Common.FieldIdentifier:
        case Common.VariableIdentifier:
            return printIdentifier;

        case Common.RefType:
            return printRefType;
        case Common.FunctionType:
            return printFunctionType;
        case Common.FunctionTypeParameters:
            return printFunctionTypeParameters;

        case Common.Ability:
            return printAbility;

        case Common.TupleType:
            return printTupleType;

        // === Bindings ===

        case Common.BindUnpack:
            return printBindUnpack;
        case Common.BindFields:
            return printBindFields;
        case Common.MutBindField:
            return printMutBindField;
        case Common.BindField:
            return printBindField;
        case Common.BindList:
            return printBindList;
        case Common.CommaBindList:
            return printCommaBindList;
        case Common.OrBindList:
            return printOrBindList;
        case Common.AtBind:
            return printAtBind;
        case Common.BindNamedFields:
            return printBindNamedFields;
        case Common.BindPositionalFields:
            return printBindPositionalFields;
        case Common.BindVar:
            return printBindVar;
        case Common.MutBindVar:
            return printMutBindVar;
        case Common.ImmRef:
            return printImmRef;
        case Common.MutRef:
            return printMutRef;

        case Common.Label:
            return printLabel;
        case Common.Alias:
            return printAlias;
        case Common.BlockIdentifier:
            return printBlockIdentifier;
        case Common.UnaryOperator:
            return printUnaryOperator;
        case Common.FieldInitializeList:
            return printFieldInitializeList;
        case Common.ExpressionField:
            return printExpressionField;
        case Common.ArgList:
            return printArgList;
    }

    return null;
}

/**
 * Nodes which are used across multiple files, yet can't be categorized.
 */
export enum Common {
    PrimitiveType = 'primitive_type',
    VariableIdentifier = 'variable_identifier',
    ModuleAccess = 'module_access',
    Identifier = 'identifier',
    RefType = 'ref_type',
    FunctionType = 'function_type',
    FunctionTypeParameters = 'function_type_parameters',
    FieldIdentifier = 'field_identifier',
    BlockIdentifier = 'block_identifier',

    Ability = 'ability',
    TupleType = 'tuple_type',

    // === Bindings ===

    BindUnpack = 'bind_unpack',
    BindFields = 'bind_fields',
    MutBindField = 'mut_bind_field',
    BindField = 'bind_field',
    BindList = 'bind_list',
    BindNamedFields = 'bind_named_fields',
    CommaBindList = 'comma_bind_list',
    OrBindList = 'or_bind_list',
    AtBind = 'at_bind',
    BindPositionalFields = 'bind_positional_fields',
    BindVar = 'bind_var',
    MutBindVar = 'mut_bind_var',
    ImmRef = 'imm_ref',
    MutRef = 'mut_ref',

    Label = 'label',
    Alias = 'alias',
    UnaryOperator = 'unary_op',
    FieldInitializeList = 'field_initialize_list',
    ExpressionField = 'exp_field',

    // used in `call_expression` and `macro_call_expression`
    ArgList = 'arg_list',
}

/**
 * Print `primitive_type` node.
 */
export function printPrimitiveType(path: AstPath<Node>, _opt: MoveOptions, _p: printFn): Doc {
    return path.node.text;
}

/**
 * Print `module_access` node.
 */
export function printModuleAccess(path: AstPath<Node>, _opt: MoveOptions, print: printFn): Doc {
    return path.map(print, 'children');
}

/**
 * Print `ref_type` node.
 */
export function printRefType(path: AstPath<Node>, _opt: MoveOptions, print: printFn): Doc {
    return group([
        path.call(print, 'nonFormattingChildren', 0), // ref_type
        path.call(print, 'nonFormattingChildren', 1), // type
    ]);
}

/**
 * Print `arg_list` node.
 */
function printArgList(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    const nodes = path.node.nonFormattingChildren;

    if (nodes.length === 1 && nodes[0]!.isBreakableExpression) {
        const child = nodes[0]!;
        const shouldBreak =
            nodes[0]?.trailingComment?.type === 'line_comment' ||
            nodes[0]?.leadingComment.some((e) => e.type === 'line_comment');

        if (shouldBreak) {
            return [
                '(',
                indent(hardline),
                indent(path.call(print, 'nonFormattingChildren', 0)),
                hardline,
                ')',
            ];
        }

        return ['(', path.call(print, 'nonFormattingChildren', 0), ')'];
    }

    return group(list({ path, print, options, open: '(', close: ')' }), {
        shouldBreak: shouldBreakFirstChild(path),
    });
}

/**
 * Print `ability` node.
 */
export function printAbility(path: AstPath<Node>, _opt: MoveOptions, _p: printFn): Doc {
    return path.node.text;
}

/**
 * Print `tuple_type` node.
 */
export function printTupleType(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    return group(
        list({
            path,
            print,
            options,
            open: '(',
            close: ')',
            shouldBreak: false,
        }),
    );
}

// === Bindings ===

/**
 * Print `bind_unpack` node.
 * For easier seach: `unpack_expression`.
 *
 * Inside:
 * - `bind_var`
 * - `bind_fields`
 * - `bind_fields`
 *
 * `let Struct { field1, field2 } = ...;`
 */
function printBindUnpack(path: AstPath<Node>, _opt: MoveOptions, print: printFn): Doc {
    return path.map(print, 'nonFormattingChildren');
}

/**
 * Print `bind_fields` node.
 * Choice node between `bind_named_fields` and `bind_positional_fields`.
 */
function printBindFields(path: AstPath<Node>, _opt: MoveOptions, print: printFn): Doc {
    return path.call(print, 'nonFormattingChildren', 0);
}

/**
 * Print `bind_field` node.
 */
function printBindField(path: AstPath<Node>, _opt: MoveOptions, print: printFn): Doc {
    // special case for `..` operator
    if (path.node.child(0)?.type == '..') {
        return '..';
    }

    // if there's only one child, we can just print it
    // if there're two, they will be joined
    return join(': ', path.map(print, 'nonFormattingChildren'));
}

/**
 * Print `mut_bind_field` node.
 */
function printMutBindField(path: AstPath<Node>, _opt: MoveOptions, print: printFn): Doc {
    return ['mut ', path.call(print, 'nonFormattingChildren', 0)];
}

/**
 * Print `bind_list` node.
 * In the bind list we have two paths:
 *
 * - one is just `bind_var` with potential `mut`
 * - another is a list, and we know it because the first member is `(`.
 */
function printBindList(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    if (path.node.nonFormattingChildren.length == 1) {
        return join(' ', path.map(print, 'nonFormattingChildren'));
    }

    return group(list({ path, print, options, open: '(', close: ')' }));
}

/**
 * Print `comma_bind_list` node.
 */
function printCommaBindList(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    return group(list({ path, print, options, open: '(', close: ')' }));
}

/**
 * Print `at_bind` node.
 */
function printAtBind(path: AstPath<Node>, _opt: MoveOptions, print: printFn): Doc {
    return join(' @ ', path.map(print, 'nonFormattingChildren'));
}

/**
 * Print `or_bind_list` node.
 */
function printOrBindList(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    return group(join([' |', line], path.map(print, 'nonFormattingChildren')));
}

/**
 * Print `bind_named_fields` node.
 */
function printBindNamedFields(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    return [
        ' ',
        group(list({ path, print, options, open: '{', close: '}', addWhitespace: true }), {
            shouldBreak: shouldBreakFirstChild(path),
        }),
    ];
}

/**
 * Print `bind_positional_fields` node.
 */
function printBindPositionalFields(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    return group(list({ path, print, options, open: '(', close: ')' }), {
        shouldBreak: shouldBreakFirstChild(path),
    });
}

/**
 * Print `bind_var` node.
 */
function printBindVar(path: AstPath<Node>, _opt: MoveOptions, print: printFn): Doc {
    return path.call(print, 'nonFormattingChildren', 0);
}

/**
 * Print `mut_bind_var` node.
 */
function printMutBindVar(path: AstPath<Node>, _opt: MoveOptions, print: printFn): Doc {
    return ['mut ', path.call(print, 'nonFormattingChildren', 0)];
}

/**
 * Print `imm_ref` node.
 */
function printImmRef(path: AstPath<Node>, _opt: MoveOptions, print: printFn): Doc {
    return '&';
}

/**
 * Print `mut_ref` node.
 */
function printMutRef(path: AstPath<Node>, _opt: MoveOptions, print: printFn): Doc {
    return '&mut ';
}

/**
 * Print `alias` node. ...as `identifier`
 */
export function printAlias(path: AstPath<Node>, _opt: MoveOptions, print: printFn): Doc {
    return ['as ', path.call(print, 'nonFormattingChildren', 0)];
}

/**
 * Print `block_identifier` node.
 */
function printBlockIdentifier(path: AstPath<Node>, _opt: MoveOptions, print: printFn): Doc {
    return path.call(print, 'nonFormattingChildren', 0);
}

/**
 * Print `label` node.
 */
function printLabel(path: AstPath<Node>, _opt: MoveOptions, _p: printFn): Doc {
    if (path.node.nextSibling?.type == ':') {
        return [path.node.text, ':'];
    }

    return path.node.text;
}

/**
 * Print `unary_op` node.
 */
function printUnaryOperator(path: AstPath<Node>, _opt: MoveOptions, _p: printFn): Doc {
    return path.node.text;
}

/**
 * Print `field_initialize_list` node.
 */
function printFieldInitializeList(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    return [
        ' ',
        group(list({ path, print, options, open: '{', close: '}', addWhitespace: true }), {
            shouldBreak: shouldBreakFirstChild(path),
        }),
    ];
}

/**
 * Print `expression_field` node.
 * Inside:
 * - `field_identifier`
 * - `expression`
 */
function printExpressionField(path: AstPath<Node>, _opt: MoveOptions, print: printFn): Doc {
    const children = path.map(print, 'nonFormattingChildren');

    if (children.length === 1) {
        return children[0]!;
    }

    return group([children[0]!, ': ', children[1]!]);
}

/**
 * Print `function_type` node.
 * Inside:
 * - `function_type_parameters`
 * - `return_type`
 */
function printFunctionType(path: AstPath<Node>, _opt: MoveOptions, print: printFn): Doc {
    const children = path.map(print, 'nonFormattingChildren');

    if (children.length === 0) {
        return '||';
    }

    if (children.length === 1) {
        return children[0]!;
    }

    return join(' -> ', children);
}

/**
 * Print `function_type_parameters` node.
 */
function printFunctionTypeParameters(
    path: AstPath<Node>,
    options: MoveOptions,
    print: printFn,
): Doc {
    return group(list({ path, print, options, open: '|', close: '|' }));
}
