// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64, toB64 } from '@mysten/bcs';
import type { BaseSchema, Input, Output } from 'valibot';
import {
	array,
	bigint,
	custom,
	integer,
	is,
	literal,
	nullable,
	nullish,
	number,
	object,
	optional,
	parse,
	recursive,
	string,
	union,
	unknown,
} from 'valibot';

import { TypeTagSerializer } from '../../bcs/index.js';
import type { StructTag as StructTagType, TypeTag as TypeTagType } from '../../bcs/types.js';
import { ObjectArg, safeEnum, TransactionBlockData } from './internal.js';
import type { Argument } from './internal.js';

export const NormalizedCallArg = safeEnum({
	Object: ObjectArg,
	Pure: array(number([integer()])),
});

export const ObjectRef = object({
	digest: string(),
	objectId: string(),
	version: union([number([integer()]), string(), bigint()]),
});

const TransactionBlockInput = union([
	object({
		kind: literal('Input'),
		index: number([integer()]),
		value: unknown(),
		type: optional(literal('object')),
	}),
	object({
		kind: literal('Input'),
		index: number([integer()]),
		value: unknown(),
		type: literal('pure'),
	}),
]);

const TransactionExpiration = union([
	object({ Epoch: number([integer()]) }),
	object({ None: nullable(literal(true)) }),
]);

const StringEncodedBigint = union(
	[number(), string(), bigint()],
	[
		custom((val) => {
			if (!['string', 'number', 'bigint'].includes(typeof val)) return false;

			try {
				BigInt(val as string);
				return true;
			} catch {
				return false;
			}
		}),
	],
);

export const TypeTag: BaseSchema<TypeTagType> = union([
	object({ bool: nullable(literal(true)) }),
	object({ u8: nullable(literal(true)) }),
	object({ u64: nullable(literal(true)) }),
	object({ u128: nullable(literal(true)) }),
	object({ address: nullable(literal(true)) }),
	object({ signer: nullable(literal(true)) }),
	object({ vector: recursive(() => TypeTag) }),
	object({ struct: recursive(() => StructTag) }),
	object({ u16: nullable(literal(true)) }),
	object({ u32: nullable(literal(true)) }),
	object({ u256: nullable(literal(true)) }),
]);

// https://github.com/MystenLabs/sui/blob/cea8742e810142a8145fd83c4c142d61e561004a/external-crates/move/crates/move-core-types/src/language_storage.rs#L140-L147
export const StructTag: BaseSchema<StructTagType> = object({
	address: string(),
	module: string(),
	name: string(),
	typeParams: array(TypeTag),
});

const GasConfig = object({
	budget: optional(StringEncodedBigint),
	price: optional(StringEncodedBigint),
	payment: optional(array(ObjectRef)),
	owner: optional(string()),
});

const TransactionArgumentTypes = [
	TransactionBlockInput,
	object({ kind: literal('GasCoin') }),
	object({ kind: literal('Result'), index: number([integer()]) }),
	object({
		kind: literal('NestedResult'),
		index: number([integer()]),
		resultIndex: number([integer()]),
	}),
] as const;

// Generic transaction argument
export const TransactionArgument = union([...TransactionArgumentTypes]);

const MoveCallTransaction = object({
	kind: literal('MoveCall'),
	target: string([
		custom((target) => target.split('::').length === 3),
	]) as BaseSchema<`${string}::${string}::${string}`>,
	typeArguments: array(string()),
	arguments: array(TransactionArgument),
});

const TransferObjectsTransaction = object({
	kind: literal('TransferObjects'),
	objects: array(TransactionArgument),
	address: TransactionArgument,
});

const SplitCoinsTransaction = object({
	kind: literal('SplitCoins'),
	coin: TransactionArgument,
	amounts: array(TransactionArgument),
});

const MergeCoinsTransaction = object({
	kind: literal('MergeCoins'),
	destination: TransactionArgument,
	sources: array(TransactionArgument),
});

const MakeMoveVecTransaction = object({
	kind: literal('MakeMoveVec'),
	type: union([object({ Some: TypeTag }), object({ None: nullable(literal(true)) })]),
	objects: array(TransactionArgument),
});

const PublishTransaction = object({
	kind: literal('Publish'),
	modules: array(array(number([integer()]))),
	dependencies: array(string()),
});

const UpgradeTransaction = object({
	kind: literal('Upgrade'),
	modules: array(array(number([integer()]))),
	dependencies: array(string()),
	packageId: string(),
	ticket: TransactionArgument,
});

const TransactionTypes = [
	MoveCallTransaction,
	TransferObjectsTransaction,
	SplitCoinsTransaction,
	MergeCoinsTransaction,
	PublishTransaction,
	UpgradeTransaction,
	MakeMoveVecTransaction,
] as const;

