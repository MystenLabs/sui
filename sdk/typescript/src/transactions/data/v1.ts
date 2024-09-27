// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromBase64, toBase64 } from '@mysten/bcs';
import type { GenericSchema, InferInput, InferOutput } from 'valibot';
import {
	array,
	bigint,
	boolean,
	check,
	integer,
	is,
	lazy,
	literal,
	nullable,
	nullish,
	number,
	object,
	optional,
	parse,
	pipe,
	string,
	union,
	unknown,
} from 'valibot';

import { TypeTagSerializer } from '../../bcs/index.js';
import type { StructTag as StructTagType, TypeTag as TypeTagType } from '../../bcs/types.js';
import { JsonU64, ObjectID, safeEnum, TransactionData } from './internal.js';
import type { Argument } from './internal.js';

export const ObjectRef = object({
	digest: string(),
	objectId: string(),
	version: union([pipe(number(), integer()), string(), bigint()]),
});

const ObjectArg = safeEnum({
	ImmOrOwned: ObjectRef,
	Shared: object({
		objectId: ObjectID,
		initialSharedVersion: JsonU64,
		mutable: boolean(),
	}),
	Receiving: ObjectRef,
});

export const NormalizedCallArg = safeEnum({
	Object: ObjectArg,
	Pure: array(pipe(number(), integer())),
});

const TransactionInput = union([
	object({
		kind: literal('Input'),
		index: pipe(number(), integer()),
		value: unknown(),
		type: optional(literal('object')),
	}),
	object({
		kind: literal('Input'),
		index: pipe(number(), integer()),
		value: unknown(),
		type: literal('pure'),
	}),
]);

const TransactionExpiration = union([
	object({ Epoch: pipe(number(), integer()) }),
	object({ None: nullable(literal(true)) }),
]);

const StringEncodedBigint = pipe(
	union([number(), string(), bigint()]),
	check((val) => {
		if (!['string', 'number', 'bigint'].includes(typeof val)) return false;

		try {
			BigInt(val as string);
			return true;
		} catch {
			return false;
		}
	}),
);

export const TypeTag: GenericSchema<TypeTagType> = union([
	object({ bool: nullable(literal(true)) }),
	object({ u8: nullable(literal(true)) }),
	object({ u64: nullable(literal(true)) }),
	object({ u128: nullable(literal(true)) }),
	object({ address: nullable(literal(true)) }),
	object({ signer: nullable(literal(true)) }),
	object({ vector: lazy(() => TypeTag) }),
	object({ struct: lazy(() => StructTag) }),
	object({ u16: nullable(literal(true)) }),
	object({ u32: nullable(literal(true)) }),
	object({ u256: nullable(literal(true)) }),
]);

