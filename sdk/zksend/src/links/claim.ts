// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { bcs } from '@mysten/sui/bcs';
import { getFullnodeUrl, SuiClient } from '@mysten/sui/client';
import type {
	CoinStruct,
	SuiObjectData,
	SuiTransaction,
	SuiTransactionBlockResponse,
} from '@mysten/sui/client';
import type { Keypair } from '@mysten/sui/cryptography';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';
import type { TransactionObjectArgument } from '@mysten/sui/transactions';
import { Transaction } from '@mysten/sui/transactions';
import {
	fromB64,
	normalizeStructTag,
	normalizeSuiAddress,
	normalizeSuiObjectId,
	parseStructTag,
	SUI_TYPE_ARG,
	toB64,
} from '@mysten/sui/utils';

import type { ZkSendLinkBuilderOptions } from './builder.js';
import { ZkSendLinkBuilder } from './builder.js';
import type { LinkAssets } from './utils.js';
import { getAssetsFromTransaction, isOwner, ownedAfterChange } from './utils.js';
import type { ZkBagContractOptions } from './zk-bag.js';
import { MAINNET_CONTRACT_IDS, ZkBag } from './zk-bag.js';

const DEFAULT_ZK_SEND_LINK_OPTIONS = {
	host: 'https://zksend.com',
	path: '/claim',
	network: 'mainnet' as const,
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
	isContractLink: boolean;
	contract?: ZkBagContractOptions | null;
} & (
	| {
			address: string;
			keypair?: never;
	  }
	| {
			keypair: Keypair;
			address?: never;
	  }
);

export class ZkSendLink {
	address: string;
	keypair?: Keypair;
	creatorAddress?: string;
	assets?: LinkAssets;
	claimed?: boolean;
	bagObject?: SuiObjectData | null;

	#client: SuiClient;
	#contract?: ZkBag<ZkBagContractOptions>;
	#network: 'mainnet' | 'testnet';
	#host: string;
	#path: string;
	#claimApi: string;

