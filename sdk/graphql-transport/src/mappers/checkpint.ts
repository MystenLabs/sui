// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Checkpoint, EndOfEpochData } from '@mysten/sui.js/client';

import type { Rpc_Checkpoint_FieldsFragment } from '../generated/queries.js';

export function mapGraphQLCheckpointToRpcCheckpoint(
	checkpoint: Rpc_Checkpoint_FieldsFragment,
): Checkpoint {
	const endOfEpochTx = checkpoint.endOfEpoch.nodes[0];
	let endOfEpochData: EndOfEpochData | undefined;

	if (
		endOfEpochTx?.kind?.__typename === 'EndOfEpochTransaction' &&
		endOfEpochTx.kind?.transactions.nodes[0].__typename === 'ChangeEpochTransaction'
	) {
		endOfEpochData = {
			epochCommitments: [], // TODO
			nextEpochCommittee:
				endOfEpochTx.kind.transactions.nodes[0].epoch?.validatorSet?.activeValidators?.nodes.map(
					(val) => [val.credentials?.protocolPubKey, val.votingPower?.toString()!],
				) ?? [],
			nextEpochProtocolVersion: String(
				endOfEpochTx.kind.transactions.nodes[0].epoch?.protocolConfigs.protocolVersion,
			),
		};
	}

	return {
		checkpointCommitments: [], // TODO
		digest: checkpoint.digest,
		endOfEpochData,
		epoch: String(checkpoint.epoch?.epochId),
		epochRollingGasCostSummary: {
			computationCost: checkpoint.rollingGasSummary?.computationCost,
			nonRefundableStorageFee: checkpoint.rollingGasSummary?.nonRefundableStorageFee,
			storageCost: checkpoint.rollingGasSummary?.storageCost,
			storageRebate: checkpoint.rollingGasSummary?.storageRebate,
		},
		networkTotalTransactions: String(checkpoint.networkTotalTransactions),
		...(checkpoint.previousCheckpointDigest
			? { previousDigest: checkpoint.previousCheckpointDigest }
			: {}),
		sequenceNumber: String(checkpoint.sequenceNumber),
		timestampMs: new Date(checkpoint.timestamp).getTime().toString(),
		transactions:
			checkpoint.transactionBlocks?.nodes.map((transactionBlock) => transactionBlock.digest!) ?? [],
		validatorSignature: checkpoint.validatorSignatures,
	};
}
