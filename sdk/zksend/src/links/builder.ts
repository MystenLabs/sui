// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getFullnodeUrl, SuiClient } from '@mysten/sui/client';
import type { CoinStruct } from '@mysten/sui/client';
import { decodeSuiPrivateKey } from '@mysten/sui/cryptography';
import type { Keypair, Signer } from '@mysten/sui/cryptography';
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';
import type { TransactionObjectArgument, TransactionObjectInput } from '@mysten/sui/transactions';
import { Transaction } from '@mysten/sui/transactions';
import { normalizeStructTag, normalizeSuiAddress, SUI_TYPE_ARG, toB64 } from '@mysten/sui/utils';

import type { ZkBagContractOptions } from './zk-bag.js';
import { MAINNET_CONTRACT_IDS, ZkBag } from './zk-bag.js';

interface ZkSendLinkRedirect {
	url: string;
	name?: string;
}

export interface ZkSendLinkBuilderOptions {
	host?: string;
	path?: string;
	keypair?: Keypair;
	network?: 'mainnet' | 'testnet';
	client?: SuiClient;
	sender: string;
	redirect?: ZkSendLinkRedirect;
	contract?: ZkBagContractOptions | null;
}

const DEFAULT_ZK_SEND_LINK_OPTIONS = {
	host: 'https://zksend.com',
	path: '/claim',
	network: 'mainnet' as const,
};

const SUI_COIN_TYPE = normalizeStructTag(SUI_TYPE_ARG);

export interface CreateZkSendLinkOptions {
	transaction?: Transaction;
	calculateGas?: (options: {
		balances: Map<string, bigint>;
		objects: TransactionObjectInput[];
		gasEstimateFromDryRun: bigint;
	}) => Promise<bigint> | bigint;
}

export class ZkSendLinkBuilder {
	objectIds = new Set<string>();
	objectRefs: {
		ref: TransactionObjectArgument;
		type: string;
	}[] = [];
	balances = new Map<string, bigint>();
	sender: string;
	#host: string;
	#path: string;
	keypair: Keypair;
	#client: SuiClient;
	#redirect?: ZkSendLinkRedirect;
	#coinsByType = new Map<string, CoinStruct[]>();
	#contract?: ZkBag<ZkBagContractOptions>;

	constructor({
		host = DEFAULT_ZK_SEND_LINK_OPTIONS.host,
		path = DEFAULT_ZK_SEND_LINK_OPTIONS.path,
		keypair = new Ed25519Keypair(),
		network = DEFAULT_ZK_SEND_LINK_OPTIONS.network,
		client = new SuiClient({ url: getFullnodeUrl(network) }),
		sender,
		redirect,
		contract = network === 'mainnet' ? MAINNET_CONTRACT_IDS : undefined,
	}: ZkSendLinkBuilderOptions) {
		this.#host = host;
		this.#path = path;
		this.#redirect = redirect;
		this.keypair = keypair;
		this.#client = client;
		this.sender = normalizeSuiAddress(sender);

		if (contract) {
			this.#contract = new ZkBag(contract.packageId, contract);
		}
	}

	addClaimableMist(amount: bigint) {
		this.addClaimableBalance(SUI_COIN_TYPE, amount);
	}

	addClaimableBalance(coinType: string, amount: bigint) {
		const normalizedType = normalizeStructTag(coinType);
		this.balances.set(normalizedType, (this.balances.get(normalizedType) ?? 0n) + amount);
	}

	addClaimableObject(id: string) {
		this.objectIds.add(id);
	}

	addClaimableObjectRef(ref: TransactionObjectArgument, type: string) {
		this.objectRefs.push({ ref, type });
	}

