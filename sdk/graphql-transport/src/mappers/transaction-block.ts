// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64, toB64 } from '@mysten/bcs';
import { bcs, TypeTagSerializer } from '@mysten/sui.js/bcs';
import type {
	ExecutionStatus,
	SuiArgument,
	SuiCallArg,
	SuiTransaction,
	SuiTransactionBlock,
	SuiTransactionBlockKind,
	SuiTransactionBlockResponse,
	SuiTransactionBlockResponseOptions,
} from '@mysten/sui.js/client';

import type { Rpc_Transaction_FieldsFragment } from '../generated/queries.js';
import { mapGraphQLOwnerToRpcOwner } from './owner.js';
import { toShortTypeString } from './util.js';

export function mapGraphQLTransactionBlockToRpcTransactionBlock(
	transactionBlock: Rpc_Transaction_FieldsFragment,
	options?: SuiTransactionBlockResponseOptions | null,
	errors?: string[] | null,
): SuiTransactionBlockResponse {
	const deletedChanges = transactionBlock.effects?.objectChanges?.nodes
		?.filter((change) => change?.idDeleted === true)
		.map((change) => ({
			digest: change?.inputState?.digest!,
			version: String(change?.inputState?.version),
			objectId: change?.inputState?.address,
		}));
	const createdChanges = transactionBlock.effects?.objectChanges?.nodes
		?.filter((change) => change?.idCreated === true)
		.map((change) => ({
			owner: mapGraphQLOwnerToRpcOwner(change?.outputState?.owner)!,
			reference: {
				digest: change?.outputState?.digest!,
				version: change?.outputState?.version as unknown as string,
				objectId: change?.outputState?.address,
			},
		}));

	return {
		balanceChanges: transactionBlock.effects?.balanceChanges?.nodes.map((balanceChange) => ({
			amount: balanceChange?.amount,
			coinType: toShortTypeString(balanceChange?.coinType?.repr),
			owner: balanceChange.owner?.asObject?.address
				? {
						ObjectOwner: balanceChange.owner?.asObject?.address,
				  }
				: {
						AddressOwner: balanceChange.owner?.asAddress?.address!,
				  },
		})),
		...(typeof transactionBlock.effects?.checkpoint?.sequenceNumber === 'number'
			? { checkpoint: transactionBlock.effects.checkpoint.sequenceNumber.toString() }
			: {}),
		...(transactionBlock.effects?.timestamp
			? { timestampMs: new Date(transactionBlock.effects?.timestamp).getTime().toString() }
			: {}),
		digest: transactionBlock.digest!,
		effects: options?.showEffects
			? {
					...(createdChanges?.length ? { created: createdChanges } : {}),
					...(deletedChanges?.length ? { deleted: deletedChanges } : {}),
					dependencies: transactionBlock.effects?.dependencies?.nodes.map((dep) => dep?.digest!),
					executedEpoch: String(transactionBlock.effects?.executedEpoch?.epochId),
					gasObject: {
						owner: mapGraphQLOwnerToRpcOwner(
							transactionBlock.effects?.gasEffects?.gasObject?.owner,
						)!,
						reference: {
							digest: transactionBlock.effects?.gasEffects?.gasObject?.digest!,
							version: transactionBlock.effects?.gasEffects?.gasObject
								?.version as unknown as string,
							objectId: transactionBlock.effects?.gasEffects?.gasObject?.address,
						},
					},
					gasUsed: {
						computationCost: transactionBlock.effects?.gasEffects?.gasSummary?.computationCost,
						nonRefundableStorageFee:
							transactionBlock.effects?.gasEffects?.gasSummary?.nonRefundableStorageFee,
						storageCost: transactionBlock.effects?.gasEffects?.gasSummary?.storageCost,
						storageRebate: transactionBlock.effects?.gasEffects?.gasSummary?.storageRebate,
					},
					messageVersion: 'v1' as const,
					modifiedAtVersions: transactionBlock.effects?.objectChanges?.nodes
						?.filter((change) => !change?.idCreated && !change?.idDeleted)
						?.map((change) => ({
							objectId: change?.inputState?.address,
							sequenceNumber: String(change?.inputState?.version),
						})),
					mutated: transactionBlock.effects?.objectChanges?.nodes
						?.filter((change) => !change?.idCreated && !change?.idDeleted)
						?.map((change) => ({
							owner: mapGraphQLOwnerToRpcOwner(change?.outputState?.owner)!,
							reference: {
								digest: change?.outputState?.digest!,
								version: change?.outputState?.version as unknown as string,
								objectId: change?.outputState?.address,
							},
						})),

					status: { status: transactionBlock.effects?.status?.toLowerCase() } as ExecutionStatus,
					transactionDigest: transactionBlock.digest!,
					// sharedObjects: [], // TODO
					// unwrapped: [], // TODO
					// unwrappedThenDeleted: [], // TODO
					// wrapped: [], // TODO
			  }
			: undefined,
		...(errors ? { errors: errors } : {}),
		events: options?.showEvents
			? transactionBlock.effects?.events?.nodes.map((event) => ({
					bcs: event.bcs,
					id: {
						eventSeq: '', // TODO
						txDigest: '', // TODO
					},
					packageId: event.sendingModule?.package.address!,
					parsedJson: event.json ? JSON.parse(event.json) : undefined,
					sender: event.sender?.address,
					timestampMs: new Date(event.timestamp).getTime().toString(),
					transactionModule: `${event.sendingModule?.package.address}::${event.sendingModule?.name}`,
					type: toShortTypeString(event.type?.repr)!,
			  })) ?? []
			: undefined,
		rawTransaction: options?.showRawInput ? transactionBlock.rawTransaction : undefined,
		...(options?.showInput
			? {
					transaction:
						transactionBlock.rawTransaction &&
						mapTransactionBlockToInput(
							bcs.SenderSignedData.parse(fromB64(transactionBlock.rawTransaction))[0],
						),
			  }
			: {}),
		objectChanges: options?.showObjectChanges
			? transactionBlock.effects?.objectChanges?.nodes
					?.map((change) =>
						change?.idDeleted
							? {
									digest: change?.inputState?.digest!,
									objectId: change?.inputState?.address,
									owner: mapGraphQLOwnerToRpcOwner(change.inputState?.owner),
									objectType: toShortTypeString(
										change?.inputState?.asMoveObject?.contents?.type.repr,
									),
									sender: transactionBlock.sender?.address!,
									type: 'deleted' as const,
									version: change?.inputState?.version.toString()!,
							  }
							: {
									digest: change?.outputState?.digest!,
									objectId: change?.outputState?.address,
									owner: mapGraphQLOwnerToRpcOwner(change.outputState?.owner)!,
									objectType: toShortTypeString(
										change?.outputState?.asMoveObject?.contents?.type.repr,
									),
									...((typeof change?.inputState?.version === 'number'
										? { previousVersion: change?.inputState?.version.toString()! }
										: {}) as { previousVersion: string }),
									sender: transactionBlock.sender?.address,
									type: change?.idCreated ? ('created' as const) : ('mutated' as const),
									version: change?.outputState?.version.toString()!,
							  },
					)
					.sort((a, b) => {
						if (a.type === 'created' && b.type === 'deleted') {
							return -1;
						} else if (a.type === 'deleted' && b.type === 'created') {
							return 1;
						}
						return 0;
					})
			: undefined,
	};
}

