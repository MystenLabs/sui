"use strict";
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0
Object.defineProperty(exports, "__esModule", { value: true });
const prettier = require("prettier");
const plugin = require(".");
const fs_1 = require("fs");
const process_1 = require("process");
const path_1 = require("path");
function usage() {
    process_1.stdout.write('\nUsage:\n\n');
    process_1.stdout.write(`    node ${(0, path_1.basename)(__filename)} move_file\n\n`);
}
if (process.argv.length !== 3) {
    usage();
    process.exit(1);
}
const filepath = process.argv[2];
if (filepath != null) {
    const text = (0, fs_1.readFileSync)(filepath, { encoding: 'utf8' });
    const result = prettier.format(text, {
        plugins: [plugin],
        parser: 'move-parse',
    });
    result.then((response) => console.log(response));
}
else {
    usage();
    process.exit(1);
}
//# sourceMappingURL=test_run.js.map