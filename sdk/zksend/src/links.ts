// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getFullnodeUrl, SuiClient } from '@mysten/sui.js/client';
import type { CoinStruct, ObjectOwner, SuiObjectChange } from '@mysten/sui.js/client';
import { decodeSuiPrivateKey } from '@mysten/sui.js/cryptography';
import type { Keypair, Signer } from '@mysten/sui.js/cryptography';
import { Ed25519Keypair } from '@mysten/sui.js/keypairs/ed25519';
import type { TransactionObjectInput } from '@mysten/sui.js/transactions';
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

interface ZkSendLinkRedirect {
	url: string;
	name?: string;
}

export interface ZkSendLinkBuilderOptions {
	host?: string;
	path?: string;
	mist?: number;
	keypair?: Keypair;
	client?: SuiClient;
	sender: string;
	redirect?: ZkSendLinkRedirect;
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

const SUI_COIN_TYPE = normalizeStructTag(SUI_TYPE_ARG);
const SUI_COIN_OBJECT_TYPE = normalizeStructTag('0x2::coin::Coin<0x2::sui::SUI>');

interface CreateZkSendLinkOptions {
	transactionBlock?: TransactionBlock;
	calculateGas?: (options: {
		balances: Map<string, bigint>;
		objects: TransactionObjectInput[];
		gasEstimateFromDryRun: bigint;
	}) => Promise<bigint> | bigint;
}

export class ZkSendLinkBuilder {
	#host: string;
	#path: string;
	#keypair: Keypair;
	#client: SuiClient;
	#redirect?: ZkSendLinkRedirect;
	#objects = new Set<TransactionObjectInput>();
	#balances = new Map<string, bigint>();
	#sender: string;

	#coinsByType = new Map<string, CoinStruct[]>();

	constructor({
		host = DEFAULT_ZK_SEND_LINK_OPTIONS.host,
		path = DEFAULT_ZK_SEND_LINK_OPTIONS.path,
		keypair = new Ed25519Keypair(),
		client = DEFAULT_ZK_SEND_LINK_OPTIONS.client,
		sender,
		redirect,
	}: ZkSendLinkBuilderOptions) {
		this.#host = host;
		this.#path = path;
		this.#redirect = redirect;
		this.#keypair = keypair;
		this.#client = client;
		this.#sender = normalizeSuiAddress(sender);
	}

	addClaimableMist(amount: bigint) {
		this.addClaimableBalance(SUI_COIN_TYPE, amount);
	}

	addClaimableBalance(coinType: string, amount: bigint) {
		const normalizedType = normalizeStructTag(coinType);
		this.#balances.set(normalizedType, (this.#balances.get(normalizedType) ?? 0n) + amount);
	}

	addClaimableObject(id: string) {
		this.#objects.add(id);
	}

	getLink(): string {
		const link = new URL(this.#host);
		link.pathname = this.#path;
		link.hash = toB64(decodeSuiPrivateKey(this.#keypair.getSecretKey()).secretKey);

		if (this.#redirect) {
			link.searchParams.set('redirect_url', this.#redirect.url);
			if (this.#redirect.name) {
				link.searchParams.set('name', this.#redirect.name);
			}
		}

		return link.toString();
	}

	async create({
		signer,
		...options
	}: CreateZkSendLinkOptions & {
		signer: Signer;
	}) {
		const txb = await this.createSendTransaction(options);

		return this.#client.signAndExecuteTransactionBlock({
			transactionBlock: await txb.build({ client: this.#client }),
			signer,
		});
	}
	async createSendTransaction({
		transactionBlock: txb = new TransactionBlock(),
		calculateGas,
	}: CreateZkSendLinkOptions = {}) {
		const gasEstimateFromDryRun = await this.#estimateClaimGasFee();
		const baseGasAmount = calculateGas
			? await calculateGas({
					balances: this.#balances,
					objects: [...this.#objects],
					gasEstimateFromDryRun,
			  })
			: gasEstimateFromDryRun * 2n;

		// Ensure that rounded gas is not less than the calculated gas
		const gasWithBuffer = baseGasAmount + 1013n;
		// Ensure that gas amount ends in 987
		const roundedGasAmount = gasWithBuffer - (gasWithBuffer % 1000n) - 13n;

		const address = this.#keypair.toSuiAddress();
		const objectsToTransfer = [...this.#objects].map((id) => txb.object(id));
		const [gas] = txb.splitCoins(txb.gas, [roundedGasAmount]);
		objectsToTransfer.push(gas);

		txb.setSenderIfNotSet(this.#sender);

		for (const [coinType, amount] of this.#balances) {
			if (coinType === SUI_COIN_TYPE) {
				const [sui] = txb.splitCoins(txb.gas, [amount]);
				objectsToTransfer.push(sui);
			} else {
				const coins = (await this.#getCoinsByType(coinType)).map((coin) => coin.coinObjectId);

				if (coins.length > 1) {
					txb.mergeCoins(coins[0], coins.slice(1));
				}
				const [split] = txb.splitCoins(coins[0], [amount]);
				objectsToTransfer.push(split);
			}
		}

		txb.transferObjects(objectsToTransfer, address);

		return txb;
	}

	async #estimateClaimGasFee(): Promise<bigint> {
		const txb = new TransactionBlock();
		txb.setSender(this.#sender);
		txb.setGasPayment([]);
		txb.transferObjects([txb.gas], this.#keypair.toSuiAddress());

		const idsToTransfer = [...this.#objects];

		for (const [coinType] of this.#balances) {
			const coins = await this.#getCoinsByType(coinType);

			if (!coins.length) {
				throw new Error(`Sending account does not contain any coins of type ${coinType}`);
			}

			idsToTransfer.push(coins[0].coinObjectId);
		}

		if (idsToTransfer.length > 0) {
			txb.transferObjects(
				idsToTransfer.map((id) => txb.object(id)),
				this.#keypair.toSuiAddress(),
			);
		}

		const result = await this.#client.dryRunTransactionBlock({
			transactionBlock: await txb.build({ client: this.#client }),
		});

		return (
			BigInt(result.effects.gasUsed.computationCost) +
			BigInt(result.effects.gasUsed.storageCost) -
			BigInt(result.effects.gasUsed.storageRebate)
		);
	}

	async #getCoinsByType(coinType: string) {
		if (this.#coinsByType.has(coinType)) {
			return this.#coinsByType.get(coinType)!;
		}

		const coins = await this.#client.getCoins({
			coinType,
			owner: this.#sender,
		});

		this.#coinsByType.set(coinType, coins.data);

		return coins.data;
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
	#ownedObjects: Array<{
		objectId: string;
		version: string;
		digest: string;
		type: string;
	}> = [];
	#gasCoin?: CoinStruct;
	#hasSui = false;
	#creatorAddress?: string;

	constructor({
		client = DEFAULT_ZK_SEND_LINK_OPTIONS.client,
		keypair = new Ed25519Keypair(),
	}: ZkSendLinkOptions & { linkAddress?: string }) {
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
		await Promise.all([this.#loadInitialTransactionData(), this.#loadOwnedObjects()]);
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
		const owner = this.#keypair.toSuiAddress();
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
		const address = this.#keypair.toSuiAddress();
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
