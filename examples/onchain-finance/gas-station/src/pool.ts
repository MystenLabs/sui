// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiGrpcClient } from '@mysten/sui/grpc';

// docs::#gas-coin-type
interface GasCoin {
	objectId: string;
	version: string;
	digest: string;
	balance: bigint;
	reserved: boolean;
}
// docs::/#gas-coin-type

// docs::#pool
class GasCoinPool {
	private coins: Map<string, GasCoin> = new Map();
	private readonly minBalance: bigint;

	// Coins below minBalance cannot cover the gas budget. Handing one out
	// wastes a request: the sponsor only discovers the shortfall at simulation.
	constructor(minBalance: bigint) {
		this.minBalance = minBalance;
	}

	async initialize(client: SuiGrpcClient, sponsorAddress: string) {
		// A sponsor pool is deliberately made of many small coins, so the first
		// page is never the whole pool. Page until the cursor is exhausted.
		let cursor: string | null = null;
		do {
			const page = await client.listOwnedObjects({
				owner: sponsorAddress,
				type: '0x2::coin::Coin<0x2::sui::SUI>',
				cursor,
			});

			for (const object of page.objects) {
				const balance = BigInt(object.balance ?? 0n);
				if (balance < this.minBalance) continue;
				this.coins.set(object.objectId, {
					objectId: object.objectId,
					version: object.version,
					digest: object.digest,
					balance,
					reserved: false,
				});
			}

			cursor = page.hasNextPage ? page.cursor : null;
		} while (cursor);
	}

	acquire(): GasCoin | null {
		for (const coin of this.coins.values()) {
			if (!coin.reserved) {
				coin.reserved = true;
				return coin;
			}
		}
		return null;
	}

	release(objectId: string, newVersion?: string, newDigest?: string, newBalance?: bigint) {
		const coin = this.coins.get(objectId);
		if (!coin) return;
		coin.reserved = false;
		if (newVersion) coin.version = newVersion;
		if (newDigest) coin.digest = newDigest;
		if (newBalance !== undefined) coin.balance = newBalance;
		// Gas spend eventually drains a coin below the usable threshold.
		if (coin.balance < this.minBalance) this.coins.delete(objectId);
	}

	discard(objectId: string) {
		this.coins.delete(objectId);
	}

	// Drives replenishment: split more coins when this drops below a threshold.
	get availableCount(): number {
		let count = 0;
		for (const coin of this.coins.values()) if (!coin.reserved) count += 1;
		return count;
	}
}
// docs::/#pool

export { GasCoinPool };
export type { GasCoin };
