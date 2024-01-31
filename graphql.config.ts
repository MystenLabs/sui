// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { IGraphQLConfig } from 'graphql-config';

const config: IGraphQLConfig = {
	projects: {
		tsSDK: {
			schema: './crates/sui-graphql-rpc/schema/current_progress_schema.graphql',
			documents: ['./sdk/rpc-gql-compat/src/**/*.ts', './sdk/rpc-gql-compat/src/**/*.graphql'],
			include: ['./sdk/rpc-gql-compat/src/**/*.ts', './sdk/rpc-gql-compat/src/**/*.graphql'],
		},
	},
};

export default config;
