// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const { promisify } = require('node:util');

const { name } = require('../package.json');

const execFile = promisify(require('node:child_process').execFile);

const doNotBuild = () => {
    console.log('Disabling build.');
    process.exit(0);
};

const requiresBuild = () => {
    console.log('Changes detected, requesting build.');
    process.exit(1);
};

async function main() {
    const { stdout, stderr } = await execFile('pnpm', [
        'list',
        '--filter',
        '...[origin/main]',
        '--depth',
        '-1',
        '--json',
    ]);

    if (!stdout || stderr) {
        return doNotBuild();
    }

    const packages = JSON.parse(stdout);
    const explorerHasChanges = packages.find((pkg) => pkg.name === name);

    if (explorerHasChanges) {
        return requiresBuild();
    } else {
        return doNotBuild();
    }
}

main().catch((e) => {
    // In the case of an error, play it safe and build:
    console.error('Vercel Ignored Build Step Failed', e);
    doNotBuild();
});
