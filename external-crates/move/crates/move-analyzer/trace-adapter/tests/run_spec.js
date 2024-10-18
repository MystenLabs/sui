// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

const assert = require('assert');
const linediff = require('line-diff');
const fs = require('fs');
const path = require('path');
const runtime = require('../out/runtime');

const UB = process.env['UB'];

/**
 * Testing harness, assuming that the tested function
 * is the `test` function in the `m` module. It executes
 * a given callback function and compares its result with
 * the expected one stored in a file.
 *
 * @param dirname the directory where the test (its manifest file) is located
 * @param action a function to be executed by the harness that
 * takes DAP runtime as argument and returns a string representing
 * test result
 */
global.run_spec = function (dirname, action) {
    const test_dir = path.basename(dirname);
    describe(test_dir, () => {
        it(test_dir, () => {
            const rt = new runtime.Runtime();
            // assume that the test is always in the `test` function
            // of the `m` module
            const traceInfo = test_dir +  '::' + 'm::test';
            return rt.start(path.join(dirname, 'sources', `m.move`), traceInfo, true).then(() => {
                const result = action(rt);
                const exp_file = 'test.exp';
                const exp_path = path.join(dirname, exp_file);
                if (UB === '1') {
                    // user asked to regenerate output
                    fs.writeFileSync(exp_path, result, 'utf8');
                    return;
                }
                if (!fs.existsSync(exp_path)) {
                    assert.fail(`\n${result}\nNo expected output file`);
                }
                const exp_out = fs.readFileSync(exp_path, { encoding: 'utf8' });
                if (result !== exp_out) {
                    const out_diff = new linediff(exp_out, result).toString();
                    assert.fail(`${out_diff}\nCurrent output does not match the expected one (run with UB=1 to save the current output)`);
                }
            });
        });
    });
};
