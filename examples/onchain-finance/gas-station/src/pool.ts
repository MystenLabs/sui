// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiClient } from '@mysten/sui/client';

// docs::#gas-coin-type
interface GasCoin {
	objectId: string;
	version: string;
	digest: string;
	reserved: boolean;
}
// docs::/#gas-coin-type

// docs::#pool
class GasCoinPool {
	private coins: Map<string, GasCoin> = new Map();

	async initialize(client: SuiClient, sponsorAddress: string) {
		const { data: ownedCoins } = await client.getOwnedObjects({
			owner: sponsorAddress,
			filter: { StructType: '0x2::coin::Coin<0x2::sui::SUI>' },
			options: { showContent: true },
		});
		for (const item of ownedCoins) {
			if (!item.data) continue;
			this.coins.set(item.data.objectId, {
				objectId: item.data.objectId,
				version: item.data.version,
				digest: item.data.digest,
				reserved: false,
			});
		}
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

	release(objectId: string, newVersion?: string, newDigest?: string) {
		const coin = this.coins.get(objectId);
		if (coin) {
			coin.reserved = false;
			if (newVersion) coin.version = newVersion;
			if (newDigest) coin.digest = newDigest;
		}
	}

	discard(objectId: string) {
		this.coins.delete(objectId);
	}
}
// docs::/#pool

export { GasCoinPool };
export type { GasCoin };
