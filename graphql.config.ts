// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { IGraphQLConfig } from 'graphql-config';

const config: IGraphQLConfig = {
	projects: {
		tsSDK: {
			schema: './sdk/typescript/src/graphql/generated/2024.4/schema.graphql',
			documents: [
				'./sdk/graphql-transport/src/**/*.ts',
				'./sdk/graphql-transport/src/**/*.graphql',
			],
			include: ['./sdk/graphql-transport/src/**/*.ts', './sdk/graphql-transport/src/**/*.graphql'],
		},
	},
};

export default config;