export function mapTransactionBlockToInput(
	data: typeof bcs.SenderSignedTransaction.$inferType,
): SuiTransactionBlock | null {
	const txData = data.intentMessage.value.V1;

	const programableTransaction =
		'ProgrammableTransaction' in txData.kind ? txData.kind.ProgrammableTransaction : null;

	if (!programableTransaction) {
		return null;
	}

	return {
		txSignatures: data.txSignatures,
		data: {
			gasData: {
				budget: txData.gasData.budget,
				owner: txData.gasData.owner,
				payment: txData.gasData.payment.map((payment) => ({
					digest: payment.digest,
					objectId: payment.objectId,
					version: Number(payment.version) as never as string,
				})),
				price: txData.gasData.price,
			},
			messageVersion: 'v1',
			sender: txData.sender,
			transaction: mapProgramableTransaction(programableTransaction),
		},
	};
}

export function mapProgramableTransaction(
	programableTransaction: typeof bcs.ProgrammableTransaction.$inferType,
): SuiTransactionBlockKind {
	return {
		inputs: programableTransaction.inputs.map(mapTransactionInput),
		kind: 'ProgrammableTransaction',
		transactions: programableTransaction.transactions.map(mapTransaction),
	};
}

function mapTransactionInput(input: typeof bcs.CallArg.$inferType): SuiCallArg {
	if ('Pure' in input) {
		return {
			type: 'pure',
			value: Uint8Array.from(input.Pure),
		};
	}

	if ('Object' in input) {
		if ('ImmOrOwned' in input.Object) {
			return {
				type: 'object',
				digest: input.Object.ImmOrOwned.digest,
				version: input.Object.ImmOrOwned.version,
				objectId: input.Object.ImmOrOwned.objectId,
				objectType: 'immOrOwnedObject',
			};
		}
		if ('Shared' in input.Object) {
			return {
				type: 'object',
				initialSharedVersion: input.Object.Shared.initialSharedVersion,
				objectId: input.Object.Shared.objectId,
				mutable: input.Object.Shared.mutable,
				objectType: 'sharedObject',
			};
		}

		if ('Receiving' in input.Object) {
			return {
				type: 'object',
				digest: input.Object.Receiving.digest,
				version: input.Object.Receiving.version,
				objectId: input.Object.Receiving.objectId,
				objectType: 'receiving',
			};
		}

		throw new Error(`Unknown object type: ${input.Object}`);
	}

	throw new Error(`Unknown input type ${input}`);
}