	// State for non-contract based links
	#gasCoin?: CoinStruct;
	#hasSui = false;
	#ownedObjects: {
		objectId: string;
		version: string;
		digest: string;
		type: string;
	}[] = [];

	constructor({
		network = DEFAULT_ZK_SEND_LINK_OPTIONS.network,
		client = new SuiClient({ url: getFullnodeUrl(network) }),
		keypair,
		contract = network === 'mainnet' ? MAINNET_CONTRACT_IDS : null,
		address,
		host = DEFAULT_ZK_SEND_LINK_OPTIONS.host,
		path = DEFAULT_ZK_SEND_LINK_OPTIONS.path,
		claimApi = `${host}/api`,
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
		options: Omit<ZkSendLinkOptions, 'keypair' | 'address' | 'isContractLink'> = {},
	) {
		const parsed = new URL(url);
		const isContractLink = parsed.hash.startsWith('#$');

		let link: ZkSendLink;
		if (isContractLink) {
			const keypair = Ed25519Keypair.fromSecretKey(fromB64(parsed.hash.slice(2)));
			link = new ZkSendLink({
				...options,
				keypair,
				host: `${parsed.protocol}//${parsed.host}`,
				path: parsed.pathname,
				isContractLink: true,
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

		await link.loadAssets();

		return link;
	}

	static async fromAddress(
		address: string,
		options: Omit<ZkSendLinkOptions, 'keypair' | 'address' | 'isContractLink'>,
	) {
		const link = new ZkSendLink({
			...options,
			address,
			isContractLink: true,
		});

		await link.loadAssets();

		return link;
	}

	async loadClaimedStatus() {
		await this.#loadBag({ loadAssets: false });
	}

	async loadAssets(
		options: {
			transaction?: SuiTransactionBlockResponse;
			loadClaimedAssets?: boolean;
		} = {},
	) {
		if (this.#contract) {
			await this.#loadBag(options);
		} else {
			await this.#loadOwnedObjects(options);
		}
	}

	async claimAssets(
		address: string,
		{
			reclaim,
			sign,
		}:
			| { reclaim?: false; sign?: never }
			| {
					reclaim: true;
					sign: (transaction: Uint8Array) => Promise<string>;
			  } = {},
	) {
		if (!this.keypair && !sign) {
			throw new Error('Cannot claim assets without links keypair');
		}

		if (this.claimed) {
			throw new Error('Assets have already been claimed');
		}

		if (!this.#contract) {
			const bytes = await this.createClaimTransaction(address).build({
				client: this.#client,
			});
			const signature = sign
				? await sign(bytes)
				: (await this.keypair!.signTransaction(bytes)).signature;

			return this.#client.executeTransactionBlock({
				transactionBlock: bytes,
				signature,
			});
		}

		if (!this.assets) {
			await this.#loadBag();
		}

		const tx = this.createClaimTransaction(address, { reclaim });

		const sponsored = await this.#createSponsoredTransaction(
			tx,
			address,
			reclaim ? address : this.keypair!.toSuiAddress(),
		);

		const bytes = fromB64(sponsored.bytes);
		const signature = sign
			? await sign(bytes)
			: (await this.keypair!.signTransaction(bytes)).signature;

		const { digest } = await this.#executeSponsoredTransaction(sponsored, signature);

		return this.#client.waitForTransaction({ digest });
	}

	createClaimTransaction(
		address: string,
		{
			reclaim,
		}: {
			reclaim?: boolean;
		} = {},
	) {
		if (!this.#contract) {
			return this.#createNonContractClaimTransaction(address);
		}

		if (!this.keypair && !reclaim) {
			throw new Error('Cannot claim assets without the links keypair');
		}

		const tx = new Transaction();
		const sender = reclaim ? address : this.keypair!.toSuiAddress();
		tx.setSender(sender);

		const store = tx.object(this.#contract.ids.bagStoreId);
		const command = reclaim
			? this.#contract.reclaim({ arguments: [store, this.address] })
			: this.#contract.init_claim({ arguments: [store] });

		const [bag, proof] = tx.add(command);

		const objectsToTransfer: TransactionObjectArgument[] = [];

		const objects = [...(this.assets?.coins ?? []), ...(this.assets?.nfts ?? [])];

		for (const object of objects) {
			objectsToTransfer.push(
				this.#contract.claim({
					arguments: [
						bag,
						proof,
						tx.receivingRef({
							objectId: object.objectId,
							version: object.version,
							digest: object.digest,
						}),
					],
					typeArguments: [object.type],
				}),
			);
		}

		if (objectsToTransfer.length > 0) {
			tx.transferObjects(objectsToTransfer, address);
		}

		tx.add(this.#contract.finalize({ arguments: [bag, proof] }));

		return tx;
	}

	async createRegenerateTransaction(
		sender: string,
		options: Omit<ZkSendLinkBuilderOptions, 'sender'> = {},
	) {
		if (!this.assets) {
			await this.#loadBag();
		}

		if (this.claimed) {
			throw new Error('Assets have already been claimed');
		}

		if (!this.#contract) {
			throw new Error('Regenerating non-contract based links is not supported');
		}

		const tx = new Transaction();
		tx.setSender(sender);

		const store = tx.object(this.#contract.ids.bagStoreId);

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

		const to = tx.pure.address(newLinkKp.toSuiAddress());

		tx.add(this.#contract.update_receiver({ arguments: [store, this.address, to] }));

		return {
			url: newLink.getLink(),
			transaction: tx,
		};
	}

	async #loadBagObject() {
		if (!this.#contract) {
			throw new Error('Cannot load bag object for non-contract based links');
		}
		const bagField = await this.#client.getDynamicFieldObject({
			parentId: this.#contract.ids.bagStoreTableId,
			name: {
				type: 'address',
				value: this.address,
			},
		});

		this.bagObject = bagField.data;
		this.claimed = !bagField.data;
	}

	async #loadBag({
		transaction,
		loadAssets = true,
		loadClaimedAssets = loadAssets,
	}: {
		transaction?: SuiTransactionBlockResponse;
		loadAssets?: boolean;
		loadClaimedAssets?: boolean;
	} = {}) {
		if (!this.#contract) {
			return;
		}

		this.assets = {
			balances: [],
			nfts: [],
			coins: [],
		};

		if (!this.bagObject || !this.claimed) {
			await this.#loadBagObject();
		}

		if (!loadAssets) {
			return;
		}

		if (!this.bagObject) {
			if (loadClaimedAssets) {
				await this.#loadClaimedAssets();
			}
			return;
		}

		const bagId = (this.bagObject as any).content.fields.value.fields?.id?.id;

		if (bagId && transaction?.balanceChanges && transaction.objectChanges) {
			this.assets = getAssetsFromTransaction({
				transaction,
				address: bagId,
				isSent: false,
			});

			return;
		}

		const itemIds: string[] | undefined = (this.bagObject as any)?.content?.fields?.value?.fields
			?.item_ids.fields.contents;

		this.creatorAddress = (this.bagObject as any)?.content?.fields?.value?.fields?.owner;

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

		const balances = new Map<
			string,
			{
				coinType: string;
				amount: bigint;
			}
		>();

		objectsResponse.forEach((object, i) => {
			if (!object.data || !object.data.type) {
				throw new Error(`Failed to load claimable object ${itemIds[i]}`);
			}

			const type = parseStructTag(normalizeStructTag(object.data.type));

			if (
				type.address === normalizeSuiAddress('0x2') &&
				type.module === 'coin' &&
				type.name === 'Coin'
			) {
				this.assets!.coins.push({
					objectId: object.data.objectId,
					type: object.data.type,
					version: object.data.version,
					digest: object.data.digest,
				});

				if (object.data.content?.dataType === 'moveObject') {
					const amount = BigInt((object.data.content.fields as Record<string, string>).balance);
					const coinType = normalizeStructTag(
						parseStructTag(object.data.content.type).typeParams[0],
					);
					if (!balances.has(coinType)) {
						balances.set(coinType, { coinType, amount });
					} else {
						balances.get(coinType)!.amount += amount;
					}
				}
			} else {
				this.assets!.nfts.push({
					objectId: object.data.objectId,
					type: object.data.type,
					version: object.data.version,
					digest: object.data.digest,
				});
			}
		});

		this.assets.balances = [...balances.values()];
	}

	async #loadClaimedAssets() {
		const result = await this.#client.queryTransactionBlocks({
			limit: 1,
			filter: {
				FromAddress: this.address,
			},
			options: {
				showObjectChanges: true,
				showBalanceChanges: true,
				showInput: true,
			},
		});

		if (!result?.data[0]) {
			return;
		}

		const [tx] = result.data;

		if (tx.transaction?.data.transaction.kind !== 'ProgrammableTransaction') {
			return;
		}

		const transfer = tx.transaction.data.transaction.transactions.findLast(
			(tx): tx is Extract<SuiTransaction, { TransferObjects: unknown }> => 'TransferObjects' in tx,
		);

		if (!transfer) {
			return;
		}

		const receiverArg = transfer.TransferObjects[1];

		if (!(typeof receiverArg === 'object' && 'Input' in receiverArg)) {
			return;
		}

		const input = tx.transaction.data.transaction.inputs[receiverArg.Input];

		if (input.type !== 'pure') {
			return;
		}

		const receiver =
			typeof input.value === 'string'
				? input.value
				: bcs.Address.parse(new Uint8Array((input.value as { Pure: number[] }).Pure));

		this.assets = getAssetsFromTransaction({
			transaction: tx,
			address: receiver,
			isSent: false,
		});
	}

	async #createSponsoredTransaction(tx: Transaction, claimer: string, sender: string) {
		return this.#fetch<{ digest: string; bytes: string }>('transaction-blocks/sponsor', {
			method: 'POST',
			body: JSON.stringify({
				network: this.#network,
				sender,
				claimer,
				transactionBlockKindBytes: toB64(
					await tx.build({
						onlyTransactionKind: true,
						client: this.#client,
					}),
				),
			}),
		});
	}

	async #executeSponsoredTransaction(input: { digest: string; bytes: string }, signature: string) {
		return this.#fetch<{ digest: string }>(`transaction-blocks/sponsor/${input.digest}`, {
			method: 'POST',
			body: JSON.stringify({
				signature,
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
			console.error(path, await res.text());
			throw new Error(`Request to claim API failed with status code ${res.status}`);
		}

		const { data } = await res.json();

		return data as T;
	}

	async #listNonContractClaimableAssets() {
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

		if (this.#ownedObjects.length === 0 && !this.#hasSui) {
			return {
				balances,
				nfts,
				coins,
			};
		}

		const address = new Ed25519Keypair().toSuiAddress();
		const normalizedAddress = normalizeSuiAddress(address);

		const tx = this.createClaimTransaction(normalizedAddress);

		if (this.#gasCoin || !this.#hasSui) {
			tx.setGasPayment([]);
		}

		const dryRun = await this.#client.dryRunTransactionBlock({
			transactionBlock: await tx.build({ client: this.#client }),
		});

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

	#createNonContractClaimTransaction(address: string) {
		if (!this.keypair) {
			throw new Error('Cannot claim assets without the links keypair');
		}

		const tx = new Transaction();
		tx.setSender(this.keypair.toSuiAddress());

		const objectsToTransfer: TransactionObjectArgument[] = this.#ownedObjects
			.filter((object) => {
				if (this.#gasCoin) {
					if (object.objectId === this.#gasCoin.coinObjectId) {
						return false;
					}
				} else if (object.type === SUI_COIN_OBJECT_TYPE) {
					return false;
				}

				return true;
			})
			.map((object) => tx.object(object.objectId));

		if (this.#gasCoin && this.creatorAddress) {
			tx.transferObjects([tx.gas], this.creatorAddress);
		} else {
			objectsToTransfer.push(tx.gas);
		}

		if (objectsToTransfer.length > 0) {
			tx.transferObjects(objectsToTransfer, address);
		}

		return tx;
	}

	async #loadOwnedObjects({
		loadClaimedAssets = true,
	}: {
		loadClaimedAssets?: boolean;
	} = {}) {
		this.assets = {
			nfts: [],
			balances: [],
			coins: [],
		};

		let nextCursor: string | null | undefined;
		do {
			const ownedObjects = await this.#client.getOwnedObjects({
				cursor: nextCursor,
				owner: this.address,
				options: {
					showType: true,
					showContent: true,
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

		const result = await this.#client.queryTransactionBlocks({
			limit: 1,
			order: 'ascending',
			filter: {
				ToAddress: this.address,
			},
			options: {
				showInput: true,
				showBalanceChanges: true,
				showObjectChanges: true,
			},
		});

		this.creatorAddress = result.data[0]?.transaction?.data.sender;

		if (this.#hasSui || this.#ownedObjects.length > 0) {
			this.claimed = false;
			this.assets = await this.#listNonContractClaimableAssets();
		} else if (result.data[0] && loadClaimedAssets) {
			this.claimed = true;
			await this.#loadClaimedAssets();
		}
	}
}
