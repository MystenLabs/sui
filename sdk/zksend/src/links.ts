// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getFullnodeUrl, SuiClient } from '@mysten/sui.js/client';
import type { ObjectOwner, SuiObjectChange } from '@mysten/sui.js/client';
import type { Keypair } from '@mysten/sui.js/cryptography';
import { Ed25519Keypair } from '@mysten/sui.js/keypairs/ed25519';
import type { TransactionObjectInput } from '@mysten/sui.js/transactions';
import { TransactionBlock } from '@mysten/sui.js/transactions';
import {
	fromB64,
	normalizeStructTag,
	normalizeSuiAddress,
	normalizeSuiObjectId,
	parseStructTag,
} from '@mysten/sui.js/utils';

export interface ZkSendLinkBuilderOptions {
	host?: string;
	path?: string;
	mist?: number;
	keypair?: Keypair;
}

export interface ZkSendLinkOptions {
	keypair?: Keypair;
	client?: SuiClient;
}

const DEFAULT_ZK_SEND_LINK_OPTIONS = {
	host: 'https://zksend.com',
	path: '/claim',
	client: new SuiClient({ url: getFullnodeUrl('mainnet') }),
};

const SUI_COIN_TYPE = normalizeStructTag('0x2::coin::Coin<0x2::sui::SUI>');

export class ZkSendLinkBuilder {
	#host: string;
	#path: string;
	#keypair: Keypair;
	#objects = new Set<TransactionObjectInput>();
	#mist = 0n;
	#gasFee = 0n;

	constructor({
		host = DEFAULT_ZK_SEND_LINK_OPTIONS.host,
		path = DEFAULT_ZK_SEND_LINK_OPTIONS.path,
		keypair = new Ed25519Keypair(),
	}: ZkSendLinkBuilderOptions = {}) {
		this.#host = host;
		this.#path = path;
		this.#keypair = keypair;
	}

	addClaimableMist(amount: bigint) {
		this.#mist += amount;
	}

	addClaimableObject(id: TransactionObjectInput) {
		this.#objects.add(id);
	}

