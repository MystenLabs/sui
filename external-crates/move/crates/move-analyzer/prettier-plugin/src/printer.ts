// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import type { AstPath, Doc, ParserOptions } from 'prettier';
import * as prettier from 'prettier';

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
                'struct ',
                path.call(print, 'namedChildren', 0),
                path.call(print, 'namedChildren', 1),
                path.call(print, 'namedChildren', 2),
                path.call(print, 'namedChildren', 3),
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
        case 'ability_decls':
            return [
                ' has ',
                join(', ', path.map(print, 'namedChildren'))
            ];
        case 'type_parameters':
            const tparams = Symbol('tparams');
            return [
                '<',
                group([
                    indent(softline),
                    indent(join([',', line], path.map(print, 'namedChildren'))),
                ], {id: tparams}),
                ifBreak([',', softline], '', {groupId: tparams}),
                '>',
            ];
        case 'type_parameter':
            let abilities = [];
            for (let i = 1; i < node.namedChildren.length; i++) {
                abilities.push(path.call(print, 'namedChildren', i));
            }
            return [
                path.call(print, 'firstNamedChild'),
                node.namedChildren.length > 1 ? ': ' : '' ,
                join(' + ', abilities),
            ];
        case 'struct_def_fields':
            return node.namedChildren.length == 0
                ? ' {}'
                : [
                    ' {',
                    indent(hardline),
                    indent(join([',', hardline], path.map(print, 'namedChildren'))),
                    ',',
                    hardline,
                    '}',
                ];
        case 'field_annotation':
            return [
                path.call(print, 'namedChildren', 0),
                ': ',
                path.call(print, 'namedChildren', 1),
            ];
        default:
            return node.text;
    }
}
