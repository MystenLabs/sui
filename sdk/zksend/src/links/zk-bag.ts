// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Argument, Transaction, TransactionObjectArgument } from '@mysten/sui/transactions';

export interface ZkBagContractOptions {
	packageId: string;
	bagStoreId: string;
	bagStoreTableId: string;
}

export const MAINNET_CONTRACT_IDS: ZkBagContractOptions = {
	packageId: '0x5bb7d0bb3240011336ca9015f553b2646302a4f05f821160344e9ec5a988f740',
	bagStoreId: '0x65b215a3f2a951c94313a89c43f0adbd2fd9ea78a0badf81e27d1c9868a8b6fe',
	bagStoreTableId: '0x616db54ca564660cd58e36a4548be68b289371ef2611485c62c374a60960084e',
};

export class ZkBag<IDs> {
	#package: string;
	#module = 'zk_bag' as const;
	ids: IDs;

	constructor(packageAddress: string, ids: IDs) {
		this.#package = packageAddress;
		this.ids = ids;
	}

	new({
		arguments: [store, receiver],
	}: {
		arguments: [store: TransactionObjectArgument | string, receiver: Argument | string];
	}) {
		return (tx: Transaction) => {
			tx.moveCall({
				target: `${this.#package}::${this.#module}::new`,
				arguments: [
					tx.object(store),
					typeof receiver === 'string' ? tx.pure.address(receiver) : receiver,
				],
			});
		};
	}

	add({
		arguments: [store, receiver, item],
		typeArguments,
	}: {
		arguments: [
			store: TransactionObjectArgument | string,
			receiver: Argument | string,
			item: TransactionObjectArgument | string,
		];
		typeArguments: [string];
	}): (tx: Transaction) => Extract<Argument, { $kind: 'Result' }> {
		return (tx: Transaction) =>
			tx.moveCall({
				target: `${this.#package}::${this.#module}::add`,
				arguments: [
					tx.object(store),
					typeof receiver === 'string' ? tx.pure.address(receiver) : receiver,
					tx.object(item),
				],
				typeArguments: typeArguments,
			});
	}

	init_claim({ arguments: [store] }: { arguments: [store: TransactionObjectArgument | string] }) {
		return (tx: Transaction) => {
			const [bag, claimProof] = tx.moveCall({
				target: `${this.#package}::${this.#module}::init_claim`,
				arguments: [tx.object(store)],
			});

			return [bag, claimProof] as const;
		};
	}

	reclaim({
		arguments: [store, receiver],
	}: {
		arguments: [store: TransactionObjectArgument | string, receiver: Argument | string];
	}) {
		return (tx: Transaction) => {
			const [bag, claimProof] = tx.moveCall({
				target: `${this.#package}::${this.#module}::reclaim`,
				arguments: [
					tx.object(store),
					typeof receiver === 'string' ? tx.pure.address(receiver) : receiver,
				],
			});

			return [bag, claimProof] as const;
		};
	}

	claim({
		arguments: [bag, claim, id],
		typeArguments,
	}: {
		arguments: [
			bag: TransactionObjectArgument | string,
			claim: Extract<Argument, { $kind: 'NestedResult' }>,
			id: TransactionObjectArgument | string,
		];
		typeArguments: [string];
	}): (tx: Transaction) => Extract<Argument, { $kind: 'Result' }> {
		return (tx: Transaction) =>
			tx.moveCall({
				target: `${this.#package}::${this.#module}::claim`,
				arguments: [tx.object(bag), tx.object(claim), typeof id === 'string' ? tx.object(id) : id],
				typeArguments,
			});
	}

	finalize({
		arguments: [bag, claim],
	}: {
		arguments: [
			bag: TransactionObjectArgument | string,
			claim: Extract<Argument, { $kind: 'NestedResult' }>,
		];
	}) {
		return (tx: Transaction) => {
			tx.moveCall({
				target: `${this.#package}::${this.#module}::finalize`,
				arguments: [tx.object(bag), tx.object(claim)],
			});
		};
	}

	update_receiver({
		arguments: [bag, from, to],
	}: {
		arguments: [
			bag: TransactionObjectArgument | string,
			from: Argument | string,
			to: Argument | string,
		];
	}) {
		return (tx: Transaction) => {
			tx.moveCall({
				target: `${this.#package}::${this.#module}::update_receiver`,
				arguments: [
					tx.object(bag),
					typeof from === 'string' ? tx.pure.address(from) : from,
					typeof to === 'string' ? tx.pure.address(to) : to,
				],
			});
		};
	}
}