	getLink(): string {
		const link = new URL(this.#host);
		link.pathname = this.#path;
		link.hash = this.#keypair.export().privateKey;

		return link.toString();
	}

	async addGasForClaim(
		getAmount?: (options: {
			mist: bigint;
			objects: TransactionObjectInput[];
			estimatedFee: bigint;
		}) => Promise<bigint> | bigint,
	) {
		const estimatedFee = await this.#estimateClaimGasFee();
		this.#gasFee = getAmount
			? await getAmount({
					mist: this.#mist,
					objects: [...this.#objects],
					estimatedFee,
			  })
			: estimatedFee;
	}

	createSendTransaction() {
		const txb = new TransactionBlock();
		const address = this.#keypair.toSuiAddress();
		const objectsToTransfer = [...this.#objects].map((id) => txb.object(id));
		const totalMist = this.#mist + this.#gasFee;

		if (totalMist) {
			const [coin] = txb.splitCoins(txb.gas, [totalMist]);
			objectsToTransfer.push(coin);
		}

		txb.transferObjects(objectsToTransfer, address);

		return txb;
	}

	#estimateClaimGasFee(): Promise<bigint> {
		return Promise.resolve(0n);
	}
}

export interface ZkSendLinkOptions {
	keypair?: Keypair;
	client?: SuiClient;
}
export class ZkSendLink {
	#client: SuiClient;
	#keypair: Keypair;
	#initiallyOwnedObjects = new Set<string>();
	#ownedBalances = new Map<string, bigint>();
	#ownedObjects: Array<{
		objectId: string;
		version: string;
		digest: string;
		type: string;
	}> = [];

	constructor({
		client = DEFAULT_ZK_SEND_LINK_OPTIONS.client,
		keypair = new Ed25519Keypair(),
	}: ZkSendLinkOptions) {
		this.#client = client;
		this.#keypair = keypair;
	}

	static async fromUrl(url: string, options?: Omit<ZkSendLinkOptions, 'keypair'>) {
		const parsed = new URL(url);
		const keypair = Ed25519Keypair.fromSecretKey(fromB64(parsed.hash.slice(1)));

		const link = new ZkSendLink({
			...options,
			keypair,
		});

		await link.loadOwnedData();

		return link;
	}

	async loadOwnedData() {
		await Promise.all([
			this.#loadInitialTransactionData(),
			this.#loadOwnedObjects(),
			this.#loadOwnedBalances(),
		]);
	}

	async listClaimableAssets(
		address: string,
		options?: {
			claimObjectsAddedAfterCreation?: boolean;
			coinTypes?: string[];
			objects?: string[];
		},
	) {
		const normalizedAddress = normalizeSuiAddress(address);
		const txb = this.createClaimTransaction(normalizedAddress, options);

		const dryRun = await this.#client.dryRunTransactionBlock({
			transactionBlock: await txb.build({ client: this.#client }),
		});

		const balances: {
			coinType: string;
			amount: bigint;
		}[] = [];

		const nfts: {
			objectId: string;
			type: string;
			version: string;
			digest: string;
		}[] = [];

		dryRun.balanceChanges.forEach((balanceChange) => {
			if (BigInt(balanceChange.amount) > 0n && isOwner(balanceChange.owner, normalizedAddress)) {
				balances.push({ coinType: balanceChange.coinType, amount: BigInt(balanceChange.amount) });
			}
		});

		dryRun.objectChanges.forEach((objectChange) => {
			if ('objectType' in objectChange) {
				const type = parseStructTag(objectChange.objectType);

				if (
					type.address === normalizeSuiAddress('0x2') &&
					type.module === 'coin' &&
					type.name === 'Coin'
				) {
					return;
				}
			}

			if (ownedAfterChange(objectChange, normalizedAddress)) {
				nfts.push(objectChange);
			}
		});

		return {
			balances,
			nfts,
		};
	}

	async claimAssets(
		address: string,
		options?: {
			claimObjectsAddedAfterCreation?: boolean;
			coinTypes?: string[];
			objects?: string[];
		},
	) {
		return this.#client.signAndExecuteTransactionBlock({
			transactionBlock: await this.createClaimTransaction(address, options),
			signer: this.#keypair,
		});
	}

	createClaimTransaction(
		address: string,
		options?: {
			claimObjectsAddedAfterCreation?: boolean;
			coinTypes?: string[];
			objects?: string[];
		},
	) {
		const claimAll = !options?.coinTypes && !options?.objects;
		const txb = new TransactionBlock();
		txb.setSender(this.#keypair.toSuiAddress());
		const coinTypes = new Set(
			options?.coinTypes?.map((type) => normalizeStructTag(`0x2::coin::Coin<${type}>`)) ?? [],
		);

		const objectsToTransfer = this.#ownedObjects
			.filter((object) => {
				if (object.type === SUI_COIN_TYPE) {
					return false;
				}

				if (coinTypes?.has(object.type) || options?.objects?.includes(object.objectId)) {
					return true;
				}

				if (
					!options?.claimObjectsAddedAfterCreation &&
					!this.#initiallyOwnedObjects.has(object.objectId)
				) {
					return false;
				}

				return claimAll;
			})
			.map((object) => txb.object(object.objectId));

		if (claimAll || options?.coinTypes?.includes(SUI_COIN_TYPE)) {
			objectsToTransfer.push(txb.gas);
		}

		txb.transferObjects(objectsToTransfer, address);

		return txb;
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

			// RPC response returns cursor even if there are no more pages
			nextCursor = ownedObjects.hasNextPage ? ownedObjects.nextCursor : null;
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
				showObjectChanges: true,
			},
		});

		const address = this.#keypair.toSuiAddress();

		result.data[0]?.objectChanges?.forEach((objectChange) => {
			if (ownedAfterChange(objectChange, address)) {
				this.#initiallyOwnedObjects.add(normalizeSuiObjectId(objectChange.objectId));
			}
		});
	}
}

function ownedAfterChange(
	objectChange: SuiObjectChange,
	address: string,
): objectChange is Extract<SuiObjectChange, { type: 'created' | 'transferred' }> {
	if (objectChange.type === 'transferred' && isOwner(objectChange.recipient, address)) {
		return true;
	}

	if (objectChange.type === 'created' && isOwner(objectChange.owner, address)) {
		return true;
	}

	return false;
}

function isOwner(owner: ObjectOwner, address: string): owner is { AddressOwner: string } {
	return (
		owner &&
		typeof owner === 'object' &&
		'AddressOwner' in owner &&
		normalizeSuiAddress(owner.AddressOwner) === address
	);
}