const TransactionType = union([...TransactionTypes]);

export const SerializedTransactionBlockDataV1 = object({
	version: literal(1),
	sender: optional(string()),
	expiration: nullish(TransactionExpiration),
	gasConfig: GasConfig,
	inputs: array(TransactionBlockInput),
	transactions: array(TransactionType),
});

export type SerializedTransactionBlockDataV1 = Output<typeof SerializedTransactionBlockDataV1>;

export function serializeV1TransactionBlockData(
	blockData: TransactionBlockData,
): SerializedTransactionBlockDataV1 {
	const inputs: Output<typeof TransactionBlockInput>[] = blockData.inputs.map((input, index) => {
		if (input.Object) {
			return {
				kind: 'Input',
				index,
				value: input.Object.ImmOrOwnedObject
					? {
							ImmOrOwnedObject: input.Object.ImmOrOwnedObject,
					  }
					: input.Object.Receiving
					? {
							Receiving: {
								digest: input.Object.Receiving.digest,
								version: input.Object.Receiving.version,
								objectId: input.Object.Receiving.objectId,
							},
					  }
					: {
							SharedObject: {
								mutable: input.Object.SharedObject.mutable,
								initialSharedVersion: input.Object.SharedObject.initialSharedVersion,
								objectId: input.Object.SharedObject.objectId,
							},
					  },
				type: 'object',
			};
		}
		if (input.Pure) {
			return {
				kind: 'Input',
				index,
				value: {
					Pure: Array.from(fromB64(input.Pure.bytes)),
				},
				type: 'pure',
			};
		}

		if (input.UnresolvedPure) {
			return {
				kind: 'Input',
				type: 'pure',
				index,
				value: input.UnresolvedPure.value,
			};
		}

		if (input.UnresolvedObject) {
			return {
				kind: 'Input',
				type: 'object',
				index,
				value: input.UnresolvedObject.objectId,
			};
		}

		throw new Error('Invalid input');
	});

	return {
		version: 1,
		sender: blockData.sender ?? undefined,
		expiration:
			blockData.expiration?.$kind === 'Epoch'
				? { Epoch: Number(blockData.expiration.Epoch) }
				: blockData.expiration
				? { None: true }
				: null,
		gasConfig: {
			owner: blockData.gasData.owner ?? undefined,
			budget: blockData.gasData.budget ?? undefined,
			price: blockData.gasData.price ?? undefined,
			payment: blockData.gasData.payment ?? undefined,
		},
		inputs,
		transactions: blockData.transactions.map((transaction): Output<typeof TransactionType> => {
			if (transaction.MakeMoveVec) {
				return {
					kind: 'MakeMoveVec',
					type:
						transaction.MakeMoveVec.type === null
							? { None: true }
							: { Some: TypeTagSerializer.parseFromStr(transaction.MakeMoveVec.type) },
					objects: transaction.MakeMoveVec.objects.map((arg) =>
						convertTransactionArgument(arg, inputs),
					),
				};
			}
			if (transaction.MergeCoins) {
				return {
					kind: 'MergeCoins',
					destination: convertTransactionArgument(transaction.MergeCoins.destination, inputs),
					sources: transaction.MergeCoins.sources.map((arg) =>
						convertTransactionArgument(arg, inputs),
					),
				};
			}
			if (transaction.MoveCall) {
				return {
					kind: 'MoveCall',
					target: `${transaction.MoveCall.package}::${transaction.MoveCall.module}::${transaction.MoveCall.function}`,
					typeArguments: transaction.MoveCall.typeArguments,
					arguments: transaction.MoveCall.arguments.map((arg) =>
						convertTransactionArgument(arg, inputs),
					),
				};
			}
			if (transaction.Publish) {
				return {
					kind: 'Publish',
					modules: transaction.Publish.modules.map((mod) => Array.from(fromB64(mod))),
					dependencies: transaction.Publish.dependencies,
				};
			}
			if (transaction.SplitCoins) {
				return {
					kind: 'SplitCoins',
					coin: convertTransactionArgument(transaction.SplitCoins.coin, inputs),
					amounts: transaction.SplitCoins.amounts.map((arg) =>
						convertTransactionArgument(arg, inputs),
					),
				};
			}
			if (transaction.TransferObjects) {
				return {
					kind: 'TransferObjects',
					objects: transaction.TransferObjects.objects.map((arg) =>
						convertTransactionArgument(arg, inputs),
					),
					address: convertTransactionArgument(transaction.TransferObjects.address, inputs),
				};
			}

			if (transaction.Upgrade) {
				return {
					kind: 'Upgrade',
					modules: transaction.Upgrade.modules.map((mod) => Array.from(fromB64(mod))),
					dependencies: transaction.Upgrade.dependencies,
					packageId: transaction.Upgrade.package,
					ticket: convertTransactionArgument(transaction.Upgrade.ticket, inputs),
				};
			}

			throw new Error(`Unknown transaction ${Object.keys(transaction)}`);
		}),
	};
}

