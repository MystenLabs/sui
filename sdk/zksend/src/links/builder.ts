// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getFullnodeUrl, SuiClient } from '@mysten/sui.js/client';
import type { CoinStruct } from '@mysten/sui.js/client';
import { decodeSuiPrivateKey } from '@mysten/sui.js/cryptography';
import type { Keypair, Signer } from '@mysten/sui.js/cryptography';
import { Ed25519Keypair } from '@mysten/sui.js/keypairs/ed25519';
import type {
	TransactionObjectArgument,
	TransactionObjectInput,
} from '@mysten/sui.js/transactions';
import { TransactionBlock } from '@mysten/sui.js/transactions';
import { normalizeStructTag, normalizeSuiAddress, SUI_TYPE_ARG, toB64 } from '@mysten/sui.js/utils';

import type { ZkBagContractOptions } from './zk-bag.js';
import { ZkBag } from './zk-bag.js';

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
	contract?: ZkBagContractOptions;
}

const DEFAULT_ZK_SEND_LINK_OPTIONS = {
	host: 'https://zksend.com',
	path: '/claim',
	network: 'mainnet' as const,
};

const SUI_COIN_TYPE = normalizeStructTag(SUI_TYPE_ARG);

export interface CreateZkSendLinkOptions {
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
	#objects = new Set<string>();
	#balances = new Map<string, bigint>();
	#sender: string;

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
		contract,
	}: ZkSendLinkBuilderOptions) {
		this.#host = host;
		this.#path = path;
		this.#redirect = redirect;
		this.#keypair = keypair;
		this.#client = client;
		this.#sender = normalizeSuiAddress(sender);

		if (contract) {
			this.#contract = new ZkBag(contract.packageId, contract);
		}
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
		link.hash = `${this.#contract ? '$' : ''}${toB64(
			decodeSuiPrivateKey(this.#keypair.getSecretKey()).secretKey,
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
		waitForTransactionBlock?: boolean;
	}) {
		const txb = await this.createSendTransaction(options);

		const result = await this.#client.signAndExecuteTransactionBlock({
			transactionBlock: await txb.build({ client: this.#client }),
			signer,
		});

		if (options.waitForTransactionBlock) {
			await this.#client.waitForTransactionBlock({ digest: result.digest });
		}

		return result;
	}
	async createSendTransaction({
		transactionBlock: txb = new TransactionBlock(),
		calculateGas,
	}: CreateZkSendLinkOptions = {}) {
		if (!this.#contract) {
			return this.#createSendTransactionWithoutContract({ transactionBlock: txb, calculateGas });
		}
		const receiver = txb.pure.address(this.#keypair.toSuiAddress());
		const store = txb.object(this.#contract.ids.bagStoreId);

		this.#contract.new(txb, { arguments: [store, receiver] });
		txb.setSenderIfNotSet(this.#sender);

		const objectsToTransfer = await this.#objectsToTransfer(txb);

		for (const object of objectsToTransfer) {
			this.#contract.add(txb, {
				arguments: [store, receiver, object.ref],
				typeArguments: [object.type],
			});
		}

		return txb;
	}

	async #objectsToTransfer(txb: TransactionBlock) {
		const objectIDs = [...this.#objects];
		const refsWithType: {
			ref: TransactionObjectArgument;
			type: string;
		}[] = (
			await this.#client.multiGetObjects({
				ids: objectIDs,
				options: {
					showType: true,
				},
			})
		).map((res, i) => {
			if (!res.data || res.error) {
				throw new Error(`Failed to load object ${objectIDs[i]} (${res.error?.code})`);
			}

			return {
				ref: txb.objectRef({
					version: res.data.version,
					digest: res.data.digest,
					objectId: res.data.objectId,
				}),
				type: res.data.type!,
			};
		});

		txb.setSenderIfNotSet(this.#sender);

		for (const [coinType, amount] of this.#balances) {
			if (coinType === SUI_COIN_TYPE) {
				const [sui] = txb.splitCoins(txb.gas, [amount]);
				refsWithType.push({
					ref: sui,
					type: `0x2::coin::Coin<${coinType}>`,
				} as never);
			} else {
				const coins = (await this.#getCoinsByType(coinType)).map((coin) => coin.coinObjectId);

				if (coins.length > 1) {
					txb.mergeCoins(coins[0], coins.slice(1));
				}
				const [split] = txb.splitCoins(coins[0], [amount]);
				refsWithType.push({
					ref: split,
					type: `0x2::coin:Coin<${coinType}>`,
				});
			}
		}

		return refsWithType;
	}

	async #createSendTransactionWithoutContract({
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
		const objectsToTransfer = (await this.#objectsToTransfer(txb)).map((obj) => obj.ref);
		const [gas] = txb.splitCoins(txb.gas, [roundedGasAmount]);
		objectsToTransfer.push(gas);

		txb.setSenderIfNotSet(this.#sender);
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
