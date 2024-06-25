// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { TransactionBlock } from '@mysten/sui/transactions';

import { CONFIG } from '../config';
import { getActiveAddress, signAndExecute } from '../sui-utils';

/// Demo PTB to cancel an escrow.
export const cancelEscrow = async (escrowId: string) => {
	const txb = new TransactionBlock();

	const bear = txb.moveCall({
		target: `${CONFIG.SWAP_CONTRACT.packageId}::shared::return_to_sender`,
		arguments: [txb.object(escrowId)],
		typeArguments: [`${CONFIG.DEMO_CONTRACT.packageId}::demo_bear::DemoBear`],
	});

	txb.transferObjects([bear], txb.pure.address(getActiveAddress()));

	await signAndExecute(txb, CONFIG.NETWORK);
};
