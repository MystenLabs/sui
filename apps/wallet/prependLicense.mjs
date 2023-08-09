// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { readFile, writeFile } from 'fs/promises';

const LICENSE = '// Copyright (c) Mysten Labs, Inc.\n// SPDX-License-Identifier: Apache-2.0\n\n';

async function prependLicense(filename) {
	const content = await readFile(filename, 'utf8');
	writeFile(filename, LICENSE + content);
}

prependLicense('src/shared/analytics/ampli/index.ts');
