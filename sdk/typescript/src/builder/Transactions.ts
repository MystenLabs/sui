// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64 } from '@mysten/bcs';
import type { Infer, Struct } from 'superstruct';
import {
	any,
	array,
	assert,
	define,
	integer,
	is,
	literal,
	object,
	optional,
	record,
	string,
	union,
	unknown,
} from 'superstruct';

import type { TypeTag } from '../bcs/index.js';
import { bcs } from '../bcs/index.js';
import { TypeTagSerializer } from '../bcs/type-tag-serializer.js';
import { normalizeSuiObjectId } from '../utils/sui-types.js';
import { Inputs } from './Inputs.js';
import { create } from './utils.js';

const option = <T extends Struct<any, any>>(some: T) =>
	union([object({ None: union([literal(true), literal(null)]) }), object({ Some: some })]);

export const TransactionBlockInput = union([
	object({
		kind: literal('Input'),
		index: integer(),
		value: optional(any()),
		type: optional(literal('object')),
	}),
	object({
		kind: literal('Input'),
		index: integer(),
		value: optional(any()),
		type: literal('pure'),
	}),
]);
export type TransactionBlockInput = Infer<typeof TransactionBlockInput>;

const TransactionArgumentTypes = [
	TransactionBlockInput,
	object({ kind: literal('GasCoin') }),
	object({ kind: literal('Result'), index: integer() }),
	object({
		kind: literal('NestedResult'),
		index: integer(),
		resultIndex: integer(),
	}),
] as const;

// Generic transaction argument
export const TransactionArgument = union([...TransactionArgumentTypes]);
export type TransactionArgument = Infer<typeof TransactionArgument>;

export const MoveCallTransaction = object({
	kind: literal('MoveCall'),
	target: define<`${string}::${string}::${string}`>('target', string().validator),
	typeArguments: array(string()),
	arguments: array(TransactionArgument),
});
export type MoveCallTransaction = Infer<typeof MoveCallTransaction>;

export const TransferObjectsTransaction = object({
	kind: literal('TransferObjects'),
	objects: array(TransactionArgument),
	address: TransactionArgument,
});
export type TransferObjectsTransaction = Infer<typeof TransferObjectsTransaction>;

export const SplitCoinsTransaction = object({
	kind: literal('SplitCoins'),
	coin: TransactionArgument,
	amounts: array(TransactionArgument),
});
export type SplitCoinsTransaction = Infer<typeof SplitCoinsTransaction>;

export const MergeCoinsTransaction = object({
	kind: literal('MergeCoins'),
	destination: TransactionArgument,
	sources: array(TransactionArgument),
});
export type MergeCoinsTransaction = Infer<typeof MergeCoinsTransaction>;

export const MakeMoveVecTransaction = object({
	kind: literal('MakeMoveVec'),
	// TODO: ideally we should use `TypeTag` instead of `record()` here,
	// but TypeTag is recursively defined and it's tricky to define a
	// recursive struct in superstruct
	type: optional(option(record(string(), unknown()))) as never as Struct<
		{ Some: TypeTag } | { None: true | null }
	>,
	objects: array(TransactionArgument),
});
export type MakeMoveVecTransaction = Infer<typeof MakeMoveVecTransaction>;

export const PublishTransaction = object({
	kind: literal('Publish'),
	modules: array(array(integer())),
	dependencies: array(string()),
});
export type PublishTransaction = Infer<typeof PublishTransaction>;

// Keep in sync with constants in
// crates/sui-framework/packages/sui-framework/sources/package.move
export enum UpgradePolicy {
	COMPATIBLE = 0,
	ADDITIVE = 128,
	DEP_ONLY = 192,
}

export const UpgradeTransaction = object({
	kind: literal('Upgrade'),
	modules: array(array(integer())),
	dependencies: array(string()),
	packageId: string(),
	ticket: TransactionArgument,
});
export type UpgradeTransaction = Infer<typeof UpgradeTransaction>;

const TransactionTypes = [
	MoveCallTransaction,
	TransferObjectsTransaction,
	SplitCoinsTransaction,
	MergeCoinsTransaction,
	PublishTransaction,
	UpgradeTransaction,
	MakeMoveVecTransaction,
] as const;

export const TransactionType = union([...TransactionTypes]);
export type TransactionType = Infer<typeof TransactionType>;

export function getTransactionType(data: unknown) {
	assert(data, TransactionType);
	return TransactionTypes.find((schema) => is(data, schema as Struct))!;
}

/**
 * Simple helpers used to construct transactions:
 */
export const Transactions = {
	MoveCall(
		input: Omit<MoveCallTransaction, 'kind' | 'arguments' | 'typeArguments'> & {
			arguments?: TransactionArgument[];
			typeArguments?: string[];
		},
	): MoveCallTransaction {
		return create(
			{
				kind: 'MoveCall',
				target: input.target,
				arguments: input.arguments ?? [],
				typeArguments: input.typeArguments ?? [],
			},
			MoveCallTransaction,
		);
	},
	TransferObjects(
		objects: TransactionArgument[],
		address: TransactionArgument,
	): TransferObjectsTransaction {
		if (address.kind === 'Input' && address.type === 'pure' && typeof address.value !== 'object') {
			address.value = Inputs.Pure(bcs.Address.serialize(address.value));
		}

		return create({ kind: 'TransferObjects', objects, address }, TransferObjectsTransaction);
	},
	SplitCoins(coin: TransactionArgument, amounts: TransactionArgument[]): SplitCoinsTransaction {
		// Handle deprecated usage of `Input.Pure(100)`
		amounts.forEach((input) => {
			if (input.kind === 'Input' && input.type === 'pure' && typeof input.value !== 'object') {
				input.value = Inputs.Pure(bcs.U64.serialize(input.value));
			}
		});

		return create(
			{
				kind: 'SplitCoins',
				coin,
				amounts,
			},
			SplitCoinsTransaction,
		);
	},
	MergeCoins(
		destination: TransactionArgument,
		sources: TransactionArgument[],
	): MergeCoinsTransaction {
		return create({ kind: 'MergeCoins', destination, sources }, MergeCoinsTransaction);
	},
	Publish({
		modules,
		dependencies,
	}: {
		modules: number[][] | string[];
		dependencies: string[];
	}): PublishTransaction {
		return create(
			{
				kind: 'Publish',
				modules: modules.map((module) =>
					typeof module === 'string' ? Array.from(fromB64(module)) : module,
				),
				dependencies: dependencies.map((dep) => normalizeSuiObjectId(dep)),
			},
			PublishTransaction,
		);
	},
	Upgrade({
		modules,
		dependencies,
		packageId,
		ticket,
	}: {
		modules: number[][] | string[];
		dependencies: string[];
		packageId: string;
		ticket: TransactionArgument;
	}): UpgradeTransaction {
		return create(
			{
				kind: 'Upgrade',
				modules: modules.map((module) =>
					typeof module === 'string' ? Array.from(fromB64(module)) : module,
				),
				dependencies: dependencies.map((dep) => normalizeSuiObjectId(dep)),
				packageId,
				ticket,
			},
			UpgradeTransaction,
		);
	},
	MakeMoveVec({
		type,
		objects,
	}: Omit<MakeMoveVecTransaction, 'kind' | 'type'> & {
		type?: string;
	}): MakeMoveVecTransaction {
		return create(
			{
				kind: 'MakeMoveVec',
				type: type ? { Some: TypeTagSerializer.parseFromStr(type) } : { None: null },
				objects,
			},
			MakeMoveVecTransaction,
		);
	},
};