	getLink(): string {
		const link = new URL(this.#host);
		link.pathname = this.#path;
		link.hash = `${this.#contract ? '$' : ''}${toB64(
			decodeSuiPrivateKey(this.keypair.getSecretKey()).secretKey,
		)}`;

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
		waitForTransaction?: boolean;
	}) {
		const tx = await this.createSendTransaction(options);

		const result = await this.#client.signAndExecuteTransaction({
			transaction: await tx.build({ client: this.#client }),
			signer,
		});

		if (options.waitForTransaction) {
			await this.#client.waitForTransaction({ digest: result.digest });
		}

		return result;
	}
	async createSendTransaction({
		transaction = new Transaction(),
		calculateGas,
	}: CreateZkSendLinkOptions = {}) {
		if (!this.#contract) {
			return this.#createSendTransactionWithoutContract({ transaction, calculateGas });
		}

		transaction.setSenderIfNotSet(this.sender);

		return ZkSendLinkBuilder.createLinks({
			transaction,
			client: this.#client,
			contract: this.#contract.ids,
			links: [this],
		});
	}

	async createSendToAddressTransaction({
		transaction = new Transaction(),
		address,
	}: CreateZkSendLinkOptions & {
		address: string;
	}) {
		const objectsToTransfer = (await this.#objectsToTransfer(transaction)).map((obj) => obj.ref);

		transaction.setSenderIfNotSet(this.sender);
		transaction.transferObjects(objectsToTransfer, address);

		return transaction;
	}

	async #objectsToTransfer(tx: Transaction) {
		const objectIDs = [...this.objectIds];
		const refsWithType = this.objectRefs.concat(
			(objectIDs.length > 0
				? await this.#client.multiGetObjects({
						ids: objectIDs,
						options: {
							showType: true,
						},
					})
				: []
			).map((res, i) => {
				if (!res.data || res.error) {
					throw new Error(`Failed to load object ${objectIDs[i]} (${res.error?.code})`);
				}

				return {
					ref: tx.objectRef({
						version: res.data.version,
						digest: res.data.digest,
						objectId: res.data.objectId,
					}),
					type: res.data.type!,
				};
			}),
		);

		for (const [coinType, amount] of this.balances) {
			if (coinType === SUI_COIN_TYPE) {
				const [sui] = tx.splitCoins(tx.gas, [amount]);
				refsWithType.push({
					ref: sui,
					type: `0x2::coin::Coin<${coinType}>`,
				} as never);
			} else {
				const coins = (await this.#getCoinsByType(coinType)).map((coin) => coin.coinObjectId);

				if (coins.length > 1) {
					tx.mergeCoins(coins[0], coins.slice(1));
				}
				const [split] = tx.splitCoins(coins[0], [amount]);
				refsWithType.push({
					ref: split,
					type: `0x2::coin::Coin<${coinType}>`,
				});
			}
		}

		return refsWithType;
	}

	async #createSendTransactionWithoutContract({
		transaction: tx = new Transaction(),
		calculateGas,
	}: CreateZkSendLinkOptions = {}) {
		const gasEstimateFromDryRun = await this.#estimateClaimGasFee();
		const baseGasAmount = calculateGas
			? await calculateGas({
					balances: this.balances,
					objects: [...this.objectIds],
					gasEstimateFromDryRun,
				})
			: gasEstimateFromDryRun * 2n;

		// Ensure that rounded gas is not less than the calculated gas
		const gasWithBuffer = baseGasAmount + 1013n;
		// Ensure that gas amount ends in 987
		const roundedGasAmount = gasWithBuffer - (gasWithBuffer % 1000n) - 13n;

		const address = this.keypair.toSuiAddress();
		const objectsToTransfer = (await this.#objectsToTransfer(tx)).map((obj) => obj.ref);
		const [gas] = tx.splitCoins(tx.gas, [roundedGasAmount]);
		objectsToTransfer.push(gas);

		tx.setSenderIfNotSet(this.sender);
		tx.transferObjects(objectsToTransfer, address);

		return tx;
	}

	async #estimateClaimGasFee(): Promise<bigint> {
		const tx = new Transaction();
		tx.setSender(this.sender);
		tx.setGasPayment([]);
		tx.transferObjects([tx.gas], this.keypair.toSuiAddress());

		const idsToTransfer = [...this.objectIds];

		for (const [coinType] of this.balances) {
			const coins = await this.#getCoinsByType(coinType);

			if (!coins.length) {
				throw new Error(`Sending account does not contain any coins of type ${coinType}`);
			}

			idsToTransfer.push(coins[0].coinObjectId);
		}

		if (idsToTransfer.length > 0) {
			tx.transferObjects(
				idsToTransfer.map((id) => tx.object(id)),
				this.keypair.toSuiAddress(),
			);
		}

		const result = await this.#client.dryRunTransactionBlock({
			transactionBlock: await tx.build({ client: this.#client }),
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
			owner: this.sender,
		});

		this.#coinsByType.set(coinType, coins.data);

		return coins.data;
	}

	static async createLinks({
		links,
		network = 'mainnet',
		client = new SuiClient({ url: getFullnodeUrl(network) }),
		transaction = new Transaction(),
		contract: contractIds = MAINNET_CONTRACT_IDS,
	}: {
		transaction?: Transaction;
		client?: SuiClient;
		network?: 'mainnet' | 'testnet';
		links: ZkSendLinkBuilder[];
		contract?: ZkBagContractOptions;
	}) {
		const contract = new ZkBag(contractIds.packageId, contractIds);
		const store = transaction.object(contract.ids.bagStoreId);

		const coinsByType = new Map<string, CoinStruct[]>();
		const allIds = links.flatMap((link) => [...link.objectIds]);
		const sender = links[0].sender;
		transaction.setSenderIfNotSet(sender);

		await Promise.all(
			[...new Set(links.flatMap((link) => [...link.balances.keys()]))].map(async (coinType) => {
				const coins = await client.getCoins({
					coinType,
					owner: sender,
				});

				coinsByType.set(
					coinType,
					coins.data.filter((coin) => !allIds.includes(coin.coinObjectId)),
				);
			}),
		);

		const objectRefs = new Map<
			string,
			{
				ref: TransactionObjectArgument;
				type: string;
			}
		>();

		const pageSize = 50;
		let offset = 0;
		while (offset < allIds.length) {
			let chunk = allIds.slice(offset, offset + pageSize);
			offset += pageSize;

			const objects = await client.multiGetObjects({
				ids: chunk,
				options: {
					showType: true,
				},
			});

			for (const [i, res] of objects.entries()) {
				if (!res.data || res.error) {
					throw new Error(`Failed to load object ${chunk[i]} (${res.error?.code})`);
				}
				objectRefs.set(chunk[i], {
					ref: transaction.objectRef({
						version: res.data.version,
						digest: res.data.digest,
						objectId: res.data.objectId,
					}),
					type: res.data.type!,
				});
			}
		}

		const mergedCoins = new Map<string, TransactionObjectArgument>([
			[SUI_COIN_TYPE, transaction.gas],
		]);

		for (const [coinType, coins] of coinsByType) {
			if (coinType === SUI_COIN_TYPE) {
				continue;
			}

			const [first, ...rest] = coins.map((coin) =>
				transaction.objectRef({
					objectId: coin.coinObjectId,
					version: coin.version,
					digest: coin.digest,
				}),
			);
			if (rest.length > 0) {
				transaction.mergeCoins(first, rest);
			}
			mergedCoins.set(coinType, transaction.object(first));
		}

		for (const link of links) {
			const receiver = link.keypair.toSuiAddress();
			transaction.add(contract.new({ arguments: [store, receiver] }));

			link.objectRefs.forEach(({ ref, type }) => {
				transaction.add(
					contract.add({
						arguments: [store, receiver, ref],
						typeArguments: [type],
					}),
				);
			});

			link.objectIds.forEach((id) => {
				const object = objectRefs.get(id);
				if (!object) {
					throw new Error(`Object ${id} not found`);
				}
				transaction.add(
					contract.add({
						arguments: [store, receiver, object.ref],
						typeArguments: [object.type],
					}),
				);
			});
		}

		for (const [coinType, merged] of mergedCoins) {
			const linksWithCoin = links.filter((link) => link.balances.has(coinType));
			if (linksWithCoin.length === 0) {
				continue;
			}

			const balances = linksWithCoin.map((link) => link.balances.get(coinType)!);
			const splits = transaction.splitCoins(merged, balances);
			for (const [i, link] of linksWithCoin.entries()) {
				transaction.add(
					contract.add({
						arguments: [store, link.keypair.toSuiAddress(), splits[i]],
						typeArguments: [`0x2::coin::Coin<${coinType}>`],
					}),
				);
			}
		}

		return transaction;
	}
}
