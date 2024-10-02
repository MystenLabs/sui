// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import type { AstPath, Doc, ParserOptions } from 'prettier';
import * as prettier from 'prettier';
import { SyntaxNode } from 'web-tree-sitter'

const { hardline, indent, join, line, softline, group, ifBreak } = prettier.doc.builders;

type printFn = (path: AstPath) => Doc;

export function print(path: AstPath, options: ParserOptions, print: printFn) {
    const node = path.getValue()

    switch (node.type) {
        case 'source_file':
            return join(hardline, path.map(print, 'children'));
        case 'module_definition':
            return [
                'module ',
                path.call(print, 'namedChildren', 0), // module_identity
                ' ',
                path.call(print, 'namedChildren', 1), // module_body
                hardline
            ];
        case 'module_identity':
            return [
                path.call(print, 'namedChildren', 0),
                '::',
                path.call(print, 'namedChildren', 1)
            ];
        case 'module_body':
            if (node.children.length == 2) {
                // empty module (the only children are curlies)
                return [ '{}' ];
            } else {
                return [
                    '{',
                    indent([[hardline, hardline], join([hardline, hardline], path.map(print, 'namedChildren'))]),
                    hardline,
                    '}'
                ];
            }
        case 'constant':
            // break and indent only on the equal sign so long form looks as follows:
            //
            // const c: u64 =
            //    42;
            return group([
                'const ',
                path.call(print, 'namedChildren', 0),
                ': ',
                path.call(print, 'namedChildren', 1),
                ' =',
                indent([line, path.call(print, 'namedChildren', 2)]),
                ';',
            ]);
        case 'struct_definition':
            // type parameters are on separate lines if they don't fit on one, but fields are always
            // on separate lines:
            //
            // struct SomeStruct<T1: key, T2: drop> has key {
            //     f: u64,
            // }
            //
            // struct AnotherStruct<
            //     T1: store + drop + key,
            //     T2: store + drop + key,
            //     T3: store + drop + key,
            // > has key, store {
            //     f1: u64,
            //     f2: u64,
            // }
            return [
                node.child(0).type === 'public' ? 'public ' : '',
                'struct ',
                path.call(print, 'namedChildren', 0),
                path.call(print, 'namedChildren', 1),
                path.call(print, 'namedChildren', 2),
                path.call(print, 'namedChildren', 3),
                path.call(print, 'namedChildren', 4),
            ]
        case 'native_struct_definition':
            // same formatting as "regular" struct but (of course) without fields
            return [
                'struct ',
                path.call(print, 'namedChildren', 0),
                path.call(print, 'namedChildren', 1),
                path.call(print, 'namedChildren', 2),
                ';',
            ]
        case 'function_definition':
            let is_entry = false;
            for (let i = 0; i < node.childCount; i++) {
                if (node.child(i).type === 'entry') {
                    is_entry = true;
                }
            }
            // first named child may be a visibility modifier
            return [
                node.namedChild(0).type === 'visibility_modifier' ? [ path.call(print, 'namedChildren', 0), ' '] : '',
                is_entry ? 'entry ' : '',
                'fun ',
                node.namedChild(0).type !== 'visibility_modifier' ? path.call(print, 'namedChildren', 0) : '',
                path.call(print, 'namedChildren', 1),
                path.call(print, 'namedChildren', 2),
                path.call(print, 'namedChildren', 3),
                path.call(print, 'namedChildren', 4),
                path.call(print, 'namedChildren', 5),
            ];
        case 'native_function_definition':
            // first named child may be a visibility modifier
            return [
                node.namedChild(0).type === 'visibility_modifier' ? [ path.call(print, 'namedChildren', 0), ' '] : '',
                'native ',
                'fun ',
                node.namedChild(0).type !== 'visibility_modifier' ? path.call(print, 'namedChildren', 0) : '',
                path.call(print, 'namedChildren', 1),
                path.call(print, 'namedChildren', 2),
                path.call(print, 'namedChildren', 3),
                path.call(print, 'namedChildren', 4),
                ';',
            ];
        // TODO: do macros
        case 'ability_decls':
            return [
                ' has ',
                join(', ', path.map(print, 'namedChildren'))
            ];
        case 'postfix_ability_decls':
            return [
                ' has ',
                join(', ', path.map(print, 'namedChildren')),
                ';',
            ];
        case 'type_parameters':
            return breakable_comma_separated_list(path, node, '<', '>', print);
        case 'type_parameter':
            let abilities = [];
            for (let i = 1; i < node.namedChildCount; i++) {
                abilities.push(path.call(print, 'namedChildren', i));
            }
            return [
                // '$' and 'phantom' are mutually exclusive (one for macros and the other for structs)
                node.child(0).type === '$' ? '$' : (node.child(0).type === 'phantom' ? 'phantom ' : ''),
                path.call(print, 'firstNamedChild'),
                node.namedChildren.length > 1 ? ': ' : '' ,
                join(' + ', abilities),
            ];
        case 'datatype_fields':
            return path.call(print, 'firstNamedChild');
        case 'named_fields':
            return block(path, node, print, ',');
        case 'field_annotation':
            return [
                path.call(print, 'namedChildren', 0),
                ': ',
                path.call(print, 'namedChildren', 1),
            ];
        case 'positional_fields':
            return breakable_comma_separated_list(path, node, '(', ')', print);
        case 'block':
            return block(path, node, print, '');
        case 'function_parameters':
            return breakable_comma_separated_list(path, node, '(', ')', print);
        case 'function_parameter':
            return [
                path.call(print, 'namedChildren', 0),
                ': ',
                path.call(print, 'namedChildren', 1),
            ];
        case 'ret_type':
            return [ ': ', path.call(print, 'namedChildren', 0) ];
        case 'struct_identifier':
        case 'ability':
        case 'type_parameter_identifier':
        case 'field_identifier':
        case 'function_identifier':
        case 'variable_identifier':
        default:
            return node.text;
    }
}

function breakable_comma_separated_list(path: AstPath,
                                        node: SyntaxNode,
                                        start: string,
                                        end: string,
                                        print: printFn) {

    const items = Symbol('items');
    return [
        start,
        group([
            indent(softline),
            indent(join([',', line], path.map(print, 'namedChildren'))),
        ], {id: items}),
        ifBreak([',', softline], '', {groupId: items}),
        end,
    ];
}

function block(path: AstPath, node: SyntaxNode, print: printFn, line_ending: string) {
    return node.namedChildren.length == 0
        ? ' {}'
        : [
            ' {',
            indent(hardline),
            indent(join([line_ending, hardline], path.map(print, 'namedChildren'))),
            line_ending,
            hardline,
            '}',
        ];
}
