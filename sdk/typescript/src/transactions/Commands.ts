// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { toB64 } from '@mysten/bcs';
import type { Input } from 'valibot';
import { parse } from 'valibot';

import { normalizeSuiObjectId } from '../utils/sui-types.js';
import { Argument } from './data/internal.js';
import type { CallArg, Command } from './data/internal.js';
import type { Transaction } from './Transaction.js';

export type TransactionArgument =
	| Input<typeof Argument>
	| ((tx: Transaction) => Input<typeof Argument>);
export type TransactionInput = CallArg;

// Keep in sync with constants in
// crates/sui-framework/packages/sui-framework/sources/package.move
export enum UpgradePolicy {
	COMPATIBLE = 0,
	ADDITIVE = 128,
	DEP_ONLY = 192,
}

type TransactionShape<T extends Command['$kind']> = { $kind: T } & {
	[K in T]: Extract<Command, { [K in T]: any }>[T];
};

/**
 * Simple helpers used to construct transactions:
 */
export const Commands = {
	MoveCall(
		input:
			| {
					package: string;
					module: string;
					function: string;
					arguments?: Argument[];
					typeArguments?: string[];
			  }
			| {
					target: string;
					arguments?: Argument[];
					typeArguments?: string[];
			  },
	): TransactionShape<'MoveCall'> {
		const [pkg, mod = '', fn = ''] =
			'target' in input ? input.target.split('::') : [input.package, input.module, input.function];

		return {
			$kind: 'MoveCall',
			MoveCall: {
				package: pkg,
				module: mod,
				function: fn,
				typeArguments: input.typeArguments ?? [],
				arguments: input.arguments ?? [],
			},
		};
	},

	TransferObjects(
		objects: Input<typeof Argument>[],
		address: Input<typeof Argument>,
	): TransactionShape<'TransferObjects'> {
		return {
			$kind: 'TransferObjects',
			TransferObjects: {
				objects: objects.map((o) => parse(Argument, o)),
				address: parse(Argument, address),
			},
		};
	},
	SplitCoins(
		coin: Input<typeof Argument>,
		amounts: Input<typeof Argument>[],
	): TransactionShape<'SplitCoins'> {
		return {
			$kind: 'SplitCoins',
			SplitCoins: {
				coin: parse(Argument, coin),
				amounts: amounts.map((o) => parse(Argument, o)),
			},
		};
	},
	MergeCoins(
		destination: Input<typeof Argument>,
		sources: Input<typeof Argument>[],
	): TransactionShape<'MergeCoins'> {
		return {
			$kind: 'MergeCoins',
			MergeCoins: {
				destination: parse(Argument, destination),
				sources: sources.map((o) => parse(Argument, o)),
			},
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
			Publish: {
				modules: modules.map((module) =>
					typeof module === 'string' ? module : toB64(new Uint8Array(module)),
				),
				dependencies: dependencies.map((dep) => normalizeSuiObjectId(dep)),
			},
		};
	},
	Upgrade({
		modules,
		dependencies,
		package: packageId,
		ticket,
	}: {
		modules: number[][] | string[];
		dependencies: string[];
		package: string;
		ticket: Input<typeof Argument>;
	}): TransactionShape<'Upgrade'> {
		return {
			$kind: 'Upgrade',
			Upgrade: {
				modules: modules.map((module) =>
					typeof module === 'string' ? module : toB64(new Uint8Array(module)),
				),
				dependencies: dependencies.map((dep) => normalizeSuiObjectId(dep)),
				package: packageId,
				ticket: parse(Argument, ticket),
			},
		};
	},
	MakeMoveVec({
		type,
		elements,
	}: {
		type?: string;
		elements: Input<typeof Argument>[];
	}): TransactionShape<'MakeMoveVec'> {
		return {
			$kind: 'MakeMoveVec',
			MakeMoveVec: {
				type: type ?? null,
				elements: elements.map((o) => parse(Argument, o)),
			},
		};
	},
	Intent({
		name,
		inputs = {},
		data = {},
	}: {
		name: string;
		inputs?: Record<string, Input<typeof Argument> | Input<typeof Argument>[]>;
		data?: Record<string, unknown>;
	}): TransactionShape<'$Intent'> {
		return {
			$kind: '$Intent',
			$Intent: {
				name,
				inputs: Object.fromEntries(
					Object.entries(inputs).map(([key, value]) => [
						key,
						Array.isArray(value) ? value.map((o) => parse(Argument, o)) : parse(Argument, value),
					]),
				),
				data,
			},
		};
	},
};
