// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { CodegenConfig } from '@graphql-codegen/cli';

const header = `
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
/* eslint-disable */
`.trimStart();

const config: CodegenConfig = {
	overwrite: true,
	schema: '../../crates/sui-graphql-rpc/schema/current_progress_schema.graphql',
	documents: ['src/graphql/queries/*.graphql'],
	ignoreNoDocuments: true,
	generates: {
		'src/graphql/generated/queries.ts': {
			// hooks: { afterOneFileWrite: ['prettier --write'] },
			plugins: [
				{
					add: {
						content: header,
					},
				},
				'typescript',
				'typescript-operations',
				{
					'typed-document-node': {
						documentMode: 'string',
					},
				},
			],
		},
	},
};

export default config;