function mapTransaction(transaction: typeof bcs.Transaction.$inferType): SuiTransaction {
	switch (transaction.kind) {
		case 'MoveCall': {
			const [pkg, module, fn] = transaction.target.split('::');
			return {
				MoveCall: {
					arguments: transaction.arguments.map(mapTransactionArgument),
					function: fn,
					module,
					package: pkg,
					type_arguments: transaction.typeArguments,
				},
			};
		}

		case 'MakeMoveVec': {
			return {
				MakeMoveVec: [
					'Some' in transaction.type ? TypeTagSerializer.tagToString(transaction.type.Some) : null,
					transaction.objects.map(mapTransactionArgument),
				],
			};
		}
		case 'MergeCoins': {
			return {
				MergeCoins: [
					mapTransactionArgument(transaction.destination),
					transaction.sources.map(mapTransactionArgument),
				],
			};
		}
		case 'Publish': {
			return {
				Publish: transaction.modules.map((module) => toB64(Uint8Array.from(module))),
			};
		}
		case 'SplitCoins': {
			return {
				SplitCoins: [
					mapTransactionArgument(transaction.coin),
					transaction.amounts.map(mapTransactionArgument),
				],
			};
		}
		case 'TransferObjects': {
			return {
				TransferObjects: [
					transaction.objects.map(mapTransactionArgument),
					mapTransactionArgument(transaction.address),
				],
			};
		}
		case 'Upgrade': {
			return {
				Upgrade: [
					transaction.modules.map((module) => toB64(Uint8Array.from(module))),
					transaction.packageId,
					mapTransactionArgument(transaction.ticket),
				],
			};
		}
	}

	throw new Error(`Unknown transaction type ${transaction}`);
}

function mapTransactionArgument(arg: typeof bcs.Argument.$inferType): SuiArgument {
	switch (arg.kind) {
		case 'GasCoin': {
			return 'GasCoin';
		}
		case 'Input': {
			return {
				Input: arg.index,
			};
		}
		case 'Result': {
			return {
				Result: arg.index,
			};
		}
		case 'NestedResult': {
			return {
				NestedResult: [arg.index, arg.resultIndex],
			};
		}
	}

	throw new Error(`Unknown argument type ${arg}`);
}
