// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT_DIR = path.join(fileURLToPath(new URL('.', import.meta.url)), '../svgs');

async function processDir(dirname) {
	const files = await fs.promises.readdir(dirname, {
		withFileTypes: true,
	});

	for (const file of files) {
		if (file.isFile()) {
			await fs.promises.rename(
				path.join(dirname, file.name),
				path.join(ROOT_DIR, file.name.trim()),
			);
		} else if (file.isDirectory()) {
			await processDir(path.join(dirname, file.name));
			await fs.promises.rmdir(path.join(dirname, file.name));
		}
	}
}

async function main() {
	await processDir(ROOT_DIR);
}

main().catch(console.error);
