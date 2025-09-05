// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { AstPath, Doc } from 'prettier';
import { Node } from '..';
import { treeFn } from '../printer';

/**
 * Node: `_literal_value` in the grammar.json.
 */
export enum Literal {
    AddressLiteral = 'address_literal',
    BoolLiteral = 'bool_literal',
    NumLiteral = 'num_literal',
    HexStringLiteral = 'hex_string_literal',
    ByteStringLiteral = 'byte_string_literal',
}

export default function (path: AstPath<Node>): treeFn | null {
    switch (path.node.type) {
        case Literal.AddressLiteral:
            return printAddressLiteral;
        case Literal.BoolLiteral:
            return printBoolLiteral;
        case Literal.NumLiteral:
            return printNumLiteral;
        case Literal.HexStringLiteral:
            return printHexStringLiteral;
        case Literal.ByteStringLiteral:
            return printByteStringLiteral;
    }

    return null;
}

/**
 * Print `byte_string_literal` node.
 */
export function printByteStringLiteral(path: AstPath<Node>): Doc {
    return path.node.text;
}

/**
 * Print `bool_literal` node.
 */
export function printBoolLiteral(path: AstPath<Node>): Doc {
    return path.node.text;
}

/**
 * Print `num_literal` node.
 */
export function printNumLiteral(path: AstPath<Node>): Doc {
    return path.node.text;
}

/**
 * Print `address_literal` node.
 */
export function printAddressLiteral(path: AstPath<Node>): Doc {
    return path.node.text;
}

/**
 * Print `hex_literal` node.
 */
export function printHexStringLiteral(path: AstPath<Node>): Doc {
    return path.node.text;
}
