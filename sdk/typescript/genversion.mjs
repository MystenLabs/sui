// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { writeFile } from 'fs/promises';
const LICENSE =
  '// Copyright (c) Mysten Labs, Inc.\n// SPDX-License-Identifier: Apache-2.0\n\n';

import pkg from './package.json' assert { type: 'json' };

async function main() {
  await writeFile(
    'src/pkg-version.ts',
    LICENSE + `export const version = '${pkg.version}';\n`,
  );
}

await main();