// https://github.com/MystenLabs/sui/blob/cea8742e810142a8145fd83c4c142d61e561004a/external-crates/move/crates/move-core-types/src/language_storage.rs#L140-L147
export const StructTag: GenericSchema<StructTagType> = object({
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
	TransactionInput,
	object({ kind: literal('GasCoin') }),
	object({ kind: literal('Result'), index: pipe(number(), integer()) }),
	object({
		kind: literal('NestedResult'),
		index: pipe(number(), integer()),
		resultIndex: pipe(number(), integer()),
	}),
] as const;

// Generic transaction argument
export const TransactionArgument = union([...TransactionArgumentTypes]);

const MoveCallTransaction = object({
	kind: literal('MoveCall'),
	target: pipe(
		string(),
		check((target) => target.split('::').length === 3),
	) as GenericSchema<`${string}::${string}::${string}`>,
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
	modules: array(array(pipe(number(), integer()))),
	dependencies: array(string()),
});

const UpgradeTransaction = object({
	kind: literal('Upgrade'),
	modules: array(array(pipe(number(), integer()))),
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

export const SerializedTransactionDataV1 = object({
	version: literal(1),
	sender: optional(string()),
	expiration: nullish(TransactionExpiration),
	gasConfig: GasConfig,
	inputs: array(TransactionInput),
	transactions: array(TransactionType),
});

export type SerializedTransactionDataV1 = InferOutput<typeof SerializedTransactionDataV1>;

export function serializeV1TransactionData(
	transactionData: TransactionData,
): SerializedTransactionDataV1 {
	const inputs: InferOutput<typeof TransactionInput>[] = transactionData.inputs.map(
		(input, index) => {
			if (input.Object) {
				return {
					kind: 'Input',
					index,
					value: {
						Object: input.Object.ImmOrOwnedObject
							? {
									ImmOrOwned: input.Object.ImmOrOwnedObject,
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
										Shared: {
											mutable: input.Object.SharedObject.mutable,
											initialSharedVersion: input.Object.SharedObject.initialSharedVersion,
											objectId: input.Object.SharedObject.objectId,
										},
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
						Pure: Array.from(fromBase64(input.Pure.bytes)),
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
		},
	);

	return {
		version: 1,
		sender: transactionData.sender ?? undefined,
		expiration:
			transactionData.expiration?.$kind === 'Epoch'
				? { Epoch: Number(transactionData.expiration.Epoch) }
				: transactionData.expiration
					? { None: true }
					: null,
		gasConfig: {
			owner: transactionData.gasData.owner ?? undefined,
			budget: transactionData.gasData.budget ?? undefined,
			price: transactionData.gasData.price ?? undefined,
			payment: transactionData.gasData.payment ?? undefined,
		},
		inputs,
		transactions: transactionData.commands.map((command): InferOutput<typeof TransactionType> => {
			if (command.MakeMoveVec) {
				return {
					kind: 'MakeMoveVec',
					type:
						command.MakeMoveVec.type === null
							? { None: true }
							: { Some: TypeTagSerializer.parseFromStr(command.MakeMoveVec.type) },
					objects: command.MakeMoveVec.elements.map((arg) =>
						convertTransactionArgument(arg, inputs),
					),
				};
			}
			if (command.MergeCoins) {
				return {
					kind: 'MergeCoins',
					destination: convertTransactionArgument(command.MergeCoins.destination, inputs),
					sources: command.MergeCoins.sources.map((arg) => convertTransactionArgument(arg, inputs)),
				};
			}
			if (command.MoveCall) {
				return {
					kind: 'MoveCall',
					target: `${command.MoveCall.package}::${command.MoveCall.module}::${command.MoveCall.function}`,
					typeArguments: command.MoveCall.typeArguments,
					arguments: command.MoveCall.arguments.map((arg) =>
						convertTransactionArgument(arg, inputs),
					),
				};
			}
			if (command.Publish) {
				return {
					kind: 'Publish',
					modules: command.Publish.modules.map((mod) => Array.from(fromBase64(mod))),
					dependencies: command.Publish.dependencies,
				};
			}
			if (command.SplitCoins) {
				return {
					kind: 'SplitCoins',
					coin: convertTransactionArgument(command.SplitCoins.coin, inputs),
					amounts: command.SplitCoins.amounts.map((arg) => convertTransactionArgument(arg, inputs)),
				};
			}
			if (command.TransferObjects) {
				return {
					kind: 'TransferObjects',
					objects: command.TransferObjects.objects.map((arg) =>
						convertTransactionArgument(arg, inputs),
					),
					address: convertTransactionArgument(command.TransferObjects.address, inputs),
				};
			}

			if (command.Upgrade) {
				return {
					kind: 'Upgrade',
					modules: command.Upgrade.modules.map((mod) => Array.from(fromBase64(mod))),
					dependencies: command.Upgrade.dependencies,
					packageId: command.Upgrade.package,
					ticket: convertTransactionArgument(command.Upgrade.ticket, inputs),
				};
			}

			throw new Error(`Unknown transaction ${Object.keys(command)}`);
		}),
	};
}

function convertTransactionArgument(
	arg: Argument,
	inputs: InferOutput<typeof TransactionInput>[],
): InferOutput<typeof TransactionArgument> {
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

export function transactionDataFromV1(data: SerializedTransactionDataV1): TransactionData {
	return parse(TransactionData, {
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
						if (value.Object.ImmOrOwned) {
							return {
								Object: {
									ImmOrOwnedObject: {
										objectId: value.Object.ImmOrOwned.objectId,
										version: String(value.Object.ImmOrOwned.version),
										digest: value.Object.ImmOrOwned.digest,
									},
								},
							};
						}
						if (value.Object.Shared) {
							return {
								Object: {
									SharedObject: {
										mutable: value.Object.Shared.mutable ?? null,
										initialSharedVersion: value.Object.Shared.initialSharedVersion,
										objectId: value.Object.Shared.objectId,
									},
								},
							};
						}
						if (value.Object.Receiving) {
							return {
								Object: {
									Receiving: {
										digest: value.Object.Receiving.digest,
										version: String(value.Object.Receiving.version),
										objectId: value.Object.Receiving.objectId,
									},
								},
							};
						}

						throw new Error('Invalid object input');
					}

					return {
						Pure: {
							bytes: toBase64(new Uint8Array(value.Pure)),
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
		commands: data.transactions.map((transaction) => {
			switch (transaction.kind) {
				case 'MakeMoveVec':
					return {
						MakeMoveVec: {
							type:
								'Some' in transaction.type
									? TypeTagSerializer.tagToString(transaction.type.Some)
									: null,
							elements: transaction.objects.map((arg) => parseV1TransactionArgument(arg)),
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
							modules: transaction.modules.map((mod) => toBase64(Uint8Array.from(mod))),
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
							modules: transaction.modules.map((mod) => toBase64(Uint8Array.from(mod))),
							dependencies: transaction.dependencies,
							package: transaction.packageId,
							ticket: parseV1TransactionArgument(transaction.ticket),
						},
					};
				}
			}

			throw new Error(`Unknown transaction ${Object.keys(transaction)}`);
		}),
	} satisfies InferInput<typeof TransactionData>);
}

function parseV1TransactionArgument(
	arg: InferOutput<typeof TransactionArgument>,
): InferInput<typeof Argument> {
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
