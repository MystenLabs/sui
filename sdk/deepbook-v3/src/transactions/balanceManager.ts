// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { coinWithBalance } from '@mysten/sui/transactions';
import type { Transaction } from '@mysten/sui/transactions';

import type { DeepBookConfig } from '../utils/config.js';

/**
 * BalanceManagerContract class for managing BalanceManager operations.
 */
export class BalanceManagerContract {
	#config: DeepBookConfig;

	/**
	 * @param {DeepBookConfig} config Configuration for BalanceManagerContract
	 */
	constructor(config: DeepBookConfig) {
		this.#config = config;
	}

	/**
	 * @description Create and share a new BalanceManager
	 * @returns A function that takes a Transaction object
	 */
	createAndShareBalanceManager = () => (tx: Transaction) => {
		const manager = tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::balance_manager::new`,
		});

		tx.moveCall({
			target: '0x2::transfer::public_share_object',
			arguments: [manager],
			typeArguments: [`${this.#config.DEEPBOOK_PACKAGE_ID}::balance_manager::BalanceManager`],
		});
	};

	/**
	 * @description Deposit funds into the BalanceManager
	 * @param {string} managerKey The key of the BalanceManager
	 * @param {string} coinKey The key of the coin to deposit
	 * @param {number} amountToDeposit The amount to deposit
	 * @returns A function that takes a Transaction object
	 */
	depositIntoManager =
		(managerKey: string, coinKey: string, amountToDeposit: number) => (tx: Transaction) => {
			tx.setSenderIfNotSet(this.#config.address);
			const managerId = this.#config.getBalanceManager(managerKey).address;
			const coin = this.#config.getCoin(coinKey);
			const depositInput = Math.round(amountToDeposit * coin.scalar);
			const deposit = coinWithBalance({
				type: coin.type,
				balance: depositInput,
			});

			tx.moveCall({
				target: `${this.#config.DEEPBOOK_PACKAGE_ID}::balance_manager::deposit`,
				arguments: [tx.object(managerId), deposit],
				typeArguments: [coin.type],
			});
		};

	/**
	 * @description Withdraw funds from the BalanceManager
	 * @param {string} managerKey The key of the BalanceManager
	 * @param {string} coinKey The key of the coin to withdraw
	 * @param {number} amountToWithdraw The amount to withdraw
	 * @param {string} recipient The recipient of the withdrawn funds
	 * @returns A function that takes a Transaction object
	 */
	withdrawFromManager =
		(managerKey: string, coinKey: string, amountToWithdraw: number, recipient: string) =>
		(tx: Transaction) => {
			const managerId = this.#config.getBalanceManager(managerKey).address;
			const coin = this.#config.getCoin(coinKey);
			const withdrawInput = Math.round(amountToWithdraw * coin.scalar);
			const coinObject = tx.moveCall({
				target: `${this.#config.DEEPBOOK_PACKAGE_ID}::balance_manager::withdraw`,
				arguments: [tx.object(managerId), tx.pure.u64(withdrawInput)],
				typeArguments: [coin.type],
			});

			tx.transferObjects([coinObject], recipient);
		};

	/**
	 * @description Withdraw all funds from the BalanceManager
	 * @param {string} managerKey The key of the BalanceManager
	 * @param {string} coinKey The key of the coin to withdraw
	 * @param {string} recipient The recipient of the withdrawn funds
	 * @returns A function that takes a Transaction object
	 */
	withdrawAllFromManager =
		(managerKey: string, coinKey: string, recipient: string) => (tx: Transaction) => {
			const managerId = this.#config.getBalanceManager(managerKey).address;
			const coin = this.#config.getCoin(coinKey);
			const withdrawalCoin = tx.moveCall({
				target: `${this.#config.DEEPBOOK_PACKAGE_ID}::balance_manager::withdraw_all`,
				arguments: [tx.object(managerId)],
				typeArguments: [coin.type],
			});

			tx.transferObjects([withdrawalCoin], recipient);
		};

	/**
	 * @description Check the balance of the BalanceManager
	 * @param {string} managerKey The key of the BalanceManager
	 * @param {string} coinKey The key of the coin to check the balance of
	 * @returns A function that takes a Transaction object
	 */
	checkManagerBalance = (managerKey: string, coinKey: string) => (tx: Transaction) => {
		const managerId = this.#config.getBalanceManager(managerKey).address;
		const coin = this.#config.getCoin(coinKey);
		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::balance_manager::balance`,
			arguments: [tx.object(managerId)],
			typeArguments: [coin.type],
		});
	};

	/**
	 * @description Generate a trade proof for the BalanceManager. Calls the appropriate function based on whether tradeCap is set.
	 * @param {string} managerKey The key of the BalanceManager
	 * @returns A function that takes a Transaction object
	 */
	generateProof = (managerKey: string) => (tx: Transaction) => {
		const balanceManager = this.#config.getBalanceManager(managerKey);
		return tx.add(
			balanceManager.tradeCap
				? this.generateProofAsTrader(balanceManager.address, balanceManager.tradeCap)
				: this.generateProofAsOwner(balanceManager.address),
		);
	};

	/**
	 * @description Generate a trade proof as the owner
	 * @param {string} managerId The ID of the BalanceManager
	 * @returns A function that takes a Transaction object
	 */
	generateProofAsOwner = (managerId: string) => (tx: Transaction) => {
		return tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::balance_manager::generate_proof_as_owner`,
			arguments: [tx.object(managerId)],
		});
	};

	/**
	 * @description Generate a trade proof as a trader
	 * @param {string} managerId The ID of the BalanceManager
	 * @param {string} tradeCapId The ID of the tradeCap
	 * @returns A function that takes a Transaction object
	 */
	generateProofAsTrader = (managerId: string, tradeCapId: string) => (tx: Transaction) => {
		return tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::balance_manager::generate_proof_as_trader`,
			arguments: [tx.object(managerId), tx.object(tradeCapId)],
		});
	};

	/**
	 * @description Get the owner of the BalanceManager
	 * @param {string} managerKey The key of the BalanceManager
	 * @returns A function that takes a Transaction object
	 */
	owner = (managerKey: string) => (tx: Transaction) => {
		const managerId = this.#config.getBalanceManager(managerKey).address;
		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::balance_manager::owner`,
			arguments: [tx.object(managerId)],
		});
	};

	/**
	 * @description Get the ID of the BalanceManager
	 * @param {string} managerKey The key of the BalanceManager
	 * @returns A function that takes a Transaction object
	 */
	id = (managerKey: string) => (tx: Transaction) => {
		const managerId = this.#config.getBalanceManager(managerKey).address;
		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::balance_manager::id`,
			arguments: [tx.object(managerId)],
		});
	};
}
