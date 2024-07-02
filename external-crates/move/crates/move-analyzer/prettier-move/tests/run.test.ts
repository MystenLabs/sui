// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import * as assert from 'assert';
import * as diff from 'diff';
import * as fs from 'fs';
import * as path from 'path';
import * as plugin from '../';
import * as prettier from 'prettier';
import { describe, it } from 'vitest';

const UB = process.env['UB'];
const OPTIONS_HEADER = '// options:';

// Read the current directory and run the tests in each subdirectory.
fs.readdirSync(__dirname).forEach((dir) => {
	const dirname = path.join(__dirname, dir);
	if (fs.statSync(dirname).isDirectory()) {
		describe(dir, () => runSpec(dirname));
	}
});

/**
 * Runs the tests in the given directory.
 */
function runSpec(dirname: string) {
	return fs
		.readdirSync(dirname)
		.filter((f) => f.endsWith('.move') && !f.endsWith('.exp.move'))
		.forEach((file) =>
			it(file, async () => {
				const expFile = path.join(dirname, file.replace('.move', '.exp.move'));
				const inputFile = path.join(dirname, file);
				const content = fs.readFileSync(inputFile).toString();

				// allows `// options:` header in the test file to set prettier options
				// e.g.
				// ```
				// // options: printWidth: 80
				// // tabWidth: 2
				// ```
				let config = {
					printWidth: 80,
					tabWidth: 4,
				};

				if (content.startsWith(OPTIONS_HEADER)) {
					let lines = content.split('\n').slice(0, 10);
					while (lines.length) {
						let line = lines.shift();
						if (line?.startsWith('// ')) {
							let value = /(printWidth|tabWidth)\:\ ([0-9]+)/.exec(line);
							if (value) {
								let [_, key, val] = value || [];
								config[key] = parseInt(val);
							}
						}
					}
				}

				const result = await prettier.format(content, {
					plugins: [
						{
							languages: plugin.languages,
							parsers: plugin.parsers,
							// @ts-ignore
							printers: plugin.printers,
							defaultOptions: plugin.defaultOptions,
						},
					],
					parser: 'move-parse',
					printWidth: config.printWidth,
					tabWidth: config.tabWidth,
				});

				// user asked to regenerate output
				if (UB == '1') return fs.writeFileSync(expFile, result, 'utf8');
				if (!fs.existsSync(expFile)) {
					return assert.fail(`\n${result}\nNo expected output file`);
				}

				const expected = fs.readFileSync(expFile, { encoding: 'utf8' });

				if (result != expected) {
					const [snapshot, actual] = diff.diffLines(expected, result);
					assert.fail(
						`\nEXPECTED\n>${snapshot.value.split('\n').join('\n>')}\nGOT:\n>${actual.value.split('\n').join('\n >')}\nCurrent output does not match the expected one (run with UB=1 to save the current output)`,
					);
				}
			}),
		);
}
