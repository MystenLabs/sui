// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

/**
 * This module contains the logic for grouping imports in a file.
 *
 * @module imports-grouping
 */

import { Doc, doc } from 'prettier';
import { Node } from './';
import { UseDeclaration } from './cst/use_declaration';
const { join, softline, indent, line, group } = doc.builders;

// === Import Grouping ===

/**
 * A simple type to represent grouped imports.
 */
export type GroupedImports = Map<string, Map<string, Member[]>>;

type Member = {
    name: string | 'Self';
    alias: string | undefined;
};

/**
 * Special function to print imports if import grouping is turned on.
 * Important note: we don't use the `print` function for imports, as they're already
 * parsed. We just need to print them in the correct order and format.
 *
 * We sort and avoid duplicates in the imports. To do so, we keep track of the
 * printed keys (including alias!), and only print the import if it hasn't been
 * printed before.
 */
export function printImports(imports: GroupedImports, option: 'module' | 'package'): Doc {
    const pkgs = [...imports.keys()].sort();
    const result = [] as Doc[];

    for (const pkg of pkgs) {
        const modules = imports.get(pkg);

        // typescript wants this
        if (modules == undefined) {
            continue;
        }

        const keys = [...modules.keys()].sort();

        // if grouped by module
        if (option === 'module') {
            for (const mod of keys) {
                if (!modules.get(mod)) continue;
                result.push(['use ', pkg, '::', printModule(mod, modules.get(mod)!), ';']);
            }
        } else {
            // if grouped by package
            const modulesDoc = [] as Doc[];

            for (const mod of keys) {
                if (!modules.has(mod)) continue;
                modulesDoc.push(printModule(mod, modules.get(mod)!));
            }

            modulesDoc.length === 1
                ? result.push(['use ', pkg, '::', modulesDoc[0]!, ';'])
                : result.push([
                      'use ',
                      pkg,
                      '::',
                      group([
                          '{',
                          indent(softline),
                          indent(join([',', line], modulesDoc)),
                          softline,
                          '}',
                      ]),
                      ';',
                  ]);
        }
    }

    return result;
}

function printModule(mod: string, members: Member[]): Doc {
    const printedKeys: string[] = [];

    // perform deduplication of imports
    members = members.filter((m) => {
        const key = [mod, m.name, m.alias || '-'].join('');
        if (printedKeys.includes(key)) return false;
        printedKeys.push(key);
        return true;
    });

    if (members.length === 1) {
        const member = members[0]!;
        if (member.name === 'Self') {
            const alias = member.alias ? ` as ${member.alias}` : '';
            return `${mod}${alias}`;
        }

        return [mod, '::', printMember(member)];
    }

    const selfIdx = members.findIndex((m) => m.name === 'Self');
    if (selfIdx !== -1) {
        const self = members.splice(selfIdx, 1);
        members = [...self, ...members];
    }

    return members.length === 0
        ? [mod]
        : [
              mod,
              '::',
              group([
                  '{',
                  indent(softline),
                  indent(join([',', line], members.map(printMember))),
                  softline,
                  '}',
              ]),
          ];
}

/**
 * Print a single member of a module with an optional alias.
 */
function printMember({ name, alias }: Member): Doc {
    const a = alias ? ` as ${alias}` : '';
    return `${name}${a}`;
}

/**
 * Special function which walks the current node and collects all `use` imports.
 * Returns a tree of all imports in this file to be used for grouping.
 *
 * There are 3 main types of imports:
 * - `use_module` - `module_access` <as `alias`>;
 * - `use_module_member` - `<module_identity>::<use_member>`;
 * - `use_module_members` - `package::<use_member>, <use_member>, ...`
 *
 * @param node
 * @returns
 */
