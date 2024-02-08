// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { initGraphQLTada } from 'gql.tada';

import type { introspection } from '../../generated/2024-01/tada-env.js';

export const graphql = initGraphQLTada<{
	introspection: introspection;
}>();

export type { FragmentOf, ResultOf, VariablesOf } from 'gql.tada';
export { readFragment } from 'gql.tada';
