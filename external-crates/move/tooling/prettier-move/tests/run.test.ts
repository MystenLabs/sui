// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import * as assert from 'assert';
import * as diff from 'diff';
import * as fs from 'fs';
import * as path from 'path';
import * as plugin from '../';
import * as prettier from 'prettier';
import { describe, it } from 'vitest';
import { MoveOptions } from '../src/printer';

const UB = process.env['UB'];
const FILTER = process.env['FILTER'];
const OPTIONS_HEADER = '// options:';

// Read the current directory and run the tests in each subdirectory.
fs.readdirSync(__dirname).forEach((dir) => {
    const dirname = path.join(__dirname, dir);
    const isDir = fs.statSync(dirname).isDirectory();

    if (isDir) {
        const files = fs
            .readdirSync(dirname)
            .filter((f) => f.endsWith('.move') && !f.endsWith('.exp.move'))
            .map((f) => path.join(dirname, f))
            .filter((f) => fs.statSync(f).isFile())
            .filter((f) => (FILTER ? f.includes(FILTER) : true))
            .map((path) => [path, fs.readFileSync(path).toString()]);

        if (files.length) {
            describe(dir, () => {
                files.forEach(([path, content]) => runSpec(path, content));
            });
        }
    }

    // const matches = FILTER ? dir.includes(FILTER) : true;
    // if (isDir && matches) {
    // 	describe(dir, () => runSpec(dirname));
    // }
});

// Pin the shipped plugin defaults: format with no plugin options at all.
// Changing an option default changes output for every client with no config —
// this test makes such a change loud and deliberate.
describe('plugin defaults', () => {
    it('groups imports by package and uses the module label form', async () => {
        const src = 'module a::m;\n\nuse sui::b::B;\nuse sui::a::A;\n\nfun f(_: A, _: B) {}\n';
        const result = await prettier.format(src, {
            // @ts-ignore
            plugins: [plugin],
            parser: 'move',
        });
        assert.ok(result.startsWith('module a::m;'), 'default useModuleLabel=true keeps the label');
        assert.ok(result.includes('use sui::{a::A, b::B};'), 'default autoGroupImports=package');
    });

    it('converts a single braces module to the label form by default', async () => {
        const src = 'module a::m {\n    fun f(): u64 { 42 }\n}\n';
        const result = await prettier.format(src, {
            // @ts-ignore
            plugins: [plugin],
            parser: 'move',
        });
        assert.ok(result.startsWith('module a::m;'), 'single-module braces convert to label');
    });

    it('keeps braces when the file has more than one module', async () => {
        const src = 'module a::m {\n    fun f() {}\n}\n\nmodule a::n {\n    fun g() {}\n}\n';
        const result = await prettier.format(src, {
            // @ts-ignore
            plugins: [plugin],
            parser: 'move',
        });
        assert.ok(result.includes('module a::m {'), 'multi-module files keep braces');
        assert.ok(result.includes('module a::n {'), 'multi-module files keep braces');
    });
});

/**
 * Runs the tests in the given directory.
 */
function runSpec(filepath: string, content: string) {
    it(filepath.split('/').slice(-1)[0], async () => {
        const expFile = filepath.replace('.move', '.exp.move');

        // allows `// options:` header in the test file to set prettier options
        // e.g.
        // ```
        // // options:
        // // printWidth: 80
        // // tabWidth: 2
        // // useModuleLabel: true
        // ```
        let config = {
            printWidth: 80,
            tabWidth: 4,
            wrapComments: false,
            useModuleLabel: false,
            autoGroupImports: 'module',
            enableErrorDebug: false,
        };

        if (content.startsWith(OPTIONS_HEADER)) {
            let lines = content.split('\n').slice(0, 10);
            while (lines.length) {
                let line = lines.shift();
                if (line?.startsWith('// ')) {
                    let value =
                        /(printWidth|wrapComments|tabWidth|useModuleLabel|autoGroupImports|enableErrorDebug)\:\ (true|false|module|package|none|[0-9]+)/.exec(
                            line,
                        );
                    if (value) {
                        let [_, key, val] = value || [];
                        switch (key) {
                            case 'wrapComments':
                            case 'useModuleLabel':
                            case 'enableErrorDebug':
                                config[key] = val == 'true';
                                break;
                            case 'autoGroupImports':
                                config[key] = val;
                                break;
                            case 'printWidth':
                            case 'tabWidth':
                                config[key] = parseInt(val);
                                break;
                        }
                    }
                }
            }
        }

        const result = await prettier.format(content, {
            // @ts-ignore
            plugins: [plugin],
            parser: 'move',
            printWidth: config.printWidth,
            tabWidth: config.tabWidth,
            wrapComments: config.wrapComments,
            useModuleLabel: config.useModuleLabel,
            autoGroupImports: config.autoGroupImports as MoveOptions['autoGroupImports'],
            enableErrorDebug: config.enableErrorDebug,
        });

        // user asked to regenerate output
        if (UB == '1') return fs.writeFileSync(expFile, result, 'utf8');
        if (!fs.existsSync(expFile)) {
            return assert.fail(`\n${result}\nNo expected output file`);
        }

        const expected = fs.readFileSync(expFile, { encoding: 'utf8' });

        if (result != expected) {
            const [snapshot, actual] = diff.diffLines(expected, result);
            const lineStart = snapshot.value.length;
            assert.fail(
                `\n\nRESULT:\n+${result
                    .slice(lineStart, lineStart + actual.value.length)
                    .split('\n')
                    .join(
                        '\n+',
                    )}\n\nEXPECTED:\n-${actual.value.split('\n').join('\n-')}\nCurrent output does not match the expected one (run with UB=1 to save the current output)`,
            );
        }
    });
}
