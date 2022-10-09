// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { readFile, writeFile } from 'fs/promises';
import { generate } from 'ts-auto-guard';

const GUARD_FILES = ['src/rpc/client.guard.ts', 'src/types/index.guard.ts'];
const LICENSE =
  '// Copyright (c) Mysten Labs, Inc.\n// SPDX-License-Identifier: Apache-2.0\n\n/* eslint-disable */\n\n';

async function main() {
  const tsconfig = new URL('./tsconfig.json', import.meta.url);

  // Change the directory to be the resolved directory of this file so that
  // the path resolution done in `generate` is guaranteed to work.
  process.chdir(new URL('.', import.meta.url).pathname);

  await generate({
    project: tsconfig.pathname,
    paths: ['src/rpc/client.ts', 'src/types/index.ts'],
    processOptions: {
      exportAll: true,
    },
  });

  await Promise.all(
    GUARD_FILES.map(async (fileName) => {
      const file = await readFile(fileName, 'utf-8');
      writeFile(
        fileName,
        LICENSE +
          file.replace(
            /import { BN } from ".*";\n/g,
            'import { BN } from "bn.js";\nimport { Buffer } from "buffer";\n'
          )
      );
    })
  );
}

await main();
