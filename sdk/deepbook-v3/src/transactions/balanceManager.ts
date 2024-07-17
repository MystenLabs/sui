// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { coinWithBalance } from '@mysten/sui/transactions';
import type { Transaction } from '@mysten/sui/transactions';

import type { BalanceManager, Coin } from '../types/index.js';
import type { DeepBookConfig } from '../utils/config.js';

/**
 * BalanceManagerContract class for managing BalanceManager operations.
 */
export class BalanceManagerContract {
	#config: DeepBookConfig;

	/**
	 * @param config Configuration for BalanceManagerContract
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
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::balance_manager::share`,
			arguments: [manager],
		});
	};

	/**
	 * @description Deposit funds into the BalanceManager
	 * @param managerId The ID of the BalanceManager
	 * @param amountToDeposit The amount to deposit
	 * @param coin The coin to deposit
	 * @returns A function that takes a Transaction object
	 */
	depositIntoManager =
		(managerId: string, amountToDeposit: number, coin: Coin) => (tx: Transaction) => {
			tx.setSenderIfNotSet(this.#config.address);
			const deposit = coinWithBalance({
				type: coin.type,
				balance: amountToDeposit * coin.scalar,
			});

			tx.moveCall({
				target: `${this.#config.DEEPBOOK_PACKAGE_ID}::balance_manager::deposit`,
				arguments: [tx.object(managerId), deposit],
				typeArguments: [coin.type],
			});
		};

	/**
	 * @description Withdraw funds from the BalanceManager
	 * @param managerId The ID of the BalanceManager
	 * @param amountToWithdraw The amount to withdraw
	 * @param coin The coin to withdraw
	 * @param recipient The recipient of the withdrawn funds
	 * @returns A function that takes a Transaction object
	 */
	withdrawFromManager =
		(managerId: string, amountToWithdraw: number, coin: Coin, recipient: string) =>
		(tx: Transaction) => {
			const coinObject = tx.moveCall({
				target: `${this.#config.DEEPBOOK_PACKAGE_ID}::balance_manager::withdraw`,
				arguments: [tx.object(managerId), tx.pure.u64(amountToWithdraw * coin.scalar)],
				typeArguments: [coin.type],
			});

			tx.transferObjects([coinObject], recipient);
		};

	/**
	 * @description Withdraw all funds from the BalanceManager
	 * @param managerId The ID of the BalanceManager
	 * @param coin The coin to withdraw
	 * @param recipient The recipient of the withdrawn funds
	 * @returns A function that takes a Transaction object
	 */
	withdrawAllFromManager =
		(managerId: string, coin: Coin, recipient: string) => (tx: Transaction) => {
			const withdrawalCoin = tx.moveCall({
				target: `${this.#config.DEEPBOOK_PACKAGE_ID}::balance_manager::withdraw_all`,
				arguments: [tx.object(managerId)],
				typeArguments: [coin.type],
			});

			tx.transferObjects([withdrawalCoin], recipient);
		};

	/**
	 * @description Check the balance of the BalanceManager
	 * @param managerId The ID of the BalanceManager
	 * @param coin The coin to check the balance of
	 * @returns A function that takes a Transaction object
	 */
	checkManagerBalance = (managerId: string, coin: Coin) => (tx: Transaction) => {
		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::balance_manager::balance`,
			arguments: [tx.object(managerId)],
			typeArguments: [coin.type],
		});
	};

	/**
	 * @description Generate a trade proof for the BalanceManager. Calls the appropriate function based on whether tradeCap is set.
	 * @param balanceManager The BalanceManager object
	 * @returns A function that takes a Transaction object
	 */
	generateProof = (balanceManager: BalanceManager) => (tx: Transaction) => {
		return tx.add(
			balanceManager.tradeCap
				? this.generateProofAsTrader(balanceManager.address, balanceManager.tradeCap)
				: this.generateProofAsOwner(balanceManager.address),
		);
	};

	/**
	 * @description Generate a trade proof as the owner
	 * @param managerId The ID of the BalanceManager
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
	 * @param managerId The ID of the BalanceManager
	 * @param tradeCapId The ID of the tradeCap
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
	 * @param managerId The ID of the BalanceManager
	 * @returns A function that takes a Transaction object
	 */
	owner = (managerId: string) => (tx: Transaction) =>
		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::balance_manager::owner`,
			arguments: [tx.object(managerId)],
		});

	/**
	 * @description Get the ID of the BalanceManager
	 * @param managerId The ID of the BalanceManager
	 * @returns A function that takes a Transaction object
	 */
	id = (managerId: string) => (tx: Transaction) =>
		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::balance_manager::id`,
			arguments: [tx.object(managerId)],
		});
}