function convertTransactionArgument(
	arg: Argument,
	inputs: Output<typeof TransactionBlockInput>[],
): Output<typeof TransactionArgument> {
	if (arg.$kind === 'GasCoin') {
		return { kind: 'GasCoin' };
	}
	if (arg.$kind === 'Result') {
		return { kind: 'Result', index: arg.Result };
	}
	if (arg.$kind === 'NestedResult') {
		return { kind: 'NestedResult', index: arg.NestedResult[0], resultIndex: arg.NestedResult[1] };
	}
	if (arg.$kind === 'Input') {
		return inputs[arg.Input];
	}

	throw new Error(`Invalid argument ${Object.keys(arg)}`);
}

export function transactionBlockDataFromV1(
	data: SerializedTransactionBlockDataV1,
): TransactionBlockData {
	return parse(TransactionBlockData, {
		version: 2,
		sender: data.sender ?? null,
		expiration: data.expiration
			? 'Epoch' in data.expiration
				? { Epoch: data.expiration.Epoch }
				: { None: true }
			: null,
		gasData: {
			owner: data.gasConfig.owner ?? null,
			budget: data.gasConfig.budget?.toString() ?? null,
			price: data.gasConfig.price?.toString() ?? null,
			payment:
				data.gasConfig.payment?.map((ref) => ({
					digest: ref.digest,
					objectId: ref.objectId,
					version: ref.version.toString(),
				})) ?? null,
		},
		inputs: data.inputs.map((input) => {
			if (input.kind === 'Input') {
				if (is(NormalizedCallArg, input.value)) {
					const value = parse(NormalizedCallArg, input.value);

					if (value.Object) {
						return {
							Object: value.Object,
						};
					}

					return {
						Pure: {
							bytes: toB64(new Uint8Array(value.Pure)),
						},
					};
				}

				if (input.type === 'object') {
					return {
						UnresolvedObject: {
							objectId: input.value as string,
						},
					};
				}

				return {
					UnresolvedPure: {
						value: input.value,
					},
				};
			}

			throw new Error('Invalid input');
		}),
		transactions: data.transactions.map((transaction) => {
			switch (transaction.kind) {
				case 'MakeMoveVec':
					return {
						MakeMoveVec: {
							type:
								'Some' in transaction.type
									? TypeTagSerializer.tagToString(transaction.type.Some)
									: null,
							objects: transaction.objects.map((arg) => parseV1TransactionArgument(arg)),
						},
					};
				case 'MergeCoins': {
					return {
						MergeCoins: {
							destination: parseV1TransactionArgument(transaction.destination),
							sources: transaction.sources.map((arg) => parseV1TransactionArgument(arg)),
						},
					};
				}
				case 'MoveCall': {
					const [pkg, mod, fn] = transaction.target.split('::');
					return {
						MoveCall: {
							package: pkg,
							module: mod,
							function: fn,
							typeArguments: transaction.typeArguments,
							arguments: transaction.arguments.map((arg) => parseV1TransactionArgument(arg)),
						},
					};
				}
				case 'Publish': {
					return {
						Publish: {
							modules: transaction.modules.map((mod) => toB64(Uint8Array.from(mod))),
							dependencies: transaction.dependencies,
						},
					};
				}
				case 'SplitCoins': {
					return {
						SplitCoins: {
							coin: parseV1TransactionArgument(transaction.coin),
							amounts: transaction.amounts.map((arg) => parseV1TransactionArgument(arg)),
						},
					};
				}
				case 'TransferObjects': {
					return {
						TransferObjects: {
							objects: transaction.objects.map((arg) => parseV1TransactionArgument(arg)),
							address: parseV1TransactionArgument(transaction.address),
						},
					};
				}
				case 'Upgrade': {
					return {
						Upgrade: {
							modules: transaction.modules.map((mod) => toB64(Uint8Array.from(mod))),
							dependencies: transaction.dependencies,
							package: transaction.packageId,
							ticket: parseV1TransactionArgument(transaction.ticket),
						},
					};
				}
			}

			throw new Error(`Unknown transaction ${Object.keys(transaction)}`);
		}),
	} satisfies Input<typeof TransactionBlockData>);
}

function parseV1TransactionArgument(
	arg: Output<typeof TransactionArgument>,
): Input<typeof Argument> {
	switch (arg.kind) {
		case 'GasCoin': {
			return { GasCoin: true };
		}
		case 'Result':
			return { Result: arg.index };
		case 'NestedResult': {
			return { NestedResult: [arg.index, arg.resultIndex] };
		}
		case 'Input': {
			return { Input: arg.index };
		}
	}
}
