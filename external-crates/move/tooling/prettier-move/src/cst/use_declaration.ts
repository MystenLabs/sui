// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '..';
import { AstPath, Doc, doc } from 'prettier';
import { MoveOptions, printFn, treeFn } from '../printer';
const { group, indent, line, softline, ifBreak, join } = doc.builders;

export default function (path: AstPath<Node>): treeFn | null {
    switch (path.node.type) {
        case UseDeclaration.UseDeclaration:
            return printUseDeclaration;
        case UseDeclaration.UseModule:
            return printUseModule;
        case UseDeclaration.UseMember:
            return printUseMember;
        case UseDeclaration.UseModuleMember:
            return printUseModuleMember;
        case UseDeclaration.UseModuleMembers:
            return printUseModuleMembers;
        case UseDeclaration.UseFun:
            return printUseFun;
        case UseDeclaration.ModuleIdentity:
            return printModuleIdentity;
        case UseDeclaration.FriendDeclaration:
            return printFriendDeclaration;
        case UseDeclaration.FriendAccess:
            return printFriendAccess;
        default:
            return null;
    }
}

/**
 * Use Declaration
 *
 * Contains one of the following:
 *
 * `use_declaration` (
 * - use `use_module` <as `alias`>;
 * - use `use_module_member` <as `use_member`>;
 * - use `use_module_members`;
 * - use `use_fun`;
 * )
 *
 * `use_member` (
 * - `identifier` <as `alias`>;
 * )
 */
export enum UseDeclaration {
    /**
     * Module-level definition
     * ```
     * `<public> use ...;
     * ```
     */
    UseDeclaration = 'use_declaration',
    FriendDeclaration = 'friend_declaration',
    FriendAccess = 'friend_access',
    UseFun = 'use_fun',

    // all of the nodes below are implemented in `import-grouping.ts`
    // hence should never be printed directly.

    UseModule = 'use_module',
    UseMember = 'use_member',
    UseModuleMember = 'use_module_member',
    UseModuleMembers = 'use_module_members',
    ModuleIdentity = 'module_identity',
}

/**
 * Print @see `UseDeclaration.UseDeclaration` node.
 */
export function printUseDeclaration(
    path: AstPath<Node>,
    options: MoveOptions,
    print: printFn,
): Doc {
    const firstChild = path.node.child(0);
    const isPublic = firstChild && firstChild.type === 'public' ? ['public', ' '] : [];
    return [
        ...isPublic, // insert `public` keyword if present
        'use ',
        path.call(print, 'nonFormattingChildren', 0),
        ';',
    ];
}

/**
 * Print `use_fun` node `module_access` as `module_access`.`function_identifier`
 */
export function printUseFun(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    return group([
        'fun ',
        path.call(print, 'nonFormattingChildren', 0), // module_access
        ' as',
        indent(line),
        path.call(print, 'nonFormattingChildren', 1), // module_access
        '.',
        path.call(print, 'nonFormattingChildren', 2), // function_identifier
    ]);
}

/**
 * Print `friend_declaration` node.
 */
export function printFriendDeclaration(
    path: AstPath<Node>,
    options: MoveOptions,
    print: printFn,
): Doc {
    return group([
        'friend',
        indent(line),
        path.call(print, 'nonFormattingChildren', 0), // module_access
        ';',
    ]);
}

/**
 * Print `friend_access` node.
 */
export function printFriendAccess(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    return path.map(print, 'nonFormattingChildren');
}

/**
 * Print `use_module` node. `module_name`
 * Currently only used for `use` with annotations.
 */
export function printUseModule(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    return path.map((e) => {
        if (e.node.type == 'as') return ' as ';
        return print(e);
    }, 'children');
}

/**
 * Print `use_member` node. `member_name`
 * Currently only used for `use` with annotations.
 */
export function printUseMember(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    const isGroup = path.node.children.findIndex((e) => e.type == '{');

    // not found `::{...}`
    if (isGroup == -1) {
        return group(
            path.map((e) => {
                if (e.node.type == 'as') return ' as ';
                if (e.node.type == ',') return [',', line];
                return print(e);
            }, 'children'),
        );
    }

    const children = path.map(print, 'nonFormattingChildren');

    return group([
        children[0]!,
        '::{',
        indent(softline),
        indent(join([',', line], children.slice(1))),
        ifBreak(','), // trailing comma
        softline,
        '}',
    ]);
}

/**
 * Print `use_module_member` node. `module_name::member_name`
 * Single statement of direct import;
 * `use address::module_name::member_name;`
 *
 * Wraps the member into a group `{}` if it's too long (if line breaks).
 * Currently only used for `use` with annotations.
 */
export function printUseModuleMember(
    path: AstPath<Node>,
    options: MoveOptions,
    print: printFn,
): Doc {
    return group([
        path.call(print, 'nonFormattingChildren', 0), // module_access
        '::',
        ifBreak(['{', indent(line)]), // wrap with `{` if the member is too long
        indent(path.call(print, 'nonFormattingChildren', 1)), // module_access
        ifBreak([line, '}']), // trailing comma
    ]);
}

/**
 * Print `use_module_members` node. `module_identity::{member_name, member_name}`
 * Currently only used for `use` with annotations.
 */
export function printUseModuleMembers(
    path: AstPath<Node>,
    options: MoveOptions,
    print: printFn,
): Doc {
    const children = path.map(print, 'nonFormattingChildren');
    return group([
        children[0]!,
        '::{',
        indent(softline),
        indent(join([',', line], children.slice(1))),
        ifBreak(','), // trailing comma
        softline,
        '}',
    ]);
}

/**
 * Print the `module_identity` node. `module_identifier::module_name`.
 * Is present in the `use_module_member` and `use_module_members` nodes.
 * Currently only used for `use` with annotations.
 */
export function printModuleIdentity(
    path: AstPath<Node>,
    options: MoveOptions,
    print: printFn,
): Doc {
    return join('::', path.map(print, 'nonFormattingChildren'));
}

/**
 * Checks whether the given path is a `use` import.
 */
export function isUseImport(node: Node): boolean {
    const firstChild = node.nonFormattingChildren[0]!;

    return (
        node.type === UseDeclaration.UseDeclaration &&
        (firstChild.type === UseDeclaration.UseModule ||
            firstChild.type === UseDeclaration.UseModuleMember ||
            firstChild.type === UseDeclaration.UseModuleMembers)
    );
}
