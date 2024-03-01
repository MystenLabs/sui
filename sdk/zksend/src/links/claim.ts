// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getFullnodeUrl, SuiClient } from '@mysten/sui.js/client';
import type {
	CoinStruct,
	DynamicFieldInfo,
	ObjectOwner,
	SuiObjectChange,
} from '@mysten/sui.js/client';
import type { Keypair } from '@mysten/sui.js/cryptography';
import { Ed25519Keypair } from '@mysten/sui.js/keypairs/ed25519';
import { TransactionBlock } from '@mysten/sui.js/transactions';
import {
	fromB64,
	normalizeStructTag,
	normalizeSuiAddress,
	normalizeSuiObjectId,
	parseStructTag,
	SUI_TYPE_ARG,
	toB64,
} from '@mysten/sui.js/utils';

import type { ZkBagContractOptions } from './zk-bag.js';
import { ZkBag } from './zk-bag.js';

const DEFAULT_ZK_SEND_LINK_OPTIONS = {
	host: 'https://zksend.com',
	path: '/claim',
	network: 'mainnet' as const,
	claimApi: 'https://zksend.com/api',
};

const SUI_COIN_TYPE = normalizeStructTag(SUI_TYPE_ARG);
const SUI_COIN_OBJECT_TYPE = normalizeStructTag('0x2::coin::Coin<0x2::sui::SUI>');

