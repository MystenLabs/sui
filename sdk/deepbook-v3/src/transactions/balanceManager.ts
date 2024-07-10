// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import type { Transaction } from '@mysten/sui/transactions';

import type { BalanceManager, Coin } from '../types/index.js';
import type { DeepBookConfig } from '../utils/config.js';

export class BalanceManagerContract {
	#config: DeepBookConfig;

	constructor(config: DeepBookConfig) {
		this.#config = config;
	}

	createAndShareBalanceManager = () => (tx: Transaction) => {
		const manager = tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::balance_manager::new`,
		});

		return tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::balance_manager::share`,
			arguments: [manager],
		});
	};

	depositIntoManager =
		(managerId: string, amountToDeposit: number, coin: Coin) => (tx: Transaction) => {
			let deposit;

			if (coin.key === 'SUI') {
				[deposit] = tx.splitCoins(tx.gas, [tx.pure.u64(amountToDeposit * coin.scalar)]);
			} else {
				[deposit] = tx.splitCoins(tx.object(this.#config.getCoinId(coin.key)), [
					tx.pure.u64(amountToDeposit * coin.scalar),
				]);
			}

			return tx.moveCall({
				target: `${this.#config.DEEPBOOK_PACKAGE_ID}::balance_manager::deposit`,
				arguments: [tx.object(managerId), deposit],
				typeArguments: [coin.type],
			});
		};

	withdrawFromManager =
		(managerId: string, amountToWithdraw: number, coin: Coin, recepient: string) =>
		(tx: Transaction) => {
			const coinObject = tx.moveCall({
				target: `${this.#config.DEEPBOOK_PACKAGE_ID}::balance_manager::withdraw`,
				arguments: [tx.object(managerId), tx.pure.u64(amountToWithdraw * coin.scalar)],
				typeArguments: [coin.type],
			});

			tx.transferObjects([coinObject], recepient);
		};

	withdrawAllFromManager =
		(managerId: string, coin: Coin, recepient: string) => (tx: Transaction) => {
			const coinObject = tx.moveCall({
				target: `${this.#config.DEEPBOOK_PACKAGE_ID}::balance_manager::withdraw_all`,
				arguments: [tx.object(managerId)],
				typeArguments: [coin.type],
			});

			tx.transferObjects([coinObject], recepient);
		};

	checkManagerBalance = (managerId: string, coin: Coin) => (tx: Transaction) =>
		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::balance_manager::balance`,
			arguments: [tx.object(managerId)],
			typeArguments: [coin.type],
		});

	generateProofAsOwner = (managerId: string) => (tx: Transaction) =>
		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::balance_manager::generate_proof_as_owner`,
			arguments: [tx.object(managerId)],
		});

	generateProof = (balanceManager: BalanceManager) =>
		balanceManager.tradeCap
			? this.generateProofAsTrader(balanceManager.address, balanceManager.tradeCap)
			: this.generateProofAsOwner(balanceManager.address);

	generateProofAsTrader = (managerId: string, tradeCapId: string) => (tx: Transaction) =>
		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::balance_manager::generate_proof_as_trader`,
			arguments: [tx.object(managerId), tx.object(tradeCapId)],
		});

	validateProof = (managerId: string, tradeProofId: string) => (tx: Transaction) =>
		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::balance_manager::validate_proof`,
			arguments: [tx.object(managerId), tx.object(tradeProofId)],
		});

	owner = (managerId: string) => (tx: Transaction) =>
		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::balance_manager::owner`,
			arguments: [tx.object(managerId)],
		});

	id = (managerId: string) => (tx: Transaction) =>
		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::balance_manager::id`,
			arguments: [tx.object(managerId)],
		});
}
