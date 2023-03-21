// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { readFile, writeFile } from 'fs/promises';

const LICENSE =
  '// Copyright (c) Mysten Labs, Inc.\n// SPDX-License-Identifier: Apache-2.0\n\n';

async function main() {
  const pkg = JSON.parse(await readFile('./package.json', 'utf8'));
  await writeFile(
    'src/pkg-version.ts',
    LICENSE + `export const pkgVersion = '${pkg.version}';\n`,
  );
}

await main();
