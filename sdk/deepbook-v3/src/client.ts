// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { bcs } from '@mysten/sui/bcs';
import type { SuiClient } from '@mysten/sui/client';
import type { Signer } from '@mysten/sui/cryptography';
import { normalizeSuiAddress } from '@mysten/sui/utils';

import { BalanceManagerContract } from './transactions/balanceManager.js';
import { DeepBookContract } from './transactions/deepbook.js';
import type { BalanceManager, Environment } from './types/index.js';
import { DEEP_SCALAR, DeepBookConfig } from './utils/config.js';
import { getSignerFromPK } from './utils/utils.js';

/// DeepBook Client. If a private key is provided, then all transactions
/// will be signed with that key. Otherwise, the default key will be used.
/// Placing orders requires a balance manager to be set.
/// Client is initialized with default Coins and Pools. To trade on more pools,
/// new coins / pools must be added to the client.
export class DeepBookClient {
	#client: SuiClient;
	#signer: Signer;
	#balanceManagers: { [key: string]: BalanceManager } = {};
	#config: DeepBookConfig;
	#balanceManager: BalanceManagerContract;
	#deepBook: DeepBookContract;

	constructor({
		client,
		signer,
		env,
	}: {
		client: SuiClient;
		signer: string | Signer;
		env: Environment;
	}) {
		this.#client = client;
		this.#signer = typeof signer === 'string' ? getSignerFromPK(signer) : signer;
		this.#config = new DeepBookConfig({ client, signer: this.#signer, env });
		this.#balanceManager = new BalanceManagerContract(this.#config);
		this.#deepBook = new DeepBookContract(this.#config);
	}

	async init(mergeCoins: boolean) {
		await this.#config.init(mergeCoins);
	}

