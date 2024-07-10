// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import type { Transaction } from '@mysten/sui/transactions';

import type { BalanceManager, Pool } from '../types/index.js';
import { DEEP_SCALAR, DEEPBOOK_PACKAGE_ID, FLOAT_SCALAR } from '../utils/config.js';
import { generateProof } from './balanceManager.js';

export const stake = (
	pool: Pool,
	balanceManager: BalanceManager,
	stakeAmount: number,
	txb: Transaction,
) => {
	const tradeProof = generateProof(balanceManager, txb);

	txb.moveCall({
		target: `${DEEPBOOK_PACKAGE_ID}::pool::stake`,
		arguments: [
			txb.object(pool.address),
			txb.object(balanceManager.address),
			tradeProof,
			txb.pure.u64(stakeAmount * DEEP_SCALAR),
		],
		typeArguments: [pool.baseCoin.type, pool.quoteCoin.type],
	});
};

export const unstake = (pool: Pool, balanceManager: BalanceManager, txb: Transaction) => {
	const tradeProof = generateProof(balanceManager, txb);

	txb.moveCall({
		target: `${DEEPBOOK_PACKAGE_ID}::pool::unstake`,
		arguments: [txb.object(pool.address), txb.object(balanceManager.address), tradeProof],
		typeArguments: [pool.baseCoin.type, pool.quoteCoin.type],
	});
};

export const submitProposal = (
	pool: Pool,
	balanceManager: BalanceManager,
	takerFee: number,
	makerFee: number,
	stakeRequired: number,
	txb: Transaction,
) => {
	const tradeProof = generateProof(balanceManager, txb);

	txb.moveCall({
		target: `${DEEPBOOK_PACKAGE_ID}::pool::submit_proposal`,
		arguments: [
			txb.object(pool.address),
			txb.object(balanceManager.address),
			tradeProof,
			txb.pure.u64(takerFee * FLOAT_SCALAR),
			txb.pure.u64(makerFee * FLOAT_SCALAR),
			txb.pure.u64(stakeRequired * DEEP_SCALAR),
		],
		typeArguments: [pool.baseCoin.type, pool.quoteCoin.type],
	});
};

export const vote = (
	pool: Pool,
	balanceManager: BalanceManager,
	proposal_id: string,
	txb: Transaction,
) => {
	const tradeProof = generateProof(balanceManager, txb);

	txb.moveCall({
		target: `${DEEPBOOK_PACKAGE_ID}::pool::vote`,
		arguments: [
			txb.object(pool.address),
			txb.object(balanceManager.address),
			tradeProof,
			txb.pure.id(proposal_id),
		],
	});
};
