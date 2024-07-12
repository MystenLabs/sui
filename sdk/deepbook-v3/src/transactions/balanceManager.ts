// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { Transaction } from '@mysten/sui/transactions';

import type { BalanceManager, Coin } from '../types/index.js';
import type { DeepBookConfig } from '../utils/config.js';

export class BalanceManagerContract {
	#config: DeepBookConfig;

	constructor(config: DeepBookConfig) {
		this.#config = config;
	}

	createAndShareBalanceManager = (tx: Transaction = new Transaction()) => {
		const manager = tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::balance_manager::new`,
		});

		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::balance_manager::share`,
			arguments: [manager],
		});
	};

	depositIntoManager = (
		managerId: string,
		amountToDeposit: number,
		coin: Coin,
		tx: Transaction = new Transaction(),
	) => {
		let deposit;

		if (coin.key === 'SUI') {
			[deposit] = tx.splitCoins(tx.gas, [tx.pure.u64(amountToDeposit * coin.scalar)]);
		} else {
			[deposit] = tx.splitCoins(tx.object(this.#config.getCoinId(coin.key)), [
				tx.pure.u64(amountToDeposit * coin.scalar),
			]);
		}

		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::balance_manager::deposit`,
			arguments: [tx.object(managerId), deposit],
			typeArguments: [coin.type],
		});
	};

	withdrawFromManager = (
		managerId: string,
		amountToWithdraw: number,
		coin: Coin,
		recipient: string,
		tx: Transaction = new Transaction(),
	) => {
		const coinObject = tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::balance_manager::withdraw`,
			arguments: [tx.object(managerId), tx.pure.u64(amountToWithdraw * coin.scalar)],
			typeArguments: [coin.type],
		});

		tx.transferObjects([coinObject], recipient);
	};

	withdrawAllFromManager = (
		managerId: string,
		coin: Coin,
		recipient: string,
		tx: Transaction = new Transaction(),
	) => {
		const withdrawalCoin = tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::balance_manager::withdraw_all`,
			arguments: [tx.object(managerId)],
			typeArguments: [coin.type],
		});

		tx.transferObjects([withdrawalCoin], recipient);
	};

	checkManagerBalance = (managerId: string, coin: Coin) => {
		const tx = new Transaction();
		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::balance_manager::balance`,
			arguments: [tx.object(managerId)],
			typeArguments: [coin.type],
		});

		return tx;
	};

	generateProof = (balanceManager: BalanceManager, tx: Transaction = new Transaction()) => {
		return balanceManager.tradeCap
			? this.generateProofAsTrader(balanceManager.address, balanceManager.tradeCap, tx)
			: this.generateProofAsOwner(balanceManager.address, tx);
	};

	generateProofAsOwner = (managerId: string, tx: Transaction = new Transaction()) => {
		return tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::balance_manager::generate_proof_as_owner`,
			arguments: [tx.object(managerId)],
		});
	};

	generateProofAsTrader = (
		managerId: string,
		tradeCapId: string,
		tx: Transaction = new Transaction(),
	) => {
		return tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::balance_manager::generate_proof_as_trader`,
			arguments: [tx.object(managerId), tx.object(tradeCapId)],
		});
	};

	owner = (managerId: string, tx: Transaction = new Transaction()) =>
		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::balance_manager::owner`,
			arguments: [tx.object(managerId)],
		});

	id = (managerId: string, tx: Transaction = new Transaction()) =>
		tx.moveCall({
			target: `${this.#config.DEEPBOOK_PACKAGE_ID}::balance_manager::id`,
			arguments: [tx.object(managerId)],
		});
}
