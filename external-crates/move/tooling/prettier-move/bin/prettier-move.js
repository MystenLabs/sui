#!/usr/bin/env node

// use the
const path = require('path');
const plugin_path = path.resolve(__dirname, '..', 'out', 'index.js');
const child_process = require('child_process');

// command is prettier + plugin path + args passed to the script
const args = process.argv.slice(2);
const command = args.length == 0 ? 'prettier --help' : `prettier --plugin ${plugin_path} ${args.join(' ')}`;

// run prettier, print the output and exit with correct code
const prettier = child_process.exec(command, (error, stdout, stderr) => {
    if (error) {
        process.stderr.write(stderr);
        process.exit(1);
    }

    process.stdout.write(stdout);
    process.exit(0);
});

// additionally, if there's STDIN data, pass it to the command
process.stdin.pipe(prettier.stdin);
