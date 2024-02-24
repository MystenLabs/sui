// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import type { AstPath, Doc, ParserOptions } from 'prettier';
import * as prettier from 'prettier';

const { hardline, indent, join, line, group, indentIfBreak } = prettier.doc.builders;

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
                    indent([hardline, join(hardline, path.map(print, 'namedChildren'))]),
                    hardline,
                    '}'
                ];
            }
        case 'constant':
            // break and indent only on the equal sign
            const nid = Symbol('cname');
            const tid = Symbol('tname');
            return group([
                'const ',
                path.call(print, 'namedChildren', 0),
                ': ',
                path.call(print, 'namedChildren', 1),
                ' =',
                indent([line, path.call(print, 'namedChildren', 2)]),
                ';',
            ]);
        default:
            return node.text;
    }
}
