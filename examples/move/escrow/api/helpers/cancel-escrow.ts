// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { TransactionBlock } from '@mysten/sui.js/transactions';

import { CONFIG } from '../config';
import { getActiveAddress, signAndExecute } from '../sui-utils';

const cancelEscrow = async (escrowId: string) => {
	const txb = new TransactionBlock();

	const bear = txb.moveCall({
		target: `${CONFIG.SWAP_CONTRACT.packageId}::shared::return_to_sender`,
		arguments: [txb.object(escrowId)],
		typeArguments: [`${CONFIG.SWAP_CONTRACT.packageId}::demo_bear::DemoBear`],
	});

	txb.transferObjects([bear], txb.pure.address(getActiveAddress()));

	await signAndExecute(txb, CONFIG.NETWORK);
};

cancelEscrow('0x4f56f3c3cfcf28448a0b4f19d1abe08927d84a677b25dc31b1d8d1dfe1888bcf');
