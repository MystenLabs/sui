// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

/**
 * This file contains what VS Code's documentation refers to as "the test runner," which
 * programmatically executes each test file in our extension. For more information, see:
 * https://code.visualstudio.com/api/working-with-extensions/testing-extension#the-test-runner-script
 */

import * as glob from 'glob';
import * as Mocha from 'mocha';
import * as path from 'path';
/* eslint-disable */
// deno-lint-ignore require-await
export async function run(): Promise<void> {
    // dev mode
    const mode = process.env['mode'] || 'test';
    if (mode === 'dev') {
        return new Promise((resolve) => {
            setTimeout(resolve, 1000 * 60 * 15); // Development mode, set a timeout of 15 minutes
        });
    }

    /* eslint-disable */
    const suite = new Mocha({
        ui: 'tdd',
        color: true,
        // The default timeout of 2000 miliseconds can sometimes be too quick, since the extension
        // tests need to launch VS Code first.
        timeout: 10000,
    });

    const testsRoot = path.resolve(__dirname, '..');
    return new Promise((resolve, reject) => {
        // The test suite is composed of all files ending with '.test.js'.
        glob('**/**.test.js', { cwd: testsRoot }, (err, files: ReadonlyArray<string>) => {
            if (err) {
                return reject(err);
            }

            // Add each file to the test suite.
            files.forEach(f => suite.addFile(path.resolve(testsRoot, f)));

            // Run the test suite. Uncaught exceptions or a non-zero number of
            // test rejectures is considered a test suite rejecture.
            try {
                return suite.run(failures => {
                    if (failures > 0) {
                        reject(new Error(`${failures} tests failed.`));
                    } else {
                        resolve();
                    }
                });
            } catch (err: unknown) {
                console.error(err);
                return reject(err);
            }
        });
    });
}
