// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getFullnodeUrl, SuiClient } from '@mysten/sui.js/client';
import type {
	CoinStruct,
	ObjectOwner,
	SuiObjectChange,
	SuiParsedData,
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

import type { ZkSendLinkBuilderOptions } from './builder.js';
import { ZkSendLinkBuilder } from './builder.js';
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

export type ZkSendLinkOptions = {
	claimApi?: string;
	keypair?: Keypair;
	client?: SuiClient;
	network?: 'mainnet' | 'testnet';
	host?: string;
	path?: string;
	address?: string;
} & (
	| {
			address: string;
			keypair?: never;
	  }
	| {
			keypair: Keypair;
			address?: never;
	  }
) &
	(
		| {
				isContractLink: true;
				contract: ZkBagContractOptions;
		  }
		| {
				isContractLink: false;
				contract?: never;
		  }
	);

export class ZkSendLink {
	#client: SuiClient;
	address: string;
	keypair?: Keypair;
	creatorAddress?: string;
	#initiallyOwnedObjects = new Set<string>();
	#ownedObjects: Array<{
		objectId: string;
		version: string;
		digest: string;
		type: string;
	}> = [];
	#bagObjects: Array<{
		objectId: string;
		type: string;
		version: string;
		digest: string;
		content: SuiParsedData;
	}> | null = null;
	#gasCoin?: CoinStruct;
	#hasSui = false;

	#contract?: ZkBag<ZkBagContractOptions>;
	#claimApi: string;
	#network: 'mainnet' | 'testnet';
	#host?: string;
	#path?: string;

	constructor({
		network = DEFAULT_ZK_SEND_LINK_OPTIONS.network,
		claimApi = DEFAULT_ZK_SEND_LINK_OPTIONS.claimApi,
		client = new SuiClient({ url: getFullnodeUrl(network) }),
		keypair,
		contract,
		address,
		host,
		path,
		isContractLink,
	}: ZkSendLinkOptions) {
		if (!keypair && !address) {
			throw new Error('Either keypair or address must be provided');
		}

		this.#client = client;
		this.keypair = keypair;
		this.address = address ?? keypair!.toSuiAddress();
		this.#claimApi = claimApi;
		this.#network = network;
		this.#host = host;
		this.#path = path;

		if (isContractLink) {
			if (!contract) {
				throw new Error('Contract options are required for contract based links');
			}

			this.#contract = new ZkBag(contract.packageId, contract);
		}
	}

	static async fromUrl(
		url: string,
		{
			contract,
			...options
		}: Omit<ZkSendLinkOptions, 'keypair' | 'address' | 'isContractLink'> = {},
	) {
		const parsed = new URL(url);
		const isContractLink = parsed.hash.startsWith('#$');

		let link: ZkSendLink;
		if (isContractLink) {
			if (!contract) {
				throw new Error('Contract options are required for contract based links');
			}

			const keypair = Ed25519Keypair.fromSecretKey(fromB64(parsed.hash.slice(2)));
			link = new ZkSendLink({
				...options,
				keypair,
				host: `${parsed.protocol}//${parsed.host}`,
				path: parsed.pathname,
				isContractLink: true,
				contract,
			});
		} else {
			const keypair = Ed25519Keypair.fromSecretKey(
				fromB64(isContractLink ? parsed.hash.slice(2) : parsed.hash.slice(1)),
			);

			link = new ZkSendLink({
				...options,
				keypair,
				host: `${parsed.protocol}//${parsed.host}`,
				path: parsed.pathname,
				isContractLink: false,
			});
		}

		await link.loadOwnedData();

		return link;
	}

	static async fromAddress(
		address: string,
		options: Omit<ZkSendLinkOptions, 'keypair' | 'address' | 'isContractLink'> & {
			contract: ZkBagContractOptions;
		},
	) {
		const link = new ZkSendLink({
			...options,
			address,
			isContractLink: true,
		});

		await link.loadOwnedData();

		return link;
	}

	async loadOwnedData() {
		if (this.#contract) {
			await this.#loadBag();
		} else {
			await Promise.all([this.#loadInitialTransactionData(), this.#loadOwnedObjects()]);
		}
	}

	async listClaimableAssets(
		address: string,
		options?: {
			claimObjectsAddedAfterCreation?: boolean;
			coinTypes?: string[];
			objects?: string[];
		},
	) {
		if (!this.#contract) {
			return this.#listNonContractClaimableAssets(address, options);
		}

		const coins = [];

		const nfts: {
			objectId: string;
			type: string;
			version: string;
			digest: string;
		}[] = [];

		for (const object of this.#bagObjects ?? []) {
			const type = parseStructTag(object.type);

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

		const balances = new Map<
			string,
			{
				coinType: string;
				amount: bigint;
			}
		>();

		coins.forEach((coin) => {
			if (coin.content?.dataType !== 'moveObject') {
				return;
			}

			const amount = BigInt((coin.content.fields as Record<string, string>).balance);
			const coinType = normalizeStructTag(parseStructTag(coin.content.type).typeParams[0]);

			if (!balances.has(coinType)) {
				balances.set(coinType, { coinType, amount });
			} else {
				balances.get(coinType)!.amount += amount;
			}
		});

		return {
			balances: [...balances.values()],
			nfts,
			coins,
		};
	}

	async claimAssets(
		address: string,
		/** @deprecated filtering claims is not supported in contract based links */
		options?: {
			claimObjectsAddedAfterCreation?: boolean;
			coinTypes?: string[];
			objects?: string[];
		},
	) {
		if (!this.keypair) {
			throw new Error('Cannot claim assets without links keypair');
		}

		const txb = this.createClaimTransaction(address, options);
		if (!this.#contract || !this.#bagObjects) {
			return this.#client.signAndExecuteTransactionBlock({
				transactionBlock: txb,
				signer: this.keypair,
			});
		}

		const { digest } = await this.#executeSponsoredTransactionBlock(
			await this.#createSponsoredTransactionBlock(txb, address, this.keypair.toSuiAddress()),
			this.keypair,
		);

		return this.#client.waitForTransactionBlock({ digest });
	}

	createClaimTransaction(
		address: string,
		{
			reclaim,
			...options
		}: {
			/** @deprecated filtering claims is not supported in contract based links */
			claimObjectsAddedAfterCreation?: boolean;
			/** @deprecated filtering claims is not supported in contract based links */
			coinTypes?: string[];
			/** @deprecated filtering claims is not supported in contract based links */
			objects?: string[];
			reclaim?: boolean;
		} = {},
	) {
		if (!this.#contract) {
			return this.#createNonContractClaimTransaction(address, options);
		}

		if (Object.keys(options).length > 0) {
			throw new Error('Filtering claims is not supported for contract based links');
		}

		if (!this.keypair && !reclaim) {
			throw new Error('Cannot claim assets without the links keypair');
		}

		const txb = new TransactionBlock();
		const sender = reclaim ? address : this.keypair!.toSuiAddress();
		txb.setSender(sender);

		const store = txb.object(this.#contract.ids.bagStoreId);

		const [bag, proof] = reclaim
			? this.#contract.reclaim(txb, { arguments: [store, this.address] })
			: this.#contract.init_claim(txb, { arguments: [store] });

		const objectsToTransfer = [];

		for (const object of this.#bagObjects ?? []) {
			objectsToTransfer.push(
				this.#contract.claim(txb, {
					arguments: [
						bag,
						proof,
						txb.receivingRef({
							objectId: object.objectId,
							version: object.version,
							digest: object.digest,
						}),
					],
					typeArguments: [object.type],
				}),
			);
		}

		this.#contract.finalize(txb, { arguments: [bag, proof] });
		if (objectsToTransfer.length > 0) {
			txb.transferObjects(objectsToTransfer, address);
		}

		return txb;
	}

	async createRegenerateTransaction(
		sender: string,
		options: Omit<ZkSendLinkBuilderOptions, 'sender'> = {},
	) {
		if (!this.#contract || !this.#bagObjects) {
			throw new Error('Regenerating non-contract based links is not supported');
		}

		const txb = new TransactionBlock();
		txb.setSender(sender);

		const store = txb.object(this.#contract.ids.bagStoreId);

		const newLinkKp = Ed25519Keypair.generate();

		const newLink = new ZkSendLinkBuilder({
			...options,
			sender,
			client: this.#client,
			contract: this.#contract.ids,
			host: this.#host,
			path: this.#path,
			keypair: newLinkKp,
		});

		const to = txb.pure.address(newLinkKp.toSuiAddress());

		this.#contract.update_receiver(txb, { arguments: [store, this.address, to] });

		return {
			url: newLink.getLink(),
			transactionBlock: txb,
		};
	}

	async #loadBag() {
		if (!this.#contract) {
			return;
		}

		const bagField = await this.#client.getDynamicFieldObject({
			parentId: this.#contract.ids.bagStoreTableId,
			name: {
				type: 'address',
				value: this.address,
			},
		});

		if (!bagField.data) {
			return;
		}

		const itemIds: string[] | undefined = (bagField as any).data?.content?.fields?.value?.fields
			?.item_ids.fields.contents;

		this.creatorAddress = (bagField as any).data?.content?.fields?.value?.fields?.owner;

		if (!itemIds) {
			throw new Error('Invalid bag field');
		}

		const objectsResponse = await this.#client.multiGetObjects({
			ids: itemIds,
			options: {
				showType: true,
				showContent: true,
			},
		});

		this.#bagObjects = objectsResponse.map((object, i) => {
			if (!object.data) {
				throw new Error(`Failed to load claimable object ${itemIds[i]}`);
			}

			return {
				objectId: object.data.objectId,
				type: normalizeStructTag(object.data.type!),
				version: object.data.version,
				digest: object.data.digest,
				content: object.data.content!,
			};
		});
	}

	async #createSponsoredTransactionBlock(txb: TransactionBlock, claimer: string, sender: string) {
		return this.#fetch<{ digest: string; bytes: string }>('transaction-blocks/sponsor', {
			method: 'POST',
			body: JSON.stringify({
				network: this.#network,
				sender,
				claimer,
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

	async #executeSponsoredTransactionBlock(
		input: { digest: string; bytes: string },
		keypair: Keypair,
	) {
		return this.#fetch<{ digest: string }>(`transaction-blocks/sponsor/${input.digest}`, {
			method: 'POST',
			body: JSON.stringify({
				signature: (await keypair.signTransactionBlock(fromB64(input.bytes))).signature,
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
			console.error(await res.text());
			throw new Error(`Request to claim API failed with status code ${res.status}`);
		}

		const { data } = await res.json();

		return data as T;
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

		const coins: {
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
					if (ownedAfterChange(objectChange, normalizedAddress)) {
						coins.push(objectChange);
					}
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
			coins,
		};
	}

	#createNonContractClaimTransaction(
		address: string,
		options?: {
			claimObjectsAddedAfterCreation?: boolean;
			coinTypes?: string[];
			objects?: string[];
		},
	) {
		if (!this.keypair) {
			throw new Error('Cannot claim assets without the links keypair');
		}

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

		if (this.#gasCoin && this.creatorAddress) {
			txb.transferObjects([txb.gas], this.creatorAddress);
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
		do {
			const ownedObjects = await this.#client.getOwnedObjects({
				cursor: nextCursor,
				owner: this.address,
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
			owner: this.address,
		});

		this.#hasSui = coins.data.length > 0;
		this.#gasCoin = coins.data.find((coin) => BigInt(coin.balance) % 1000n === 987n);
	}

	async #loadInitialTransactionData() {
		const result = await this.#client.queryTransactionBlocks({
			limit: 1,
			order: 'ascending',
			filter: {
				ToAddress: this.address,
			},
			options: {
				showObjectChanges: true,
				showInput: true,
			},
		});

		result.data[0]?.objectChanges?.forEach((objectChange) => {
			if (ownedAfterChange(objectChange, this.address)) {
				this.#initiallyOwnedObjects.add(normalizeSuiObjectId(objectChange.objectId));
			}
		});

		this.creatorAddress = result.data[0]?.transaction?.data.sender;
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
