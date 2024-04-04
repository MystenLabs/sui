// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { initGraphQLTada } from 'gql.tada';

import type { introspection } from '../../generated/2024-01/tada-env.js';

export type MoveData =
	| { Address: string }
	| { UID: string }
	| { ID: string }
	| { Bool: boolean }
	| { Number: string }
	| { String: string }
	| { Vector: [MoveData] }
	| { Option: MoveData | null }
	| { Struct: [{ name: string; value: MoveData }] };

export type MoveTypeLayout =
	| 'address'
	| 'bool'
	| 'u8'
	| 'u16'
	| 'u32'
	| 'u64'
	| 'u128'
	| 'u256'
	| { vector: MoveTypeLayout }
	| {
			struct: {
				type: string;
				fields: [{ name: string; layout: MoveTypeLayout }];
			};
	  };

export type MoveTypeSignature =
	| 'address'
	| 'bool'
	| 'u8'
	| 'u16'
	| 'u32'
	| 'u64'
	| 'u128'
	| 'u256'
	| { vector: MoveTypeSignature }
	| {
			datatype: {
				package: string;
				module: string;
				type: string;
				typeParameters: [MoveTypeSignature];
			};
	  };

export type OpenMoveTypeSignature = {
	ref?: ('&' | '&mut') | null;
	body: OpenMoveTypeSignatureBody;
};

export type OpenMoveTypeSignatureBody =
	| 'address'
	| 'bool'
	| 'u8'
	| 'u16'
	| 'u32'
	| 'u64'
	| 'u128'
	| 'u256'
	| { vector: OpenMoveTypeSignatureBody }
	| {
			datatype: {
				package: string;
				module: string;
				type: string;
				typeParameters: [OpenMoveTypeSignatureBody];
			};
	  }
	| { typeParameter: number };

export const graphql = initGraphQLTada<{
	introspection: introspection;
	scalars: {
		BigInt: string;
		Base64: string;
		DateTime: string;
		JSON: unknown;
		MoveData: MoveData;
		MoveTypeLayout: MoveTypeLayout;
		MoveTypeSignature: MoveTypeSignature;
		OpenMoveTypeSignature: OpenMoveTypeSignature;
		SuiAddress: string;
	};
}>();

export type { FragmentOf, ResultOf, VariablesOf } from 'gql.tada';
export { readFragment } from 'gql.tada';
