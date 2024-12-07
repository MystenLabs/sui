#!/usr/bin/env node

// use the
const path = require('path');
const plugin_path = path.resolve(__dirname, '..', 'out', 'index.js');
const child_process = require('child_process');

// command is prettier + plugin path + args passed to the script
const args = process.argv.slice(2);

// check that prettier is installed
try {
    child_process.execFileSync('prettier', ['--version']);
} catch (e) {
    console.error('Prettier is not installed. Please install it by running `npm install -g prettier`.');
    process.exit(1);
}

// run prettier, print the output and exit with correct code
const prettier = child_process.execFile(
	'prettier',
	args.length ? ['--plugin', plugin_path, ...args] : ['--help'],
);

// additionally, exchange stdin/stdout/stderr with the prettier process
process.stdin.pipe(prettier.stdin);
prettier.stdout.pipe(process.stdout);
prettier.stderr.pipe(process.stderr);
prettier.on('exit', (code) => process.exit(code));
