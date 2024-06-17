// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { execSync } from 'child_process';
import { readFile } from 'fs/promises';
import { mkdir, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';

const result = execSync(`git branch --remote --list "origin/releases/sui-graphql-rpc-v*"`)
	.toString()
	.trim()
	.split('\n')
	.map((ref) => {
		const branch = ref.trim().replace('origin/', '');
		const match = branch.match(/^releases\/sui-graphql-rpc-v([\d.]+)-release$/);

		if (!match) {
			return null;
		}

		const version = match[1];
		const [major, minor, patch] = version ? version.split('.') : [0, 0, 0];

		return match
			? {
					version: match[1],
					minorVersion: `${major}.${minor}`,
					major,
					minor,
					patch,
					branch,
					schema: `https://raw.githubusercontent.com/MystenLabs/sui/${branch}/crates/sui-graphql-rpc/schema.graphql`,
			  }
			: null;
	})
	.filter((x): x is NonNullable<typeof x> => x !== null);

const releasesByVersion = new Map<string, (typeof result)[number]>();
for (const release of result) {
	const { minorVersion } = release;
	const existing = releasesByVersion.get(minorVersion);
	if (!existing || existing.patch < release.patch) {
		releasesByVersion.set(minorVersion, release);
	}
}

for (const { minorVersion, schema } of releasesByVersion.values()) {
	const res = await fetch(schema);
	const schemaContent = await res.text();

	const filePath = resolve(
		import.meta.url.slice(5),
		`../../src/graphql/generated/${minorVersion}/schema.graphql`,
	);

	await mkdir(resolve(filePath, '..'), { recursive: true });
	await writeFile(filePath, schemaContent);

	await writeFile(
		resolve(filePath, '..', 'tsconfig.tada.json'),
		`
{
    "compilerOptions": {
        "plugins": [
            {
                "name": "@0no-co/graphqlsp",
                "schema": "./schema.graphql",
                "tadaOutputLocation": "src/graphql/generated/${minorVersion}/tada-env.d.ts"
            }
        ]
    }
}
`.trimStart(),
	);

	execSync(`pnpm run generate-schema -c ${resolve(filePath, '..', 'tsconfig.tada.json')}`);

	await mkdir(resolve(filePath, '../../../schemas', minorVersion), { recursive: true });
	await writeFile(
		resolve(filePath, `../../../schemas/${minorVersion}/index.ts`),
		`
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { initGraphQLTada } from 'gql.tada';

import type { introspection } from '../../generated/${minorVersion}/tada-env.js';
import type { CustomScalars } from '../../types.js';

export * from '../../types.js';

export type { FragmentOf, ResultOf, VariablesOf, TadaDocumentNode } from 'gql.tada';
export { readFragment, maskFragments } from 'gql.tada';

export const graphql = initGraphQLTada<{
	introspection: introspection;
	scalars: CustomScalars;
}>();
`.trimStart(),
	);
}

await addExportsToPackageJson(Array.from(releasesByVersion.keys()));

async function addExportsToPackageJson(versions: string[]) {
	const packageJsonPath = resolve(import.meta.url.slice(5), '../../package.json');
	const packageJson = JSON.parse(await readFile(packageJsonPath, 'utf-8'));

	for (const version of versions) {
		packageJson.exports[`./graphql/schemas/${version}`] = {
			import: `./dist/esm/graphql/schemas/${version}/index.js`,
			require: `./dist/cjs/graphql/schemas/${version}/index.js`,
		};
	}

	await writeFile(packageJsonPath, `${JSON.stringify(packageJson, null, '\t')}\n`);
}
