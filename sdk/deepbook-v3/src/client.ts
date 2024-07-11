// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { bcs } from '@mysten/sui/bcs';
import { SuiClient } from '@mysten/sui/client';
import type { Signer } from '@mysten/sui/cryptography';
import { Transaction } from '@mysten/sui/transactions';
import { normalizeSuiAddress } from '@mysten/sui/utils';

import { BalanceManagerContract } from './transactions/balanceManager.js';
import { DeepBookContract } from './transactions/deepbook.js';
import { DeepBookAdminContract } from './transactions/deepbookAdmin.js';
import { FlashLoanContract } from './transactions/flashLoans.js';
import { GovernanceContract } from './transactions/governance.js';
import type {
	BalanceManager,
	CreatePoolAdminParams,
	Environment,
	PlaceLimitOrderParams,
	PlaceMarketOrderParams,
	ProposalParams,
	SwapParams,
} from './types/index.js';
import { OrderType, SelfMatchingOptions } from './types/index.js';
import { DEEP_SCALAR, DeepBookConfig, MAX_TIMESTAMP } from './utils/config.js';
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
	#deepBookAdmin: DeepBookAdminContract;
	#flashLoans: FlashLoanContract;
	#governance: GovernanceContract;

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
		this.#deepBookAdmin = new DeepBookAdminContract(this.#config);
		this.#flashLoans = new FlashLoanContract(this.#config);
		this.#governance = new GovernanceContract(this.#config);
	}

	async init(mergeCoins: boolean) {
		await this.#config.init(mergeCoins);
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

	signAndExecuteCommand = (tx: Transaction) => {
		return this.#client.signAndExecuteTransaction({
			transaction: tx,
			signer: this.#signer,
			options: {
				showEffects: true,
				showBalanceChanges: true,
			},
		});
	};

	/// Balance Manager
	createAndShareBalanceManager(tx: Transaction = new Transaction()) {
		return this.#config.balanceManager.createAndShareBalanceManager(tx);
	}

	depositIntoManager(managerKey: string, amountToDeposit: number, coinKey: string, tx: Transaction = new Transaction()) {
		const balanceManager = this.#getBalanceManager(managerKey);
		const coin = this.#config.getCoin(coinKey);

		return this.#config.balanceManager.depositIntoManager(balanceManager.address, amountToDeposit, coin, tx);
	}

	withdrawFromManager(managerKey: string, amountToWithdraw: number, coinKey: string, tx: Transaction = new Transaction()) {
		const balanceManager = this.#getBalanceManager(managerKey);
		const coin = this.#config.getCoin(coinKey);

		const recipient = this.getActiveAddress();

		return this.#config.balanceManager.withdrawFromManager(
			balanceManager.address,
			amountToWithdraw,
			coin,
			recipient,
			tx,
		)
	}

	withdrawAllFromManager(managerKey: string, coinKey: string, tx: Transaction = new Transaction()) {
		const balanceManager = this.#getBalanceManager(managerKey);
		const coin = this.#config.getCoin(coinKey);

		const recipient = this.getActiveAddress();
		return this.#balanceManager.withdrawAllFromManager(balanceManager.address, coin, recipient, tx)
	}

	async checkManagerBalance(managerKey: string, coinKey: string, tx: Transaction = new Transaction()) {
		const balanceManager = this.#getBalanceManager(managerKey);
		const coin = this.#config.getCoin(coinKey);

		this.#balanceManager.checkManagerBalance(balanceManager.address, coin, tx);
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
			transaction: tx,
			coinType: coin.type,
			balance: adjusted_balance,
		};
	}

	/// DeepBook
	placeLimitOrder(params: PlaceLimitOrderParams) {
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
		const balanceManager = this.#getBalanceManager(managerKey);

		const pool = this.#config.getPool(poolKey);

		return this.#deepBook.placeLimitOrder(
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
		)
	}

	placeMarketOrder(params: PlaceMarketOrderParams, tx: Transaction = new Transaction()) {
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
		const balanceManager = this.#getBalanceManager(managerKey);

		const pool = this.#config.getPool(poolKey);

		return this.#deepBook.placeMarketOrder(
			pool,
			balanceManager,
			clientOrderId,
			quantity,
			isBid,
			selfMatchingOption,
			payWithDeep,
			tx,
		)
	}

	cancelOrder(poolKey: string, managerKey: string, clientOrderId: number, tx: Transaction = new Transaction()) {
		const balanceManager = this.#getBalanceManager(managerKey);
		const pool = this.#config.getPool(poolKey);

		return this.#deepBook.cancelOrder(pool, balanceManager, clientOrderId, tx)
	}

	cancelAllOrders(poolKey: string, managerKey: string, tx: Transaction = new Transaction()) {
		const balanceManager = this.#getBalanceManager(managerKey);
		const pool = this.#config.getPool(poolKey);

		return this.#deepBook.cancelAllOrders(pool, balanceManager, tx);
	}

	swapExactBaseForQuote(
		params: SwapParams,
		tx: Transaction = new Transaction(),
	) {
		const [baseOut, quoteOut, deepOut] = this.#deepBook.swapExactBaseForQuote(params, tx);
		console.log(baseOut, quoteOut, deepOut);
		tx.transferObjects([baseOut, quoteOut, deepOut], this.getActiveAddress());

		return tx;
	}

	swapExactQuoteForBase(
		params: SwapParams,
		tx: Transaction = new Transaction(),
	) {
		const [baseOut, quoteOut, deepOut] = this.#deepBook.swapExactQuoteForBase(params, tx);
		console.log(baseOut, quoteOut, deepOut);
		tx.transferObjects([baseOut, quoteOut, deepOut], this.getActiveAddress());

		return tx;
	}

	addDeepPricePoint(targetPoolKey: string, referencePoolKey: string, tx: Transaction = new Transaction()) {
		const targetPool = this.#config.getPool(targetPoolKey);
		const referencePool = this.#config.getPool(referencePoolKey);

		return this.#deepBook.addDeepPricePoint(targetPool, referencePool, tx);
	}

	claimRebates(poolKey: string, managerKey: string, tx: Transaction = new Transaction()) {
		const balanceManager = this.#getBalanceManager(managerKey);

		const pool = this.#config.getPool(poolKey);

		return this.#deepBook.claimRebates(pool, balanceManager, tx);
	}

	burnDeep(poolKey: string, tx: Transaction = new Transaction()) {
		const pool = this.#config.getPool(poolKey);

		return this.#deepBook.burnDeep(pool, tx);
	}

	midPrice(poolKey: string, tx: Transaction = new Transaction()) {
		const pool = this.#config.getPool(poolKey);

		return this.#deepBook.midPrice(pool, tx);
	}

	async whitelisted(poolKey: string, tx: Transaction = new Transaction()) {
		const pool = this.#config.getPool(poolKey);

		this.#deepBook.whitelisted(pool, tx);
		const res = await this.#config.client.devInspectTransactionBlock({
			sender: normalizeSuiAddress('0xa'),
			transactionBlock: tx,
		});

		const bytes = res.results![0].returnValues![0][0];
		const whitelisted = bcs.Bool.parse(new Uint8Array(bytes));

		return {
			transaction: tx,
			whitelisted
		};
	}

	async getQuoteQuantityOut(poolKey: string, baseQuantity: number, tx: Transaction = new Transaction()) {
		const pool = this.#config.getPool(poolKey);

		this.#deepBook.getQuoteQuantityOut(pool, baseQuantity, tx);

		const res = await this.#config.client.devInspectTransactionBlock({
			sender: normalizeSuiAddress('0xa'),
			transactionBlock: tx,
		});

		const baseOut = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![0][0])));
		const quoteOut = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![1][0])));
		const deepRequired = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![2][0])));

		return {
			transaction: tx,
			baseQuantity,
			baseOut: baseOut / pool.baseCoin.scalar,
			quoteOut: quoteOut / pool.quoteCoin.scalar,
			deepRequired: deepRequired / DEEP_SCALAR,
		};
	}

	async getBaseQuantityOut(poolKey: string, baseQuantity: number, tx: Transaction = new Transaction()) {
		const pool = this.#config.getPool(poolKey);

		this.#deepBook.getBaseQuantityOut(pool, baseQuantity, tx);

		const res = await this.#config.client.devInspectTransactionBlock({
			sender: normalizeSuiAddress('0xa'),
			transactionBlock: tx,
		});

		const baseOut = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![0][0])));
		const quoteOut = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![1][0])));
		const deepRequired = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![2][0])));

		return {
			transaction: tx,
			baseQuantity,
			baseOut: baseOut / pool.baseCoin.scalar,
			quoteOut: quoteOut / pool.quoteCoin.scalar,
			deepRequired: deepRequired / DEEP_SCALAR,
		};
	}

	async getQuantityOut(poolKey: string, baseQuantity: number, quoteQuantity: number, tx: Transaction = new Transaction()) {
		const pool = this.#config.getPool(poolKey);

		this.#deepBook.getQuantityOut(pool, baseQuantity, quoteQuantity, tx);

		const res = await this.#config.client.devInspectTransactionBlock({
			sender: normalizeSuiAddress('0xa'),
			transactionBlock: tx,
		});

		const baseOut = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![0][0])));
		const quoteOut = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![1][0])));
		const deepRequired = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![2][0])));

		return {
			transaction: tx,
			baseQuantity,
			quoteQuantity,
			baseOut: baseOut / pool.baseCoin.scalar,
			quoteOut: quoteOut / pool.quoteCoin.scalar,
			deepRequired: deepRequired / DEEP_SCALAR,
		};
	}

	async accountOpenOrders(poolKey: string, managerKey: string, tx: Transaction = new Transaction()) {
		const pool = this.#config.getPool(poolKey);

		this.#deepBook.accountOpenOrders(pool, managerKey, tx);

		const res = await this.#config.client.devInspectTransactionBlock({
			sender: normalizeSuiAddress('0xa'),
			transactionBlock: tx,
		});

		const order_ids = res.results![0].returnValues![0][0];
		const VecSet = bcs.struct('VecSet', {
			constants: bcs.vector(bcs.U128),
		});

		return {
			transaction: tx,
			openOrders: VecSet.parse(new Uint8Array(order_ids)).constants
		};
	}

	async getLevel2Range(poolKey: string, priceLow: number, priceHigh: number, isBid: boolean, tx: Transaction = new Transaction()) {
		const pool = this.#config.getPool(poolKey);

		this.#deepBook.getLevel2Range(pool, priceLow, priceHigh, isBid, tx);

		const res = await this.#config.client.devInspectTransactionBlock({
			sender: normalizeSuiAddress('0xa'),
			transactionBlock: tx,
		});

		const prices = res.results![0].returnValues![0][0];
		const parsed_prices = bcs.vector(bcs.u64()).parse(new Uint8Array(prices));
		const quantities = res.results![0].returnValues![1][0];
		const parsed_quantities = bcs.vector(bcs.u64()).parse(new Uint8Array(quantities));

		return {
			transaction: tx,
			prices: parsed_prices,
			quantities: parsed_quantities,
		};
	}

	async getLevel2TicksFromMid(poolKey: string, ticks: number, tx: Transaction = new Transaction()) {
		const pool = this.#config.getPool(poolKey);

		this.#deepBook.getLevel2TicksFromMid(pool, ticks, tx);

		const res = await this.#config.client.devInspectTransactionBlock({
			sender: normalizeSuiAddress('0xa'),
			transactionBlock: tx,
		});

		const prices = res.results![0].returnValues![0][0];
		const parsed_prices = bcs.vector(bcs.u64()).parse(new Uint8Array(prices));
		const quantities = res.results![0].returnValues![1][0];
		const parsed_quantities = bcs.vector(bcs.u64()).parse(new Uint8Array(quantities));

		return {
			transaction: tx,
			prices: parsed_prices,
			quantities: parsed_quantities,
		};
	}

	async vaultBalances(poolKey: string, tx: Transaction = new Transaction()) {
		const pool = this.#config.getPool(poolKey);

		this.#deepBook.vaultBalances(pool, tx);

		const res = await this.#config.client.devInspectTransactionBlock({
			sender: normalizeSuiAddress('0xa'),
			transactionBlock: tx,
		});

		const baseInVault = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![0][0])));
		const quoteInVault = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![1][0])));
		const deepInVault = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![2][0])));

		return {
			transaction: tx,
			base: baseInVault / pool.baseCoin.scalar,
			quote: quoteInVault / pool.quoteCoin.scalar,
			deep: deepInVault / DEEP_SCALAR,
		};
	}

	async getPoolIdByAssets(baseType: string, quoteType: string, tx: Transaction = new Transaction()) {
		this.#deepBook.getPoolIdByAssets(baseType, quoteType, tx);

		const res = await this.#config.client.devInspectTransactionBlock({
			sender: normalizeSuiAddress('0xa'),
			transactionBlock: tx,
		});

		const ID = bcs.struct('ID', {
			bytes: bcs.Address,
		});
		const address = ID.parse(new Uint8Array(res.results![0].returnValues![0][0]))['bytes'];

		return {
			transaction: tx,
			address
		};
	}

	/// DeepBook Admin
	createPoolAdmin(params: CreatePoolAdminParams, tx: Transaction = new Transaction()) {
		const { baseCoinKey, quoteCoinKey, tickSize, lotSize, minSize, whitelisted, stablePool } =
			params;

		const baseCoin = this.#config.getCoin(baseCoinKey);
		const quoteCoin = this.#config.getCoin(quoteCoinKey);
		const deepCoinId = this.#config.getCoinId('DEEP');

		return this.#deepBookAdmin.createPoolAdmin(
			baseCoin,
			quoteCoin,
			deepCoinId,
			tickSize,
			lotSize,
			minSize,
			whitelisted,
			stablePool,
			tx
		)
	}

	unregisterPoolAdmin(poolKey: string, tx: Transaction = new Transaction()) {
		const pool = this.#config.getPool(poolKey);

		return this.#deepBookAdmin.unregisterPoolAdmin(pool, tx);
	}

	updateDisabledVersions(poolKey: string, tx: Transaction = new Transaction()) {
		const pool = this.#config.getPool(poolKey);

		return this.#deepBookAdmin.updateDisabledVersions(pool, tx);
	}

	stake(poolKey: string, managerKey: string, amount: number, tx: Transaction = new Transaction()) {
		const pool = this.#config.getPool(poolKey);

		const balanceManager = this.#getBalanceManager(managerKey);
		return this.#governance.stake(pool, balanceManager, amount, tx);
	}

	unstake(poolKey: string, managerKey: string, tx: Transaction = new Transaction()) {
		const pool = this.#config.getPool(poolKey);

		const balanceManager = this.#getBalanceManager(managerKey);
		return this.#governance.unstake(pool, balanceManager, tx);
	}

	submitProposal(params: ProposalParams, tx: Transaction = new Transaction()) {
		const { poolKey, managerKey, takerFee, makerFee, stakeRequired } = params;

		const pool = this.#config.getPool(poolKey);

		const balanceManager = this.#getBalanceManager(managerKey);
		return this.#governance.submitProposal(pool, balanceManager, takerFee, makerFee, stakeRequired, tx);
	}

	vote(poolKey: string, managerKey: string, proposal_id: string, tx: Transaction = new Transaction()) {
		const pool = this.#config.getPool(poolKey);

		const balanceManager = this.#getBalanceManager(managerKey);
		return this.#governance.vote(pool, balanceManager, proposal_id, tx);
	}

	// Flash Loans
	borrowBaseAsset = (
		poolKey: string,
		borrowAmount: number,
		tx: Transaction = new Transaction(),
	) => {
		const pool = this.#config.getPool(poolKey);
		const [baseCoinResult, flashLoan] = this.#flashLoans.borrowBaseAsset(pool, borrowAmount, tx);

		return {
			transaction: tx,
			baseCoin: baseCoinResult,
			flashLoan
		};
	};

	returnBaseAsset = (
		poolKey: string,
		baseCoin: any,
		flashLoan: any,
		tx: Transaction = new Transaction(),
	) => {
		const pool = this.#config.getPool(poolKey);
		this.#flashLoans.returnBaseAsset(pool, baseCoin, flashLoan, tx);

		return tx;
	};

	borrowQuoteAsset = (
		poolKey: string,
		borrowAmount: number,
		tx: Transaction = new Transaction(),
	) => {
		const pool = this.#config.getPool(poolKey);
		const [quoteCoinResult, flashLoan] = this.#flashLoans.borrowQuoteAsset(pool, borrowAmount, tx);

		return {
			transaction: tx,
			quoteCoin: quoteCoinResult,
			flashLoan
		};
	};

	returnQuoteAsset = (
		poolKey: string,
		quoteCoin: any,
		flashLoan: any,
		tx: Transaction = new Transaction(),
	) => {
		const pool = this.#config.getPool(poolKey);
		this.#flashLoans.returnQuoteAsset(pool, quoteCoin, flashLoan, tx);

		return tx;
	};

	#getBalanceManager(managerKey: string): BalanceManager {
		if (!Object.hasOwn(this.#balanceManagers, managerKey)) {
			throw new Error(`Balance manager with key ${managerKey} not found.`);
		}

		return this.#balanceManagers[managerKey];
	}
}
