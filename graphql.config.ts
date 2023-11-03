// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { IGraphQLConfig } from 'graphql-config';

const config: IGraphQLConfig = {
	projects: {
		tsSDK: {
			schema: './crates/sui-graphql-rpc/schema/current_progress_schema.graphql',
			documents: [
				'./sdk/typescript/src/graphql/**/*.ts',
				'./sdk/typescript/src/graphql/**/*.graphql',
			],
			include: [
				'./sdk/typescript/src/graphql/**/*.ts',
				'./sdk/typescript/src/graphql/**/*.graphql',
			],
		},
	},
};

export default config;
