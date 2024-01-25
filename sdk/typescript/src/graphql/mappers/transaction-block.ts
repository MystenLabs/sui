// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64 } from '@mysten/bcs';

import { bcs } from '../../bcs/index.js';
import type {
	ExecutionStatus,
	SuiTransactionBlockResponse,
	SuiTransactionBlockResponseOptions,
} from '../../client/index.js';
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
				version: change?.outputState?.version as unknown as string, // RPC type is wrong here
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
								?.version as unknown as string, // RPC type is wrong here
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
					id: 'TODO' as never, // TODO: turn id into an object
					packageId: event.sendingModule?.package.address!,
					parsedJson: event.json ? JSON.parse(event.json) : undefined,
					sender: event.sender?.address,
					timestampMs: new Date(event.timestamp).getTime().toString(),
					transactionModule: 'TODO',
					type: toShortTypeString(event.type?.repr)!,
			  })) ?? []
			: undefined,
		rawTransaction: options?.showRawInput ? transactionBlock.rawTransaction : undefined,
		...(options?.showInput
			? {
					transaction: transactionBlock.rawTransaction && {
						data: bcs.SenderSignedData.parse(fromB64(transactionBlock.rawTransaction))[0]
							.intentMessage.value.V1,
					},
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
									previousVersion: change?.inputState?.version.toString()!,
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
