// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '..';
import { MoveOptions, printFn, treeFn } from '../printer';
import { AstPath, Doc, ParserOptions, doc } from 'prettier';
import { FunctionDefinition } from './function_definition';
import { StructDefinition } from './struct_definition';
import * as Constant from './constant';
import { UseDeclaration } from './use_declaration';
import { printImports, collectImports } from '../imports-grouping';
import { EnumDefinition } from './enum_definition';
import { printIdentifier } from '../utilities';
const { join, hardline, indent } = doc.builders;

/**
 * Creates a callback function to print modules and module-related nodes.
 */
export default function (path: AstPath<Node>): treeFn | null {
    switch (path.node.type) {
        case Module.ModuleExtensionDefinition:
            return printModuleExtensionDefinition;
        case Module.ModuleDefinition:
            return printModuleDefinition;
        case Module.ModuleIdentity:
            return printModuleIdentity;
        case Module.ModuleIdentifier:
            return printIdentifier;
        case Module.ModuleBody:
            return printModuleBody;
        default:
            return null;
    }
}

/**
 * Module - top-level definition in a Move source file.
 */
export enum Module {
    ModuleExtensionDefinition = 'module_extension_definition',
    ModuleDefinition = 'module_definition',
    BlockComment = 'block_comment',
    ModuleIdentity = 'module_identity',
    ModuleIdentifier = 'module_identifier',
    ModuleBody = 'module_body',
}

/**
 * Print `module_extension_definition` node.
 */
export function printModuleExtensionDefinition(
    path: AstPath<Node>,
    _options: MoveOptions,
    print: printFn,
): Doc {
    return ['extend ', path.call(print, 'nonFormattingChildren', 0)];
}

/**
 * Print `module_definition` node.
 */
export function printModuleDefinition(
    path: AstPath<Node>,
    options: MoveOptions,
    print: printFn,
): Doc {
    let useLabel = false;

    // when option is present we must check that there's only one module per file
    if (options.useModuleLabel) {
        const modules = path.parent!.nonFormattingChildren.filter(
            (node) =>
                node.type === 'module_definition' || node.type === 'module_extension_definition',
        );

        useLabel = modules.length == 1;
    }

    // module definition can be a part of the extension, do decide whether to use
    // the label for it, we need to check its parent's parent
    if (options.useModuleLabel && path.parent!.type === 'module_extension_definition') {
        const modules = path.parent!.parent!.nonFormattingChildren.filter(
            (node) =>
                node.type === 'module_definition' || node.type === 'module_extension_definition',
        );

        useLabel = modules.length == 1;
    }

    const result = ['module ', path.call(print, 'nonFormattingChildren', 0)];

    // if we're using the label, we must add a semicolon and print the body in a
    // new line
    if (useLabel) {
        // print hard lines only if there is more than one child
        if (path.node.nonFormattingChildren[1]!.children.length > 1) {
            return result.concat([
                ';',
                hardline,
                hardline,
                path.call(print, 'nonFormattingChildren', 1),
            ]);
        } else {
            return result.concat([';']);
        }
    }

    // when not module mabel, module body is a block with curly braces and
    // indentation
    return result.concat([
        ' {',
        indent(hardline),
        indent(path.call(print, 'nonFormattingChildren', 1)),
        hardline,
        '}',
    ]);
}

/**
 * Print `module_identity` node.
 */
function printModuleIdentity(path: AstPath<Node>, options: ParserOptions, print: printFn): Doc {
    return join('::', path.map(print, 'nonFormattingChildren'));
}

/**
 * Members that must be separated by an empty line if they are next to each other.
 * For example, a function definition followed by a struct definition.
 */
const separatedMembers = [
    FunctionDefinition.FunctionDefinition,
    StructDefinition.StructDefinition,
    Constant.NODE_TYPE,
    UseDeclaration.UseDeclaration,
    UseDeclaration.FriendDeclaration,
    EnumDefinition.EnumDefinition,
] as string[];

/**
 * Print `module_body` node.
 *
 * We need to preserve spacing between members (functions, structs, constants, etc.).
 * We need to only allow a single empty line (if there are more than one, we should remove them).
 * Additionally, if `groupImports` is set to `package` or `module`, we should group imports and
 * print them at the top of the module.
 */
function printModuleBody(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    const nodes = path.node.namedAndEmptyLineChildren;
    const importsDoc = [] as Doc[];
    const imports = collectImports(path.node);
    if (imports.size > 0) {
        importsDoc.push(
            ...(printImports(imports, options.autoGroupImports as 'package' | 'module') as Doc[]),
        );
    }

    const bodyDoc = [] as Doc[];

    path.each((path, i) => {
        const next = nodes[i + 1];

        // empty lines should be removed if they are next to grouped imports
        if (path.node.isEmptyLine && path.node.previousNamedSibling?.isGroupedImport) return;
        if (path.node.isGroupedImport) return;
        if (path.node.isEmptyLine && !path.node.previousNamedSibling) return;

        if (
            separatedMembers.includes(path.node.type) &&
            separatedMembers.includes(next?.type || '') &&
            path.node.type !== next?.type
        ) {
            return bodyDoc.push([path.call(print), hardline]);
        }

        // force add empty line after function definitions
        if (
            path.node.type === FunctionDefinition.FunctionDefinition &&
            next?.type === FunctionDefinition.FunctionDefinition
        ) {
            return bodyDoc.push([path.call(print), hardline]);
        }

        return bodyDoc.push(path.call(print));
    }, 'namedAndEmptyLineChildren');

    if (bodyDoc.length > 0 && importsDoc.length > 0) {
        bodyDoc.unshift(''); // add empty line before first member
    }

    return join(hardline, importsDoc.concat(bodyDoc));
}
