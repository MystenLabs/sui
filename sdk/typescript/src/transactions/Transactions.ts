// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64 } from '@mysten/bcs';
import { parse } from 'valibot';
import type { Input } from 'valibot';

import { TypeTagSerializer } from '../bcs/type-tag-serializer.js';
import { normalizeSuiObjectId } from '../utils/sui-types.js';
import type { CallArg, Transaction } from './blockData/v2.js';
import { Argument, TypeTag } from './blockData/v2.js';

export type { Argument as TransactionArgument };
export type { CallArg as TransactionBlockInput };

// Keep in sync with constants in
// crates/sui-framework/packages/sui-framework/sources/package.move
export enum UpgradePolicy {
	COMPATIBLE = 0,
	ADDITIVE = 128,
	DEP_ONLY = 192,
}

type TransactionShape<T extends Transaction['$kind']> = { $kind: T } & {
	[K in T]: Extract<Transaction, { [K in T]: any }>[T];
};

/**
 * Simple helpers used to construct transactions:
 */
export const Transactions = {
	MoveCall(
		input:
			| {
					package: string;
					module: string;
					function: string;
					arguments?: Argument[];
					typeArguments?: (string | TypeTag)[];
			  }
			| {
					target: string;
					arguments?: Argument[];
					typeArguments?: (string | TypeTag)[];
			  },
	): TransactionShape<'MoveCall'> {
		const [pkg, mod, fn] =
			'target' in input ? input.target.split('::') : [input.package, input.module, input.function];

		return {
			$kind: 'MoveCall',
			MoveCall: {
				package: pkg,
				module: mod,
				function: fn,
				typeArguments:
					input.typeArguments?.map((arg) =>
						typeof arg === 'string' ? parse(TypeTag, TypeTagSerializer.parseFromStr(arg)) : arg,
					) ?? [],
				arguments: input.arguments ?? [],
			},
		};
	},

	TransferObjects(
		objects: Input<typeof Argument>[],
		address: Input<typeof Argument>,
	): TransactionShape<'TransferObjects'> {
		// TODO: arguments aren't linked to inputs anymore, so we need to handle this somewhere else
		// if (address.kind === 'Input' && address.type === 'pure' && typeof address.value !== 'object') {
		// 	address.value = Inputs.Pure(bcs.Address.serialize(address.value));
		// }

		return {
			$kind: 'TransferObjects',
			TransferObjects: [objects.map((o) => parse(Argument, o)), parse(Argument, address)],
		};
	},
	SplitCoins(
		coin: Input<typeof Argument>,
		amounts: Input<typeof Argument>[],
	): TransactionShape<'SplitCoins'> {
		// TODO: arguments aren't linked to inputs anymore, so we need to handle this somewhere else
		// Handle deprecated usage of `Input.Pure(100)`
		// amounts.forEach((input) => {
		// 	if (input.kind === 'Input' && input.type === 'pure' && typeof input.value !== 'object') {
		// 		input.value = Inputs.Pure(bcs.U64.serialize(input.value));
		// 	}
		// });

		return {
			$kind: 'SplitCoins',
			SplitCoins: [parse(Argument, coin), amounts.map((o) => parse(Argument, o))],
		};
	},
	MergeCoins(
		destination: Input<typeof Argument>,
		sources: Input<typeof Argument>[],
	): TransactionShape<'MergeCoins'> {
		return {
			$kind: 'MergeCoins',
			MergeCoins: [parse(Argument, destination), sources.map((o) => parse(Argument, o))],
		};
	},
	Publish({
		modules,
		dependencies,
	}: {
		modules: number[][] | string[];
		dependencies: string[];
	}): TransactionShape<'Publish'> {
		return {
			$kind: 'Publish',
			Publish: [
				modules.map((module) =>
					typeof module === 'string' ? Array.from(fromB64(module)) : module,
				),
				dependencies.map((dep) => normalizeSuiObjectId(dep)),
			],
		};
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
		ticket: Input<typeof Argument>;
	}): TransactionShape<'Upgrade'> {
		return {
			$kind: 'Upgrade',
			Upgrade: [
				modules.map((module) =>
					typeof module === 'string' ? Array.from(fromB64(module)) : module,
				),
				dependencies.map((dep) => normalizeSuiObjectId(dep)),
				packageId,
				parse(Argument, ticket),
			],
		};
	},
	MakeMoveVec({
		type,
		objects,
	}: {
		type?: string;
		objects: Input<typeof Argument>[];
	}): TransactionShape<'MakeMoveVec'> {
		return {
			$kind: 'MakeMoveVec',
			MakeMoveVec: [
				type
					? { $kind: 'Some', Some: parse(TypeTag, TypeTagSerializer.parseFromStr(type)) }
					: { $kind: 'None', None: true },
				objects.map((o) => parse(Argument, o)),
			],
		};
	},
};