	addBalanceManager(managerKey: string, managerId: string, tradeCapId?: string) {
		this.#balanceManagers[managerKey] = {
			address: managerId,
			tradeCap: tradeCapId,
		};
	}

	async checkManagerBalance(managerKey: string, coinKey: string) {
		const balanceManager = this.#getBalanceManager(managerKey);
		const coin = this.#config.getCoin(coinKey);

		const tx = this.#balanceManager.checkManagerBalance(balanceManager.address, coin);
		const sender = normalizeSuiAddress(this.#signer.getPublicKey().toSuiAddress());
		const res = await this.#client.devInspectTransactionBlock({
			sender: sender,
			transactionBlock: tx,
		});

		const bytes = res.results![0].returnValues![0][0];
		const parsed_balance = bcs.U64.parse(new Uint8Array(bytes));
		const balanceNumber = Number(parsed_balance);
		const adjusted_balance = balanceNumber / coin.scalar;

		return {
			coinType: coin.type,
			balance: adjusted_balance,
		};
	}

	async whitelisted(poolKey: string) {
		const pool = this.#config.getPool(poolKey);

		const tx = this.#deepBook.whitelisted(pool);
		const res = await this.#config.client.devInspectTransactionBlock({
			sender: normalizeSuiAddress('0xa'),
			transactionBlock: tx,
		});

		const bytes = res.results![0].returnValues![0][0];
		const whitelisted = bcs.Bool.parse(new Uint8Array(bytes));

		return whitelisted;
	}

	async getQuoteQuantityOut(poolKey: string, baseQuantity: number) {
		const pool = this.#config.getPool(poolKey);

		const tx = this.#deepBook.getQuoteQuantityOut(pool, baseQuantity);
		const res = await this.#config.client.devInspectTransactionBlock({
			sender: normalizeSuiAddress('0xa'),
			transactionBlock: tx,
		});

		const baseOut = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![0][0])));
		const quoteOut = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![1][0])));
		const deepRequired = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![2][0])));

		return {
			baseQuantity,
			baseOut: baseOut / pool.baseCoin.scalar,
			quoteOut: quoteOut / pool.quoteCoin.scalar,
			deepRequired: deepRequired / DEEP_SCALAR,
		};
	}

	async getBaseQuantityOut(poolKey: string, baseQuantity: number) {
		const pool = this.#config.getPool(poolKey);

		const tx = this.#deepBook.getBaseQuantityOut(pool, baseQuantity);
		const res = await this.#config.client.devInspectTransactionBlock({
			sender: normalizeSuiAddress('0xa'),
			transactionBlock: tx,
		});

		const baseOut = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![0][0])));
		const quoteOut = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![1][0])));
		const deepRequired = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![2][0])));

		return {
			baseQuantity,
			baseOut: baseOut / pool.baseCoin.scalar,
			quoteOut: quoteOut / pool.quoteCoin.scalar,
			deepRequired: deepRequired / DEEP_SCALAR,
		};
	}

	async getQuantityOut(poolKey: string, baseQuantity: number, quoteQuantity: number) {
		const pool = this.#config.getPool(poolKey);

		const tx = this.#deepBook.getQuantityOut(pool, baseQuantity, quoteQuantity);
		const res = await this.#config.client.devInspectTransactionBlock({
			sender: normalizeSuiAddress('0xa'),
			transactionBlock: tx,
		});

		const baseOut = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![0][0])));
		const quoteOut = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![1][0])));
		const deepRequired = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![2][0])));

		return {
			baseQuantity,
			quoteQuantity,
			baseOut: baseOut / pool.baseCoin.scalar,
			quoteOut: quoteOut / pool.quoteCoin.scalar,
			deepRequired: deepRequired / DEEP_SCALAR,
		};
	}

	async accountOpenOrders(poolKey: string, managerKey: string) {
		const pool = this.#config.getPool(poolKey);

		const tx = this.#deepBook.accountOpenOrders(pool, managerKey);
		const res = await this.#config.client.devInspectTransactionBlock({
			sender: normalizeSuiAddress('0xa'),
			transactionBlock: tx,
		});

		const order_ids = res.results![0].returnValues![0][0];
		const VecSet = bcs.struct('VecSet', {
			constants: bcs.vector(bcs.U128),
		});

		return VecSet.parse(new Uint8Array(order_ids)).constants;
	}

	async getLevel2Range(poolKey: string, priceLow: number, priceHigh: number, isBid: boolean) {
		const pool = this.#config.getPool(poolKey);

		const tx = this.#deepBook.getLevel2Range(pool, priceLow, priceHigh, isBid);
		const res = await this.#config.client.devInspectTransactionBlock({
			sender: normalizeSuiAddress('0xa'),
			transactionBlock: tx,
		});

		const prices = res.results![0].returnValues![0][0];
		const parsed_prices = bcs.vector(bcs.u64()).parse(new Uint8Array(prices));
		const quantities = res.results![0].returnValues![1][0];
		const parsed_quantities = bcs.vector(bcs.u64()).parse(new Uint8Array(quantities));

		return {
			prices: parsed_prices,
			quantities: parsed_quantities,
		};
	}

	async getLevel2TicksFromMid(poolKey: string, ticks: number) {
		const pool = this.#config.getPool(poolKey);

		const tx = this.#deepBook.getLevel2TicksFromMid(pool, ticks);
		const res = await this.#config.client.devInspectTransactionBlock({
			sender: normalizeSuiAddress('0xa'),
			transactionBlock: tx,
		});

		const prices = res.results![0].returnValues![0][0];
		const parsed_prices = bcs.vector(bcs.u64()).parse(new Uint8Array(prices));
		const quantities = res.results![0].returnValues![1][0];
		const parsed_quantities = bcs.vector(bcs.u64()).parse(new Uint8Array(quantities));

		return {
			prices: parsed_prices,
			quantities: parsed_quantities,
		};
	}

	async vaultBalances(poolKey: string) {
		const pool = this.#config.getPool(poolKey);

		const tx = this.#deepBook.vaultBalances(pool);
		const res = await this.#config.client.devInspectTransactionBlock({
			sender: normalizeSuiAddress('0xa'),
			transactionBlock: tx,
		});

		const baseInVault = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![0][0])));
		const quoteInVault = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![1][0])));
		const deepInVault = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![2][0])));

		return {
			base: baseInVault / pool.baseCoin.scalar,
			quote: quoteInVault / pool.quoteCoin.scalar,
			deep: deepInVault / DEEP_SCALAR,
		};
	}

	async getPoolIdByAssets(baseType: string, quoteType: string) {
		const tx = this.#deepBook.getPoolIdByAssets(baseType, quoteType);

		const res = await this.#config.client.devInspectTransactionBlock({
			sender: normalizeSuiAddress('0xa'),
			transactionBlock: tx,
		});

		const ID = bcs.struct('ID', {
			bytes: bcs.Address,
		});
		const address = ID.parse(new Uint8Array(res.results![0].returnValues![0][0]))['bytes'];

		return address;
	}

	#getBalanceManager(managerKey: string): BalanceManager {
		if (!Object.hasOwn(this.#balanceManagers, managerKey)) {
			throw new Error(`Balance manager with key ${managerKey} not found.`);
		}

		return this.#balanceManagers[managerKey];
	}
}
