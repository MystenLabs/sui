// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
/* eslint-disable @typescript-eslint/ban-types */
/* eslint-disable no-restricted-globals */

import { existsSync, statSync } from 'fs';
import { mkdir, readdir, readFile, writeFile } from 'fs/promises';
import { relative, resolve } from 'path';
import { parseArgs } from 'util';
import { prompt } from 'enquirer';

const { values: args } = parseArgs({
	options: {
		template: {
			type: 'string',
			default: '',
			short: 't',
		},
	},
});

async function main() {
	const results = await prompt<{
		template: string;
		dAppName: string;
	}>(
		[
			{
				type: 'select',
				name: 'template',
				message: 'Which starter template would you like to use?',
				choices: [
					{
						name: 'react-client-dapp',
						hint: 'React Client dApp that reads data from wallet and the blockchain',
					},
					{
						name: 'react-e2e-counter',
						hint: 'React dApp with a move smart contract that implements a distributed counter',
					},
				],
			},
			{
				type: 'input',

				name: 'dAppName',
				message: 'What is the name of your dApp? (this will be used as the directory name)',
				initial: 'my-first-sui-dapp',
			},
		].filter((question) => !args[question.name as 'template']),
	);

	const outDir = resolve(process.cwd(), results.dAppName);

	if (existsSync(outDir)) {
		throw new Error(`Directory ${outDir} already exists`);
	}

	const files = await collectFiles(results.template ?? args.template, results.dAppName);
	await writeFiles(files, outDir);
}

main();

async function collectFiles(template: string, dAppName: string) {
	const dependencies = await getDependencyVersions();
	const templateDir = resolve(__dirname, '../templates', template);
	const files = new Array<{
		path: string;
		content: Buffer;
	}>();

	if (!statSync(templateDir).isDirectory()) {
		throw new Error(`Template ${templateDir} could not be found`);
	}

	await addDir(templateDir);

	return files;

	async function addDir(dir: string) {
		const entries = await readdir(dir);

		for (const entry of entries) {
			if (entry === 'node_modules') {
				continue;
			}
			const entryPath = resolve(dir, entry);
			const stat = statSync(entryPath);

			if (stat.isDirectory()) {
				await addDir(entryPath);
			} else {
				let content = await readFile(entryPath);

				if (entry === 'package.json') {
					const json = JSON.parse(content.toString());
					json.name = dAppName;
					json.dependencies['@mysten/sui'] = dependencies['@mysten/sui'];
					json.dependencies['@mysten/dapp-kit'] = dependencies['@mysten/dapp-kit'];

					content = Buffer.from(JSON.stringify(json, null, 2));
				}

				files.push({ path: relative(templateDir, entryPath), content });
			}
		}
	}
}

async function writeFiles(files: Array<{ path: string; content: Buffer }>, outDir: string) {
	for (const file of files) {
		const filePath = resolve(outDir, file.path);
		const dirPath = resolve(filePath, '..');
		if (!existsSync(dirPath)) {
			await mkdir(dirPath, { recursive: true });
		}

		await writeFile(filePath, file.content);
	}
}

async function getDependencyVersions() {
	const packagePath = resolve(__dirname, '../package.json');
	const content = JSON.parse(await readFile(packagePath, 'utf-8')) as {
		dependencies: Record<string, string>;
	};

	return content.dependencies;
}
