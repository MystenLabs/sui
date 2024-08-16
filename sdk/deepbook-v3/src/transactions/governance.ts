// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import type { Transaction } from '@mysten/sui/transactions';

import type { ProposalParams } from '../types/index.js';
import type { DeepBookConfig } from '../utils/config.js';
import { DEEP_SCALAR, FLOAT_SCALAR } from '../utils/config.js';

/**
 * GovernanceContract class for managing governance operations in DeepBook.
 */
export class GovernanceContract {
	#config: DeepBookConfig;

	/**
	 * @param {DeepBookConfig} config Configuration for GovernanceContract
	 */
	constructor(config: DeepBookConfig) {
		this.#config = config;
	}

	/**
	 * @description Stake a specified amount in the pool
	 * @param {string} poolKey The key to identify the pool
	 * @param {string} balanceManagerKey The key to identify the BalanceManager
	 * @param {number} stakeAmount The amount to stake
	 * @returns A function that takes a Transaction object
	 */
	stake =
		(poolKey: string, balanceManagerKey: string, stakeAmount: number) => (tx: Transaction) => {
			const pool = this.#config.getPool(poolKey);
			const balanceManager = this.#config.getBalanceManager(balanceManagerKey);
			const tradeProof = tx.add(this.#config.balanceManager.generateProof(balanceManagerKey));
			const baseCoin = this.#config.getCoin(pool.baseCoin);
			const quoteCoin = this.#config.getCoin(pool.quoteCoin);
			const stakeInput = Math.round(stakeAmount * DEEP_SCALAR);

			tx.moveCall({
				target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::stake`,
				arguments: [
					tx.object(pool.address),
					tx.object(balanceManager.address),
					tradeProof,
					tx.pure.u64(stakeInput),
				],
				typeArguments: [baseCoin.type, quoteCoin.type],
			});
		};

	/**
	 * @description Unstake from the pool
	 * @param {string} poolKey The key to identify the pool
	 * @param {string} balanceManagerKey The key to identify the BalanceManager
	 * @returns A function that takes a Transaction object
	 */
	unstake = (poolKey: string, balanceManagerKey: string) => (tx: Transaction) => {
		const pool = this.#config.getPool(poolKey);
		const balanceManager = this.#config.getBalanceManager(balanceManagerKey);
		const tradeProof = tx.add(this.#config.balanceManager.generateProof(balanceManagerKey));
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);

		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::unstake`,
			arguments: [tx.object(pool.address), tx.object(balanceManager.address), tradeProof],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
	};

	/**
	 * @description Submit a governance proposal
	 * @param {ProposalParams} params Parameters for the proposal
	 * @returns A function that takes a Transaction object
	 */
	submitProposal = (params: ProposalParams) => (tx: Transaction) => {
		const { poolKey, balanceManagerKey, takerFee, makerFee, stakeRequired } = params;

		const pool = this.#config.getPool(poolKey);
		const balanceManager = this.#config.getBalanceManager(balanceManagerKey);
		const tradeProof = tx.add(this.#config.balanceManager.generateProof(balanceManagerKey));
		const baseCoin = this.#config.getCoin(pool.baseCoin);
		const quoteCoin = this.#config.getCoin(pool.quoteCoin);

		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::submit_proposal`,
			arguments: [
				tx.object(pool.address),
				tx.object(balanceManager.address),
				tradeProof,
				tx.pure.u64(Math.round(takerFee * FLOAT_SCALAR)),
				tx.pure.u64(Math.round(makerFee * FLOAT_SCALAR)),
				tx.pure.u64(Math.round(stakeRequired * DEEP_SCALAR)),
			],
			typeArguments: [baseCoin.type, quoteCoin.type],
		});
	};

	/**
	 * @description Vote on a proposal
	 * @param {string} poolKey The key to identify the pool
	 * @param {string} balanceManagerKey The key to identify the BalanceManager
	 * @param {string} proposal_id The ID of the proposal to vote on
	 * @returns A function that takes a Transaction object
	 */
	vote = (poolKey: string, balanceManagerKey: string, proposal_id: string) => (tx: Transaction) => {
		const pool = this.#config.getPool(poolKey);
		const balanceManager = this.#config.getBalanceManager(balanceManagerKey);
		const tradeProof = tx.add(this.#config.balanceManager.generateProof(balanceManagerKey));

		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::pool::vote`,
			arguments: [
				tx.object(pool.address),
				tx.object(balanceManager.address),
				tradeProof,
				tx.pure.id(proposal_id),
			],
		});
	};
}
