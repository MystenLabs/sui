// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

const assert = require('assert');
const linediff = require('line-diff');
const fs = require('fs');
const path = require('path');
const plugin = require('../');
const prettier = require('prettier')

const UB = process.env["UB"];

global.run_spec = function(dirname) {
    const test_dir = path.basename(dirname);
    describe(test_dir, () => {
        const input_file = 'test.move';
        it(path.join(test_dir, input_file), () => {
            const exp_file = 'test.exp';
            const exp_path = path.join(dirname, exp_file);
            const input_path = path.join(dirname, input_file);
            const input_prog = fs.readFileSync(input_path, { encoding: 'utf8'});
            const result = prettier.format(input_prog, {
                plugins: [plugin],
                parser: 'move-parse',
            })
            return result.then((formatted_prog) => {
                if (UB == 1) {
                    // user asked to regenerate output
                    fs.writeFileSync(exp_path, formatted_prog, 'utf8');
                    return;
                }
                if (!fs.existsSync(exp_path)) {
                    assert.fail(`\n${formatted_prog}\nNo expected output file`)
                }
                const exp_prog = fs.readFileSync(exp_path, { encoding: 'utf8'});
                if (formatted_prog != exp_prog) {
                    const out_diff = new linediff(exp_prog, formatted_prog).toString();
                    assert.fail(`${out_diff}\nCurrent output does not match the expected one (run with UB=1 to save the current output)`);
                }
            });
        });
    });
}
