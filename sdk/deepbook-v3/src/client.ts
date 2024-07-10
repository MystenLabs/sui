// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { bcs } from '@mysten/sui/bcs';
import { getFullnodeUrl, SuiClient } from '@mysten/sui/client';
import type { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';
import type { Secp256k1Keypair } from '@mysten/sui/keypairs/secp256k1';
import type { Secp256r1Keypair } from '@mysten/sui/keypairs/secp256r1';
import { Transaction } from '@mysten/sui/transactions';
import { normalizeSuiAddress } from '@mysten/sui/utils';

import {
	checkManagerBalance,
	createAndShareBalanceManager,
	depositIntoManager,
	withdrawAllFromManager,
	withdrawFromManager,
} from './transactions/balanceManager.js';
import {
	accountOpenOrders,
	addDeepPricePoint,
	burnDeep,
	cancelAllOrders,
	cancelOrder,
	claimRebates,
	getBaseQuantityOut,
	getLevel2Range,
	getLevel2TicksFromMid,
	getPoolIdByAssets,
	getQuoteQuantityOut,
	midPrice,
	placeLimitOrder,
	placeMarketOrder,
	swapExactBaseForQuote,
	swapExactQuoteForBase,
	vaultBalances,
	whitelisted,
} from './transactions/deepbook.js';
import {
	createPoolAdmin,
	unregisterPoolAdmin,
	updateDisabledVersions,
} from './transactions/deepbookAdmin.js';
import { stake, submitProposal, unstake, vote } from './transactions/governance.js';
import type {
	BalanceManager,
	CreatePoolAdminParams,
	PlaceLimitOrderParams,
	PlaceMarketOrderParams,
	PoolKey,
	ProposalParams,
	SwapParams,
} from './types/index.js';
import { CoinKey, OrderType, SelfMatchingOptions } from './types/index.js';
import { DeepBookConfig, MAX_TIMESTAMP } from './utils/config.js';
import { getSigner, getSignerFromPK, signAndExecuteWithClientAndSigner } from './utils/utils.js';

/// DeepBook Client. If a private key is provided, then all transactions
/// will be signed with that key. Otherwise, the default key will be used.
/// Placing orders requires a balance manager to be set.
/// Client is initialized with default Coins and Pools. To trade on more pools,
/// new coins / pools must be added to the client.
export class DeepBookClient {
	#client: SuiClient;
	#signer: Ed25519Keypair | Secp256k1Keypair | Secp256r1Keypair;
	#balanceManagers: { [key: string]: BalanceManager } = {};
	#config: DeepBookConfig = new DeepBookConfig();

	constructor(network: 'mainnet' | 'testnet' | 'devnet' | 'localnet', privateKey?: string) {
		this.#client = new SuiClient({ url: getFullnodeUrl(network) });
		if (!privateKey) {
			this.#signer = getSigner();
		} else {
			this.#signer = getSignerFromPK(privateKey);
		}
	}

	async init(mergeCoins: boolean) {
		await this.#config.init(this.#client, this.#signer, mergeCoins);
	}

	getActiveAddress() {
		return this.#signer.getPublicKey().toSuiAddress();
	}

	addBalanceManager(managerKey: string, managerId: string, tradeCapId?: string) {
		this.#balanceManagers[managerKey] = {
			address: managerId,
			tradeCap: tradeCapId,
		};
	}

	getConfig() {
		return this.#config;
	}

	/// Balance Manager
	async createAndShareBalanceManager() {
		let txb = new Transaction();
		createAndShareBalanceManager(txb);
		let res = await signAndExecuteWithClientAndSigner(txb, this.#client, this.#signer);
		console.dir(res, { depth: null });
	}

	async depositIntoManager(managerKey: string, amountToDeposit: number, coinKey: CoinKey) {
		const balanceManager = this.getBalanceManager(managerKey);
		const coin = this.#config.getCoin(coinKey);

		let txb = new Transaction();
		depositIntoManager(balanceManager.address, amountToDeposit, coin, txb);
		let res = await signAndExecuteWithClientAndSigner(txb, this.#client, this.#signer);
		console.dir(res, { depth: null });
	}

	async withdrawFromManager(managerKey: string, amountToWithdraw: number, coinKey: CoinKey) {
		const balanceManager = this.getBalanceManager(managerKey);
		const coin = this.#config.getCoin(coinKey);

		let txb = new Transaction();
		const recipient = this.getActiveAddress();
		withdrawFromManager(balanceManager.address, amountToWithdraw, coin, recipient, txb);
		let res = await signAndExecuteWithClientAndSigner(txb, this.#client, this.#signer);
		console.dir(res, { depth: null });
	}

	async withdrawAllFromManager(managerKey: string, coinKey: CoinKey) {
		const balanceManager = this.getBalanceManager(managerKey);
		const coin = this.#config.getCoin(coinKey);

		let txb = new Transaction();
		const recipient = this.getActiveAddress();
		withdrawAllFromManager(balanceManager.address, coin, recipient, txb);
		let res = await signAndExecuteWithClientAndSigner(txb, this.#client, this.#signer);
		console.dir(res, { depth: null });
	}

	async checkManagerBalance(managerKey: string, coinKey: CoinKey) {
		const balanceManager = this.getBalanceManager(managerKey);
		const coin = this.#config.getCoin(coinKey);

		let txb = new Transaction();
		checkManagerBalance(balanceManager.address, coin, txb);
		let sender = normalizeSuiAddress(this.#signer.getPublicKey().toSuiAddress());
		const res = await this.#client.devInspectTransactionBlock({
			sender: sender,
			transactionBlock: txb,
		});

		const bytes = res.results![0].returnValues![0][0];
		const parsed_balance = bcs.U64.parse(new Uint8Array(bytes));
		const balanceNumber = Number(parsed_balance);
		const adjusted_balance = balanceNumber / coin.scalar;

		console.log(`Manager balance for ${coin.type} is ${adjusted_balance.toString()}`); // Output the u64 number as a string
	}

	/// DeepBook
	async placeLimitOrder(params: PlaceLimitOrderParams) {
		const {
			poolKey,
			managerKey,
			clientOrderId,
			price,
			quantity,
			isBid,
			expiration = MAX_TIMESTAMP,
			orderType = OrderType.NO_RESTRICTION,
			selfMatchingOption = SelfMatchingOptions.SELF_MATCHING_ALLOWED,
			payWithDeep = true,
		} = params;

		if (!payWithDeep) {
			throw new Error('payWithDeep = false not yet supported.');
		}
		let balanceManager = this.getBalanceManager(managerKey);

		let pool = this.#config.getPool(poolKey);
		let txb = new Transaction();

		placeLimitOrder(
			pool,
			balanceManager,
			clientOrderId,
			price,
			quantity,
			isBid,
			expiration,
			orderType,
			selfMatchingOption,
			payWithDeep,
			txb,
		);
		let res = await signAndExecuteWithClientAndSigner(txb, this.#client, this.#signer);
		console.dir(res, { depth: null });
	}

	async placeMarketOrder(params: PlaceMarketOrderParams) {
		const {
			poolKey,
			managerKey,
			clientOrderId,
			quantity,
			isBid,
			selfMatchingOption = SelfMatchingOptions.SELF_MATCHING_ALLOWED,
			payWithDeep = true,
		} = params;

		if (!payWithDeep) {
			throw new Error('payWithDeep = false not supported.');
		}
		let balanceManager = this.getBalanceManager(managerKey);

		let pool = this.#config.getPool(poolKey);
		let txb = new Transaction();

		placeMarketOrder(
			pool,
			balanceManager,
			clientOrderId,
			quantity,
			isBid,
			selfMatchingOption,
			payWithDeep,
			txb,
		);
		let res = await signAndExecuteWithClientAndSigner(txb, this.#client, this.#signer);
		console.dir(res, { depth: null });
	}

	async cancelOrder(poolKey: PoolKey, managerKey: string, clientOrderId: number) {
		let balanceManager = this.getBalanceManager(managerKey);
		let pool = this.#config.getPool(poolKey);
		let txb = new Transaction();

		cancelOrder(pool, balanceManager, clientOrderId, txb);
		let res = await signAndExecuteWithClientAndSigner(txb, this.#client, this.#signer);
		console.dir(res, { depth: null });
	}

	async cancelAllOrders(poolKey: PoolKey, managerKey: string) {
		let balanceManager = this.getBalanceManager(managerKey);
		let pool = this.#config.getPool(poolKey);
		let txb = new Transaction();

		cancelAllOrders(pool, balanceManager, txb);
		let res = await signAndExecuteWithClientAndSigner(txb, this.#client, this.#signer);
		console.dir(res, { depth: null });
	}

	async swapExactBaseForQuote(params: SwapParams) {
		const { poolKey, coinKey: baseKey, amount: baseAmount, deepAmount } = params;

		let pool = this.#config.getPool(poolKey);
		let baseCoinId = this.#config.getCoin(baseKey).coinId;
		let deepCoinId = this.#config.getCoin(CoinKey.DEEP).coinId;

		let txb = new Transaction();
		const recipient = this.getActiveAddress();
		swapExactBaseForQuote(pool, baseAmount, baseCoinId, deepAmount, deepCoinId, recipient, txb);

		let res = await signAndExecuteWithClientAndSigner(txb, this.#client, this.#signer);
		console.dir(res, { depth: null });
	}

	async swapExactQuoteForBase(params: SwapParams) {
		const { poolKey, coinKey: quoteKey, amount: quoteAmount, deepAmount } = params;

		let pool = this.#config.getPool(poolKey);
		let quoteCoinId = this.#config.getCoin(quoteKey).coinId;
		let deepCoinId = this.#config.getCoin(CoinKey.DEEP).coinId;

		let txb = new Transaction();
		const recipient = this.getActiveAddress();
		swapExactQuoteForBase(pool, quoteAmount, quoteCoinId, deepAmount, deepCoinId, recipient, txb);

		let res = await signAndExecuteWithClientAndSigner(txb, this.#client, this.#signer);
		console.dir(res, { depth: null });
	}

	async addDeepPricePoint(targetPoolKey: PoolKey, referencePoolKey: PoolKey) {
		let targetPool = this.#config.getPool(targetPoolKey);
		let referencePool = this.#config.getPool(referencePoolKey);
		let txb = new Transaction();
		addDeepPricePoint(targetPool, referencePool, txb);
		let res = await signAndExecuteWithClientAndSigner(txb, this.#client, this.#signer);
		console.dir(res, { depth: null });
	}

	async claimRebates(poolKey: PoolKey, managerKey: string) {
		const balanceManager = this.getBalanceManager(managerKey);

		let pool = this.#config.getPool(poolKey);
		let txb = new Transaction();
		claimRebates(pool, balanceManager, txb);
		let res = await signAndExecuteWithClientAndSigner(txb, this.#client, this.#signer);
		console.dir(res, { depth: null });
	}

	async burnDeep(poolKey: PoolKey) {
		let pool = this.#config.getPool(poolKey);
		let txb = new Transaction();
		burnDeep(pool, txb);
		let res = await signAndExecuteWithClientAndSigner(txb, this.#client, this.#signer);
		console.dir(res, { depth: null });
	}

	async midPrice(poolKey: PoolKey): Promise<number> {
		let pool = this.#config.getPool(poolKey);
		let txb = new Transaction();
		return await midPrice(pool, txb);
	}

	async whitelisted(poolKey: PoolKey): Promise<boolean> {
		let pool = this.#config.getPool(poolKey);
		let txb = new Transaction();
		return await whitelisted(pool, txb);
	}

	async getQuoteQuantityOut(poolKey: PoolKey, baseQuantity: number) {
		let pool = this.#config.getPool(poolKey);
		let txb = new Transaction();

		await getQuoteQuantityOut(pool, baseQuantity, txb);
	}

	async getBaseQuantityOut(poolKey: PoolKey, quoteQuantity: number) {
		let pool = this.#config.getPool(poolKey);
		let txb = new Transaction();

		await getBaseQuantityOut(pool, quoteQuantity, txb);
	}

	async accountOpenOrders(poolKey: PoolKey, managerKey: string) {
		let pool = this.#config.getPool(poolKey);
		let txb = new Transaction();
		await accountOpenOrders(pool, managerKey, txb);
	}

	async getLevel2Range(
		poolKey: PoolKey,
		priceLow: number,
		priceHigh: number,
		isBid: boolean,
	): Promise<string[][]> {
		let pool = this.#config.getPool(poolKey);
		let txb = new Transaction();

		return getLevel2Range(pool, priceLow, priceHigh, isBid, txb);
	}

	async getLevel2TicksFromMid(poolKey: PoolKey, ticks: number): Promise<string[][]> {
		let pool = this.#config.getPool(poolKey);
		let txb = new Transaction();

		return getLevel2TicksFromMid(pool, ticks, txb);
	}

	async vaultBalances(poolKey: PoolKey): Promise<number[]> {
		let pool = this.#config.getPool(poolKey);
		let txb = new Transaction();

		return vaultBalances(pool, txb);
	}

	async getPoolIdByAssets(baseType: string, quoteType: string): Promise<string> {
		let txb = new Transaction();

		return getPoolIdByAssets(baseType, quoteType, txb);
	}

	/// DeepBook Admin
	async createPoolAdmin(params: CreatePoolAdminParams) {
		const { baseCoinKey, quoteCoinKey, tickSize, lotSize, minSize, whitelisted, stablePool } =
			params;

		let txb = new Transaction();
		let baseCoin = this.#config.getCoin(baseCoinKey);
		let quoteCoin = this.#config.getCoin(quoteCoinKey);
		let deepCoinId = this.#config.getCoin(CoinKey.DEEP).coinId;
		createPoolAdmin(
			baseCoin,
			quoteCoin,
			deepCoinId,
			tickSize,
			lotSize,
			minSize,
			whitelisted,
			stablePool,
			txb,
		);
		let res = await signAndExecuteWithClientAndSigner(txb, this.#client, this.#signer);
		console.dir(res, { depth: null });
	}

	async unregisterPoolAdmin(poolKey: PoolKey) {
		let pool = this.#config.getPool(poolKey);
		let txb = new Transaction();
		unregisterPoolAdmin(pool, txb);
		let res = await signAndExecuteWithClientAndSigner(txb, this.#client, this.#signer);
		console.dir(res, { depth: null });
	}

	async updateDisabledVersions(poolKey: PoolKey) {
		let pool = this.#config.getPool(poolKey);
		let txb = new Transaction();
		updateDisabledVersions(pool, txb);
		let res = await signAndExecuteWithClientAndSigner(txb, this.#client, this.#signer);
		console.dir(res, { depth: null });
	}

	async stake(poolKey: PoolKey, managerKey: string, amount: number) {
		let pool = this.#config.getPool(poolKey);
		let txb = new Transaction();
		const balanceManager = this.getBalanceManager(managerKey);
		stake(pool, balanceManager, amount, txb);
		let res = await signAndExecuteWithClientAndSigner(txb, this.#client, this.#signer);
		console.dir(res, { depth: null });
	}

	async unstake(poolKey: PoolKey, managerKey: string) {
		let pool = this.#config.getPool(poolKey);
		let txb = new Transaction();
		const balanceManager = this.getBalanceManager(managerKey);
		unstake(pool, balanceManager, txb);
		let res = await signAndExecuteWithClientAndSigner(txb, this.#client, this.#signer);
		console.dir(res, { depth: null });
	}

	async submitProposal(params: ProposalParams) {
		const { poolKey, managerKey, takerFee, makerFee, stakeRequired } = params;

		let pool = this.#config.getPool(poolKey);
		let txb = new Transaction();
		const balanceManager = this.getBalanceManager(managerKey);
		submitProposal(pool, balanceManager, takerFee, makerFee, stakeRequired, txb);
		let res = await signAndExecuteWithClientAndSigner(txb, this.#client, this.#signer);
		console.dir(res, { depth: null });
	}

	async vote(poolKey: PoolKey, managerKey: string, proposal_id: string) {
		let pool = this.#config.getPool(poolKey);
		let txb = new Transaction();
		const balanceManager = this.getBalanceManager(managerKey);
		vote(pool, balanceManager, proposal_id, txb);
		let res = await signAndExecuteWithClientAndSigner(txb, this.#client, this.#signer);
		console.dir(res, { depth: null });
	}

	async signAndExecute(txb: Transaction) {
		let res = await signAndExecuteWithClientAndSigner(txb, this.#client, this.#signer);
		console.dir(res, { depth: null });
	}

	getBalanceManager(managerKey: string): BalanceManager {
		if (!Object.hasOwn(this.#balanceManagers, managerKey)) {
			throw new Error(`Balance manager with key ${managerKey} not found.`);
		}

		return this.#balanceManagers[managerKey];
	}
}
