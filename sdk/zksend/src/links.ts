// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { URL } from 'url';
import { Ed25519Keypair } from '@mysten/sui.js/keypairs/ed25519';
import { TransactionBlock } from '@mysten/sui.js/src/builder';
import { getFullnodeUrl, SuiClient } from '@mysten/sui.js/src/client';
import type { Keypair } from '@mysten/sui.js/src/cryptography';
import { fromB64, normalizeStructTag, normalizeSuiObjectId } from '@mysten/sui.js/utils';

export interface ZkSendLinkOptions {
	host?: string;
	path?: string;
	pendingMist?: number;
	client?: SuiClient;
	keypair?: Keypair;
}

export const DEFAULT_ZK_SEND_LINK_OPTIONS = {
	host: 'https://zksend.com',
	path: '/claim',
	client: new SuiClient({ url: getFullnodeUrl('mainnet') }),
};

const SUI_COIN_TYPE = normalizeStructTag('0x2::coin::Coin<0x2::sui::SUI>');

export class ZkSendLink {
	#client: SuiClient;
	#host: string;
	#path: string;
	#keypair: Keypair;
	#pendingMist = 0n;
	#initiallyOwnedObjects = new Set<string>();
	#ownedBalances = new Map<string, bigint>();
	#pendingObjects = new Set<string>();
	#ownedObjects: Array<{
		objectId: string;
		version: string;
		digest: string;
		type: string;
	}> = [];
	#gasFee = 0n;

	constructor({
		host = DEFAULT_ZK_SEND_LINK_OPTIONS.host,
		path = DEFAULT_ZK_SEND_LINK_OPTIONS.path,
		client = DEFAULT_ZK_SEND_LINK_OPTIONS.client,
		keypair = new Ed25519Keypair(),
	}: ZkSendLinkOptions) {
		this.#host = host;
		this.#path = path;
		this.#client = client;
		this.#keypair = keypair;
	}

	static async fromUrl(url: string) {
		const parsed = new URL(url);
		const keypair = Ed25519Keypair.fromSecretKey(fromB64(parsed.hash.slice(1)));

		const link = new ZkSendLink({
			host: parsed.origin,
			path: parsed.pathname,
			keypair,
		});

		await link.loadOwnedData();

		return link;
	}

	addClaimableMist(amount: bigint) {
		this.#pendingMist += amount;
	}

	addClaimableObject(id: string) {
		this.#pendingObjects.add(id);
	}

	getLink(): string {
		const link = new URL(this.#host);
		link.pathname = this.#path;
		link.hash = this.#keypair.export().privateKey;

		return link.toString();
	}

	async loadOwnedData() {
		await Promise.all([
			this.#loadInitialTransactionData(),
			this.#loadOwnedObjects(),
			this.#loadOwnedBalances(),
		]);
	}

	async addGasForClaim(
		getAmount?: (options: {
			amount: bigint;
			objects: string[];
			estimatedFee: bigint;
		}) => Promise<bigint> | bigint,
	) {
		const estimatedFee = await this.#estimateClaimGasFee();
		this.#gasFee = getAmount
			? await getAmount({
					amount: this.#pendingMist,
					objects: [...this.#pendingObjects],
					estimatedFee,
			  })
			: estimatedFee;
	}

	createSendTransaction() {
		const txb = new TransactionBlock();
		const address = this.#keypair.toSuiAddress();
		const objectsToTransfer = [...this.#pendingObjects].map((id) => txb.object(id));

		if (this.#pendingMist > 0n || this.#gasFee > 0) {
			const [coin] = txb.splitCoins(txb.gas, [this.#pendingMist + this.#gasFee]);
			objectsToTransfer.push(coin);
		}

		txb.transferObjects(objectsToTransfer, address);

		return txb;
	}

	createClaimTransaction(
		address: string,
		options?: {
			claimAllObjects?: boolean;
		},
	) {
		const txb = new TransactionBlock();

		const objectsToTransfer = this.#ownedObjects
			.filter((object) => {
				if (!options?.claimAllObjects && this.#initiallyOwnedObjects.has(object.objectId)) {
					return false;
				}

				return object.type !== SUI_COIN_TYPE;
			})
			.map((object) => txb.object(object.objectId));

		txb.transferObjects([...objectsToTransfer, txb.gas], address);

		return txb;
	}

	#estimateClaimGasFee(): Promise<bigint> {
		return Promise.resolve(0n);
	}

	async #loadOwnedObjects() {
		this.#ownedObjects = [];
		let nextCursor: string | null | undefined;
		do {
			const ownedObjects = await this.#client.getOwnedObjects({
				cursor: nextCursor,
				owner: this.#keypair.toSuiAddress(),
				options: {
					showType: true,
				},
			});

			nextCursor = ownedObjects.nextCursor;
			for (const object of ownedObjects.data) {
				if (object.data) {
					this.#ownedObjects.push({
						objectId: normalizeSuiObjectId(object.data.objectId),
						version: object.data.version,
						digest: object.data.digest,
						type: normalizeStructTag(object.data.type!),
					});
				}
			}
		} while (nextCursor);
	}

	async #loadOwnedBalances() {
		this.#ownedBalances = new Map();

		const balances = await this.#client.getAllBalances({
			owner: this.#keypair.toSuiAddress(),
		});

		for (const balance of balances) {
			this.#ownedBalances.set(normalizeStructTag(balance.coinType), BigInt(balance.totalBalance));
		}
	}

	async #loadInitialTransactionData() {
		const result = await this.#client.queryTransactionBlocks({
			limit: 1,
			order: 'ascending',
			filter: {
				ToAddress: this.#keypair.toSuiAddress(),
			},
			options: {
				showEffects: true,
			},
		});
		const effects = result.data[0]?.effects;

		if (result.data[0]) {
			[...(effects?.mutated ?? []), ...(effects?.created ?? [])]?.forEach((effect) => {
				if (
					typeof effect.owner === 'object' &&
					'AddressOwner' in effect.owner &&
					effect.owner.AddressOwner === this.#keypair.toSuiAddress()
				) {
					this.#initiallyOwnedObjects.add(normalizeSuiObjectId(effect.reference.objectId));
				}
			});
		}
	}
}
