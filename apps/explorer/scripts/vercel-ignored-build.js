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

const ref =
    process.env.VERCEL === '1' && process.env.VERCEL_GIT_PREVIOUS_SHA
        ? process.env.VERCEL_GIT_PREVIOUS_SHA
        : 'HEAD^';

async function main() {
    // Run once without `--json` flag for better debugging.
    await execFile('pnpm', [
        'list',
        '--filter',
        `...[${ref}]`,
        '--depth',
        '-1',
    ]);

    const { stdout, stderr } = await execFile('pnpm', [
        'list',
        '--filter',
        `...[${ref}]`,
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
    console.log(e.message);
    // In the case of an error, play it safe and build:
    console.error('Vercel Ignored Build Step Failed', e);
    requiresBuild();
});
