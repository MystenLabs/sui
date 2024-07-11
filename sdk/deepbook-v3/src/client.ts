// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { bcs } from '@mysten/sui/bcs';
import type { SuiClient } from '@mysten/sui/client';
import type { Signer } from '@mysten/sui/cryptography';
import type { TransactionResult } from '@mysten/sui/transactions';
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

	#signAndExecuteCommand = (command: (tx: Transaction) => void) => {
		const transaction = new Transaction();
		transaction.add(command);

		return this.#client.signAndExecuteTransaction({
			transaction,
			signer: this.#signer,
			options: {
				showEffects: true,
				showBalanceChanges: true,
			},
		});
	};

	/// Balance Manager
	createAndShareBalanceManager() {
		return this.#signAndExecuteCommand(this.#config.balanceManager.createAndShareBalanceManager());
	}

	depositIntoManager(managerKey: string, amountToDeposit: number, coinKey: string) {
		const balanceManager = this.#getBalanceManager(managerKey);
		const coin = this.#config.getCoin(coinKey);

		return this.#signAndExecuteCommand(
			this.#config.balanceManager.depositIntoManager(balanceManager.address, amountToDeposit, coin),
		);
	}

	withdrawFromManager(managerKey: string, amountToWithdraw: number, coinKey: string) {
		const balanceManager = this.#getBalanceManager(managerKey);
		const coin = this.#config.getCoin(coinKey);

		const recipient = this.getActiveAddress();

		return this.#signAndExecuteCommand(
			this.#config.balanceManager.withdrawFromManager(
				balanceManager.address,
				amountToWithdraw,
				coin,
				recipient,
			),
		);
	}

	withdrawAllFromManager(managerKey: string, coinKey: string) {
		const balanceManager = this.#getBalanceManager(managerKey);
		const coin = this.#config.getCoin(coinKey);

		const recipient = this.getActiveAddress();
		return this.#signAndExecuteCommand(
			this.#balanceManager.withdrawAllFromManager(balanceManager.address, coin, recipient),
		);
	}

	async checkManagerBalance(managerKey: string, coinKey: string) {
		const balanceManager = this.#getBalanceManager(managerKey);
		const coin = this.#config.getCoin(coinKey);

		const tx = new Transaction();
		tx.add(this.#balanceManager.checkManagerBalance(balanceManager.address, coin));
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

		return this.#signAndExecuteCommand(
			this.#deepBook.placeLimitOrder(
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
			),
		);
	}

	placeMarketOrder(params: PlaceMarketOrderParams) {
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

		return this.#signAndExecuteCommand(
			this.#deepBook.placeMarketOrder(
				pool,
				balanceManager,
				clientOrderId,
				quantity,
				isBid,
				selfMatchingOption,
				payWithDeep,
			),
		);
	}

	cancelOrder(poolKey: string, managerKey: string, clientOrderId: number) {
		const balanceManager = this.#getBalanceManager(managerKey);
		const pool = this.#config.getPool(poolKey);

		return this.#signAndExecuteCommand(
			this.#deepBook.cancelOrder(pool, balanceManager, clientOrderId),
		);
	}

	cancelAllOrders(poolKey: string, managerKey: string) {
		const balanceManager = this.#getBalanceManager(managerKey);
		const pool = this.#config.getPool(poolKey);

		return this.#signAndExecuteCommand(this.#deepBook.cancelAllOrders(pool, balanceManager));
	}

	// swapExactBaseForQuote(params: SwapParams) {
	// 	const { poolKey, amount: baseAmount, deepAmount } = params;

	// 	const pool = this.#config.getPool(poolKey);
	// 	const baseCoinId = this.#config.getCoinId(baseKey);
	// 	const deepCoinId = this.#config.getCoinId('DEEP');

	// 	const recipient = this.getActiveAddress();

	// 	return this.#signAndExecuteCommand(
	// 		this.#deepBook.swapExactBaseForQuote(
	// 			pool,
	// 			baseAmount,
	// 			baseCoinId,
	// 			deepAmount,
	// 			deepCoinId,
	// 			recipient,
	// 		),
	// 	);
	// }

	// swapExactQuoteForBase(params: SwapParams) {
	// 	const { poolKey, coinKey: quoteKey, amount: quoteAmount, deepAmount } = params;

	// 	const pool = this.#config.getPool(poolKey);
	// 	const quoteCoinId = this.#config.getCoinId(quoteKey);
	// 	const deepCoinId = this.#config.getCoinId('DEEP');

	// 	const recipient = this.getActiveAddress();

	// 	return this.#signAndExecuteCommand(
	// 		this.#deepBook.swapExactQuoteForBase(
	// 			pool,
	// 			quoteAmount,
	// 			quoteCoinId,
	// 			deepAmount,
	// 			deepCoinId,
	// 			recipient,
	// 		),
	// 	);
	// }

	addDeepPricePoint(targetPoolKey: string, referencePoolKey: string) {
		const targetPool = this.#config.getPool(targetPoolKey);
		const referencePool = this.#config.getPool(referencePoolKey);

		return this.#signAndExecuteCommand(this.#deepBook.addDeepPricePoint(targetPool, referencePool));
	}

	claimRebates(poolKey: string, managerKey: string) {
		const balanceManager = this.#getBalanceManager(managerKey);

		const pool = this.#config.getPool(poolKey);

		return this.#signAndExecuteCommand(this.#deepBook.claimRebates(pool, balanceManager));
	}

	burnDeep(poolKey: string) {
		const pool = this.#config.getPool(poolKey);

		return this.#signAndExecuteCommand(this.#deepBook.burnDeep(pool));
	}

	midPrice(poolKey: string): Promise<number> {
		const pool = this.#config.getPool(poolKey);

		return this.#deepBook.midPrice(pool);
	}

	async whitelisted(poolKey: string): Promise<boolean> {
		const pool = this.#config.getPool(poolKey);

		const tx = new Transaction();
		tx.add(this.#deepBook.whitelisted(pool));
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
		const tx = new Transaction();

		tx.add(this.#deepBook.getQuoteQuantityOut(pool, baseQuantity));

		const res = await this.#config.client.devInspectTransactionBlock({
			sender: normalizeSuiAddress('0xa'),
			transactionBlock: tx,
		});

		const baseOut = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![0][0])));
		const quoteOut = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![1][0])));
		const deepRequired = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![2][0])));

		return {
			baseQuantity,
			base: baseOut / pool.baseCoin.scalar,
			quote: quoteOut / pool.quoteCoin.scalar,
			deep: deepRequired / DEEP_SCALAR,
		};
	}

	async getBaseQuantityOut(poolKey: string, baseQuantity: number) {
		const pool = this.#config.getPool(poolKey);
		const tx = new Transaction();

		tx.add(this.#deepBook.getBaseQuantityOut(pool, baseQuantity));

		const res = await this.#config.client.devInspectTransactionBlock({
			sender: normalizeSuiAddress('0xa'),
			transactionBlock: tx,
		});

		const baseOut = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![0][0])));
		const quoteOut = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![1][0])));
		const deepRequired = Number(bcs.U64.parse(new Uint8Array(res.results![0].returnValues![2][0])));

		return {
			baseQuantity,
			base: baseOut / pool.baseCoin.scalar,
			quote: quoteOut / pool.quoteCoin.scalar,
			deep: deepRequired / DEEP_SCALAR,
		};
	}

	async getQuantityOut(poolKey: string, baseQuantity: number, quoteQuantity: number) {
		const pool = this.#config.getPool(poolKey);
		const tx = new Transaction();

		tx.add(this.#deepBook.getQuantityOut(pool, baseQuantity, quoteQuantity));

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
			base: baseOut / pool.baseCoin.scalar,
			quote: quoteOut / pool.quoteCoin.scalar,
			deep: deepRequired / DEEP_SCALAR,
		};
	}

	async accountOpenOrders(poolKey: string, managerKey: string) {
		const pool = this.#config.getPool(poolKey);
		const tx = new Transaction();

		tx.add(this.#deepBook.accountOpenOrders(pool, managerKey));

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
		const tx = new Transaction();

		tx.add(this.#deepBook.getLevel2Range(pool, priceLow, priceHigh, isBid));

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
		const tx = new Transaction();

		tx.add(this.#deepBook.getLevel2TicksFromMid(pool, ticks));

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
		const tx = new Transaction();

		tx.add(this.#deepBook.vaultBalances(pool));

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
		const tx = new Transaction();

		tx.add(this.#deepBook.getPoolIdByAssets(baseType, quoteType));

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

	/// DeepBook Admin
	createPoolAdmin(params: CreatePoolAdminParams) {
		const { baseCoinKey, quoteCoinKey, tickSize, lotSize, minSize, whitelisted, stablePool } =
			params;

		const baseCoin = this.#config.getCoin(baseCoinKey);
		const quoteCoin = this.#config.getCoin(quoteCoinKey);
		const deepCoinId = this.#config.getCoinId('DEEP');

		return this.#signAndExecuteCommand(
			this.#deepBookAdmin.createPoolAdmin(
				baseCoin,
				quoteCoin,
				deepCoinId,
				tickSize,
				lotSize,
				minSize,
				whitelisted,
				stablePool,
			),
		);
	}

	unregisterPoolAdmin(poolKey: string) {
		const pool = this.#config.getPool(poolKey);

		return this.#signAndExecuteCommand(this.#deepBookAdmin.unregisterPoolAdmin(pool));
	}

	updateDisabledVersions(poolKey: string) {
		const pool = this.#config.getPool(poolKey);

		return this.#signAndExecuteCommand(this.#deepBookAdmin.updateDisabledVersions(pool));
	}

	stake(poolKey: string, managerKey: string, amount: number) {
		const pool = this.#config.getPool(poolKey);

		const balanceManager = this.#getBalanceManager(managerKey);
		return this.#signAndExecuteCommand(this.#governance.stake(pool, balanceManager, amount));
	}

	unstake(poolKey: string, managerKey: string) {
		const pool = this.#config.getPool(poolKey);

		const balanceManager = this.#getBalanceManager(managerKey);
		return this.#signAndExecuteCommand(this.#governance.unstake(pool, balanceManager));
	}

	submitProposal(params: ProposalParams) {
		const { poolKey, managerKey, takerFee, makerFee, stakeRequired } = params;

		const pool = this.#config.getPool(poolKey);

		const balanceManager = this.#getBalanceManager(managerKey);
		return this.#signAndExecuteCommand(
			this.#governance.submitProposal(pool, balanceManager, takerFee, makerFee, stakeRequired),
		);
	}

	vote(poolKey: string, managerKey: string, proposal_id: string) {
		const pool = this.#config.getPool(poolKey);

		const balanceManager = this.#getBalanceManager(managerKey);
		return this.#signAndExecuteCommand(this.#governance.vote(pool, balanceManager, proposal_id));
	}

	// // Flash Loans
	// borrowBaseAsset = (
	// 	pool: PoolKey,
	// 	borrowAmount: number,
	// ) => {
	// 	return this.#signAndExecuteCommand(
	// 		this.#flashLoans.borrowAndReturnBaseAsset(this.#config.getPool(pool), borrowAmount),
	// 	);
	// };

	// borrowAndReturnQuoteAsset = (
	// 	pool: PoolKey,
	// 	borrowAmount: number,
	// 	add: <T>(tx: Transaction, flashLoan: TransactionResult[1]) => T,
	// ) => {
	// 	return this.#signAndExecuteCommand(
	// 		this.#flashLoans.borrowAndReturnQuoteAsset(this.#config.getPool(pool), borrowAmount, add),
	// 	);
	// };

	#getBalanceManager(managerKey: string): BalanceManager {
		if (!Object.hasOwn(this.#balanceManagers, managerKey)) {
			throw new Error(`Balance manager with key ${managerKey} not found.`);
		}

		return this.#balanceManagers[managerKey];
	}
}