export function collectImports(node: Node): GroupedImports {
    const grouped: GroupedImports = new Map();
    const imports = node.nonFormattingChildren
        .filter((n) => n.isGroupedImport)
        .map((n) => n.nonFormattingChildren[0]!);

    for (let import_ of imports) {
        switch (import_.type) {
            // `module_access` <as `alias`>;
            case UseDeclaration.UseModule: {
                const moduleIdentity = import_.nonFormattingChildren[0]!;
                const alias = import_.nonFormattingChildren[1];
                const [pkg, mod] = parseModuleIdentity(moduleIdentity);

                // we use `Self` in the tree to represent the current module
                const rec = { name: 'Self', alias: alias?.text };

                // if there hasn't been a registered package yet, add it
                if (!grouped.has(pkg)) grouped.set(pkg, new Map());
                const pkgMap = grouped.get(pkg)!;
                // if there hasn't been a registered module yet, add it
                if (!pkgMap.has(mod)) pkgMap.set(mod, []);
                pkgMap.set(mod, pkgMap.get(mod)!);
                pkgMap.get(mod)!.push(rec);

                break;
            }
            // `<module_identity>::<use_member>`
            case UseDeclaration.UseModuleMember: {
                const moduleIdentity = import_.nonFormattingChildren[0]!;
                const [pkg, mod] = parseModuleIdentity(moduleIdentity);
                const useMember = import_.nonFormattingChildren[1]!;
                const [name, alias] = parseUseMember(useMember);

                if (!grouped.has(pkg)) grouped.set(pkg, new Map());
                const pkgMap = grouped.get(pkg)!;
                if (!pkgMap.has(mod)) pkgMap.set(mod, []);
                const modMap = pkgMap.get(mod)!;
                modMap.push({ name, alias });

                break;
            }
            // The only tricky node in this scheme. `use_module_members` can be
            // both for grouped by package and for grouped by module, so we have
            // to detect which version it is and then dance off of that.
            case UseDeclaration.UseModuleMembers: {
                const children = import_.nonFormattingChildren;
                const isGroupedByPackage = children[0]!.type === 'module_identifier';

                if (!isGroupedByPackage && children[0]!.type !== UseDeclaration.ModuleIdentity) {
                    throw new Error('Expected `module_identity` or `module_identifier`');
                }

                // simple scenario: the first node is `module_identity`
                if (!isGroupedByPackage) {
                    const moduleIdentity = children[0]!;
                    const [pkg, mod] = parseModuleIdentity(moduleIdentity);
                    const members = children.slice(1).map((n) => parseUseMember(n));

                    if (!grouped.has(pkg)) grouped.set(pkg, new Map());
                    const pkgMap = grouped.get(pkg)!;
                    if (!pkgMap.has(mod)) pkgMap.set(mod, []);
                    const modMap = pkgMap.get(mod)!;

                    modMap.push(...members.map(([name, alias]) => ({ name, alias })));

                    break;
                }

                // complex scenario: the first node is `module_identifier`
                // `use_member` can be recursive in this scenario with 1 level of nesting
                const pkg = children[0]!.text;
                if (!grouped.has(pkg)) grouped.set(pkg, new Map());
                const pkgMap = grouped.get(pkg)!;

                children.slice(1).forEach((node) => {
                    if (!node) return;

                    if (node.type !== UseDeclaration.UseMember)
                        throw new Error('Expected `use_member` node got `' + node.type + '`');

                    const [first, ...rest] = node.nonFormattingChildren;
                    if (!first || first.type !== 'identifier')
                        throw new Error('Expected `identifier` node in `use_module_members`');

                    const mod = first.text;
                    if (!pkgMap.has(mod)) pkgMap.set(mod, []);

                    // if there's only one member and it's the module.
                    if (!rest.length) {
                        pkgMap.get(mod)!.push({ name: 'Self', alias: undefined });
                        return;
                    }

                    // ident + ident is an alias
                    if (rest.length == 1 && rest[0]?.type === 'identifier') {
                        if (rest[0].previousSibling?.type !== 'as') {
                            pkgMap.get(mod)!.push({ name: rest[0].text, alias: undefined });
                        } else {
                            pkgMap.get(mod)!.push({ name: 'Self', alias: rest[0].text });
                        }

                        return;
                    }

                    // special case, no use member, but already expanded pair of identifiers.
                    if (
                        rest.length == 2 &&
                        rest[0]?.type === 'identifier' &&
                        rest[1]?.type === 'identifier'
                    ) {
                        if (rest[1].previousSibling?.type !== 'as') {
                            throw new Error('Expected `as` keyword after module name');
                        }

                        pkgMap.get(mod)!.push({ name: rest[0].text, alias: rest[1].text });
                        return;
                    }

                    // the rest are `use_member` nodes
                    const members = rest.map(parseUseMember);
                    pkgMap.get(mod)!.push(...members.map(([name, alias]) => ({ name, alias })));
                });
            }
        }
    }

    return grouped;
}

/**
 * Parse a `module_identity` node returning a tuple of package and module.
 */
function parseModuleIdentity(node: Node): [string, string] {
    if (node.type !== UseDeclaration.ModuleIdentity) {
        throw new Error('Expected `module_identity` node');
    }

    const [pkg, mod] = node.nonFormattingChildren.map((n) => n.text);
    return [pkg!, mod!];
}

/**
 * Parse a simple `use_member` node returning a tuple of member and alias.
 */
function parseUseMember(node: Node): [string, string | undefined] {
    if (node.type !== UseDeclaration.UseMember) {
        throw new Error('Expected `use_member` node, got `' + node.type + '`');
    }

    const [member, alias] = node.nonFormattingChildren.map((n) => n.text);
    return [member!, alias];
}
