// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64 } from '@mysten/bcs';

import { TypeTagSerializer } from '../bcs/type-tag-serializer.js';
import { normalizeSuiObjectId } from '../utils/sui-types.js';
import type { Argument, Transaction, TypeTag } from './blockData/v2.js';

// Keep in sync with constants in
// crates/sui-framework/packages/sui-framework/sources/package.move
export enum UpgradePolicy {
	COMPATIBLE = 0,
	ADDITIVE = 128,
	DEP_ONLY = 192,
}

type TransactionKind = Transaction extends infer T ? (T extends unknown ? keyof T : never) : never;
type TransactionShape<T extends TransactionKind> = {
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
			MoveCall: {
				package: pkg,
				module: mod,
				function: fn,
				typeArguments:
					input.typeArguments?.map((arg) =>
						typeof arg === 'string' ? TypeTagSerializer.parseFromStr(arg) : arg,
					) ?? [],
				arguments: input.arguments ?? [],
			},
		};
	},

	TransferObjects(objects: Argument[], address: Argument): TransactionShape<'TransferObjects'> {
		// TODO: arguments aren't linked to inputs anymore, so we need to handle this somewhere else
		// if (address.kind === 'Input' && address.type === 'pure' && typeof address.value !== 'object') {
		// 	address.value = Inputs.Pure(bcs.Address.serialize(address.value));
		// }

		return {
			TransferObjects: [objects, address],
		};
	},
	SplitCoins(coin: Argument, amounts: Argument[]): TransactionShape<'SplitCoins'> {
		// TODO: arguments aren't linked to inputs anymore, so we need to handle this somewhere else
		// Handle deprecated usage of `Input.Pure(100)`
		// amounts.forEach((input) => {
		// 	if (input.kind === 'Input' && input.type === 'pure' && typeof input.value !== 'object') {
		// 		input.value = Inputs.Pure(bcs.U64.serialize(input.value));
		// 	}
		// });

		return {
			SplitCoins: [coin, amounts],
		};
	},
	MergeCoins(destination: Argument, sources: Argument[]): TransactionShape<'MergeCoins'> {
		return {
			MergeCoins: [destination, sources],
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
		ticket: Argument;
	}): TransactionShape<'Upgrade'> {
		return {
			Upgrade: [
				modules.map((module) =>
					typeof module === 'string' ? Array.from(fromB64(module)) : module,
				),
				dependencies.map((dep) => normalizeSuiObjectId(dep)),
				packageId,
				ticket,
			],
		};
	},
	MakeMoveVec({
		type,
		objects,
	}: {
		type?: string;
		objects: Argument[];
	}): TransactionShape<'MakeMoveVec'> {
		return {
			MakeMoveVec: [
				type ? { Some: TypeTagSerializer.parseFromStr(type) } : { None: null },
				objects,
			],
		};
	},
};