export interface ZkSendLinkOptions {
	claimApi?: string;
	keypair: Keypair;
	client?: SuiClient;
	network?: 'mainnet' | 'testnet';
	contract?: ZkBagContractOptions;
	linkAddress?: string;
}
export class ZkSendLink {
	#client: SuiClient;
	keypair: Keypair;
	#initiallyOwnedObjects = new Set<string>();
	#ownedObjects: Array<{
		objectId: string;
		version: string;
		digest: string;
		type: string;
	}> = [];
	#bagObjects: Array<DynamicFieldInfo> | null = null;
	#gasCoin?: CoinStruct;
	#hasSui = false;
	#creatorAddress?: string;
	#contract?: ZkBag<ZkBagContractOptions>;
	#claimApi: string;
	#network: 'mainnet' | 'testnet';

	constructor({
		network = DEFAULT_ZK_SEND_LINK_OPTIONS.network,
		claimApi = DEFAULT_ZK_SEND_LINK_OPTIONS.claimApi,
		client = new SuiClient({ url: getFullnodeUrl(network) }),
		keypair,
		contract,
	}: ZkSendLinkOptions) {
		this.#client = client;
		this.keypair = keypair;
		this.#claimApi = claimApi;
		this.#network = network;

		if (contract) {
			this.#contract = new ZkBag(contract.packageId, contract);
		}
	}

	static async fromUrl(url: string, options?: Omit<ZkSendLinkOptions, 'keypair' | 'linkAddress'>) {
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
			this.#loadBag(),
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
		if (!this.#contract || !this.#bagObjects) {
			return this.#listNonContractClaimableAssets(address, options);
		}

		const coins = [];

		const nfts: {
			objectId: string;
			type: string;
			version: string;
			digest: string;
		}[] = [];

		for (const object of this.#bagObjects) {
			const type = parseStructTag(object.objectType);

			if (
				type.address === normalizeSuiAddress('0x2') &&
				type.module === 'coin' &&
				type.name === 'Coin'
			) {
				coins.push(object);
			} else {
				nfts.push(object);
			}
		}

		const coinsWithContent = await this.#client.multiGetObjects({
			ids: coins.map((coin) => coin.objectId),
			options: {
				showContent: true,
			},
		});

		const balances = new Map<
			string,
			{
				coinType: string;
				amount: bigint;
			}
		>();

		coinsWithContent.forEach((coin) => {
			if (coin.data?.content?.dataType !== 'moveObject') {
				return;
			}

			const amount = BigInt((coin.data.content.fields as Record<string, string>).balance);
			const coinType = normalizeStructTag(parseStructTag(coin.data.content.type).typeParams[0]);

			if (!balances.has(coinType)) {
				balances.set(coinType, { coinType, amount });
			} else {
				balances.get(coinType)!.amount += amount;
			}
		});

		return {
			balances: [...balances.values()],
			nfts,
		};
	}

	async #listNonContractClaimableAssets(
		address: string,
		options?: {
			claimObjectsAddedAfterCreation?: boolean;
			coinTypes?: string[];
			objects?: string[];
		},
	) {
		const normalizedAddress = normalizeSuiAddress(address);
		const txb = this.createClaimTransaction(normalizedAddress, options);

		if (this.#gasCoin || !this.#hasSui) {
			txb.setGasPayment([]);
		}

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
				balances.push({
					coinType: normalizeStructTag(balanceChange.coinType),
					amount: BigInt(balanceChange.amount),
				});
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
			bag: this.#bagObjects,
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
		const txb = this.createClaimTransaction(address, options);
		if (!this.#contract || !this.#bagObjects) {
			return this.#client.signAndExecuteTransactionBlock({
				transactionBlock: txb,
				signer: this.keypair,
			});
		}

		const { digest } = await this.#executeSponsoredTransactionBlock(
			await this.#createSponsoredTransactionBlock(txb),
		);

		return this.#client.waitForTransactionBlock({ digest });
	}

	createClaimTransaction(
		address: string,
		options?: {
			claimObjectsAddedAfterCreation?: boolean;
			coinTypes?: string[];
			objects?: string[];
		},
	) {
		if (!this.#contract || !this.#bagObjects) {
			return this.#createNonContractClaimTransaction(address, options);
		}

		const txb = new TransactionBlock();
		const sender = this.keypair.toSuiAddress();
		txb.setSender(sender);

		const store = txb.object(this.#contract.ids.bagStoreId);

		const [bag, proof] = this.#contract.init_claim(txb, { arguments: [store, sender] });

		const objectsToTransfer = [];

		for (const object of this.#bagObjects) {
			objectsToTransfer.push(
				this.#contract.claim(txb, {
					arguments: [bag, proof, object.name.value as number],
					typeArguments: [object.objectType],
				}),
			);
		}

		this.#contract.finalize(txb, { arguments: [bag, proof] });
		if (objectsToTransfer.length > 0) {
			txb.transferObjects(objectsToTransfer, address);
		}

		return txb;
	}

	#createNonContractClaimTransaction(
		address: string,
		options?: {
			claimObjectsAddedAfterCreation?: boolean;
			coinTypes?: string[];
			objects?: string[];
		},
	) {
		const claimAll = !options?.coinTypes && !options?.objects;
		const txb = new TransactionBlock();
		txb.setSender(this.keypair.toSuiAddress());
		const coinTypes = new Set(
			options?.coinTypes?.map((type) => normalizeStructTag(`0x2::coin::Coin<${type}>`)) ?? [],
		);

		const objectsToTransfer = this.#ownedObjects
			.filter((object) => {
				if (this.#gasCoin) {
					if (object.objectId === this.#gasCoin.coinObjectId) {
						return false;
					}
				} else if (object.type === SUI_COIN_OBJECT_TYPE) {
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

		if (this.#gasCoin && this.#creatorAddress) {
			txb.transferObjects([txb.gas], this.#creatorAddress);
		} else if (claimAll || coinTypes?.has(SUI_COIN_TYPE)) {
			objectsToTransfer.push(txb.gas);
		}

		if (objectsToTransfer.length > 0) {
			txb.transferObjects(objectsToTransfer, address);
		}

		return txb;
	}

	async #loadOwnedObjects() {
		this.#ownedObjects = [];
		let nextCursor: string | null | undefined;
		const owner = this.keypair.toSuiAddress();
		do {
			const ownedObjects = await this.#client.getOwnedObjects({
				cursor: nextCursor,
				owner,
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

		const coins = await this.#client.getCoins({
			coinType: SUI_COIN_TYPE,
			owner,
		});

		this.#hasSui = coins.data.length > 0;
		this.#gasCoin = coins.data.find((coin) => BigInt(coin.balance) % 1000n === 987n);
	}

	async #loadInitialTransactionData() {
		const address = this.keypair.toSuiAddress();
		const result = await this.#client.queryTransactionBlocks({
			limit: 1,
			order: 'ascending',
			filter: {
				ToAddress: address,
			},
			options: {
				showObjectChanges: true,
				showInput: true,
			},
		});

		result.data[0]?.objectChanges?.forEach((objectChange) => {
			if (ownedAfterChange(objectChange, address)) {
				this.#initiallyOwnedObjects.add(normalizeSuiObjectId(objectChange.objectId));
			}
		});

		this.#creatorAddress = result.data[0]?.transaction?.data.sender;
	}

	async #loadBag() {
		if (!this.#contract) {
			return;
		}

		const bagField = await this.#client.getDynamicFieldObject({
			parentId: this.#contract.ids.bagStoreTableId,
			name: {
				type: 'address',
				value: this.keypair.toSuiAddress(),
			},
		});

		if (!bagField.data) {
			return;
		}

		const bagId: string | undefined = (bagField as any).data?.content?.fields?.value?.fields?.id
			?.id;

		if (!bagId) {
			throw new Error('Invalid bag field');
		}

		const objectsResponse = await this.#client.getDynamicFields({
			parentId: bagId,
		});

		this.#bagObjects = objectsResponse.data;
	}

	async #createSponsoredTransactionBlock(txb: TransactionBlock) {
		return this.#fetch<{ digest: string; bytes: string }>('transaction-blocks/sponsor', {
			method: 'POST',
			body: JSON.stringify({
				network: this.#network,
				sender: this.keypair.toSuiAddress(),
				transactionBlockKindBytes: toB64(
					await txb.build({
						onlyTransactionKind: true,
						client: this.#client,
						// Theses limits will get verified during the final transaction construction, so we can safely ignore them here:
						limits: {
							maxGasObjects: Infinity,
							maxPureArgumentSize: Infinity,
							maxTxGas: Infinity,
							maxTxSizeBytes: Infinity,
						},
					}),
				),
			}),
		});
	}

	async #executeSponsoredTransactionBlock(input: { digest: string; bytes: string }) {
		return this.#fetch<{ digest: string }>(`transaction-blocks/sponsor/${input.digest}`, {
			method: 'POST',
			body: JSON.stringify({
				signature: (await this.keypair.signTransactionBlock(fromB64(input.bytes))).signature,
			}),
		});
	}

	async #fetch<T = unknown>(path: string, init: RequestInit): Promise<T> {
		const res = await fetch(`${this.#claimApi}/v1/${path}`, {
			...init,
			headers: {
				...init.headers,
				'Content-Type': 'application/json',
			},
		});

		if (!res.ok) {
			throw new Error(`Request to claim API failed with status code ${res.status}`);
		}

		const { data } = await res.json();

		return data as T;
	}
}

function ownedAfterChange(
	objectChange: SuiObjectChange,
	address: string,
): objectChange is Extract<SuiObjectChange, { type: 'created' | 'transferred' | 'mutated' }> {
	if (objectChange.type === 'transferred' && isOwner(objectChange.recipient, address)) {
		return true;
	}

	if (
		(objectChange.type === 'created' || objectChange.type === 'mutated') &&
		isOwner(objectChange.owner, address)
	) {
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
