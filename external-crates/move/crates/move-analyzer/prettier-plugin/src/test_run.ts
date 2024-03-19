// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import * as prettier from 'prettier'
import * as plugin from '.'
import { readFileSync } from 'fs'
import { stdout } from 'process'
import { basename } from 'path'

function usage(): void {
    stdout.write('\nUsage:\n\n')
    stdout.write(`    node ${basename(__filename)} move_file\n\n`)
}

if (process.argv.length !== 3) {
    usage();
    process.exit(1);
}

const filepath = process.argv[2]

if (filepath != null) {
    const text = readFileSync(filepath, { encoding: 'utf8'});
    const result = prettier.format(text, {
        plugins: [plugin],
        parser: 'move-parse',
    })
    result.then((response) => console.log(response));
} else {
    usage();
    process.exit(1);
}
