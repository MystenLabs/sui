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
	CoinKey,
	CreatePoolAdminParams,
	Environment,
	PlaceLimitOrderParams,
	PlaceMarketOrderParams,
	PoolKey,
	ProposalParams,
	SwapParams,
} from './types/index.js';
import { OrderType, SelfMatchingOptions } from './types/index.js';
import { DeepBookConfig, MAX_TIMESTAMP } from './utils/config.js';
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

	depositIntoManager(managerKey: string, amountToDeposit: number, coinKey: CoinKey) {
		const balanceManager = this.#getBalanceManager(managerKey);
		const coin = this.#config.getCoin(coinKey);

		return this.#signAndExecuteCommand(
			this.#config.balanceManager.depositIntoManager(balanceManager.address, amountToDeposit, coin),
		);
	}

	withdrawFromManager(managerKey: string, amountToWithdraw: number, coinKey: CoinKey) {
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

	withdrawAllFromManager(managerKey: string, coinKey: CoinKey) {
		const balanceManager = this.#getBalanceManager(managerKey);
		const coin = this.#config.getCoin(coinKey);

		const recipient = this.getActiveAddress();
		return this.#signAndExecuteCommand(
			this.#balanceManager.withdrawAllFromManager(balanceManager.address, coin, recipient),
		);
	}

	async checkManagerBalance(managerKey: string, coinKey: CoinKey) {
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

	cancelOrder(poolKey: PoolKey, managerKey: string, clientOrderId: number) {
		const balanceManager = this.#getBalanceManager(managerKey);
		const pool = this.#config.getPool(poolKey);

		return this.#signAndExecuteCommand(
			this.#deepBook.cancelOrder(pool, balanceManager, clientOrderId),
		);
	}

	cancelAllOrders(poolKey: PoolKey, managerKey: string) {
		const balanceManager = this.#getBalanceManager(managerKey);
		const pool = this.#config.getPool(poolKey);

		return this.#signAndExecuteCommand(this.#deepBook.cancelAllOrders(pool, balanceManager));
	}

	swapExactBaseForQuote(params: SwapParams) {
		const { poolKey, coinKey: baseKey, amount: baseAmount, deepAmount } = params;

		const pool = this.#config.getPool(poolKey);
		const baseCoinId = this.#config.getCoinId(baseKey);
		const deepCoinId = this.#config.getCoinId('DEEP');

		const recipient = this.getActiveAddress();

		return this.#signAndExecuteCommand(
			this.#deepBook.swapExactBaseForQuote(
				pool,
				baseAmount,
				baseCoinId,
				deepAmount,
				deepCoinId,
				recipient,
			),
		);
	}

	swapExactQuoteForBase(params: SwapParams) {
		const { poolKey, coinKey: quoteKey, amount: quoteAmount, deepAmount } = params;

		const pool = this.#config.getPool(poolKey);
		const quoteCoinId = this.#config.getCoinId(quoteKey);
		const deepCoinId = this.#config.getCoinId('DEEP');

		const recipient = this.getActiveAddress();

		return this.#signAndExecuteCommand(
			this.#deepBook.swapExactQuoteForBase(
				pool,
				quoteAmount,
				quoteCoinId,
				deepAmount,
				deepCoinId,
				recipient,
			),
		);
	}

	addDeepPricePoint(targetPoolKey: PoolKey, referencePoolKey: PoolKey) {
		const targetPool = this.#config.getPool(targetPoolKey);
		const referencePool = this.#config.getPool(referencePoolKey);

		return this.#signAndExecuteCommand(this.#deepBook.addDeepPricePoint(targetPool, referencePool));
	}

	claimRebates(poolKey: PoolKey, managerKey: string) {
		const balanceManager = this.#getBalanceManager(managerKey);

		const pool = this.#config.getPool(poolKey);

		return this.#signAndExecuteCommand(this.#deepBook.claimRebates(pool, balanceManager));
	}

	burnDeep(poolKey: PoolKey) {
		const pool = this.#config.getPool(poolKey);

		return this.#signAndExecuteCommand(this.#deepBook.burnDeep(pool));
	}

	midPrice(poolKey: PoolKey): Promise<number> {
		const pool = this.#config.getPool(poolKey);

		return this.#deepBook.midPrice(pool);
	}

	whitelisted(poolKey: PoolKey): Promise<boolean> {
		const pool = this.#config.getPool(poolKey);

		return this.#deepBook.whitelisted(pool);
	}

	getQuoteQuantityOut(poolKey: PoolKey, baseQuantity: number) {
		const pool = this.#config.getPool(poolKey);

		return this.#deepBook.getQuoteQuantityOut(pool, baseQuantity);
	}

	getBaseQuantityOut(poolKey: PoolKey, quoteQuantity: number) {
		const pool = this.#config.getPool(poolKey);

		return this.#deepBook.getBaseQuantityOut(pool, quoteQuantity);
	}

	accountOpenOrders(poolKey: PoolKey, managerKey: string) {
		const pool = this.#config.getPool(poolKey);

		return this.#deepBook.accountOpenOrders(pool, managerKey);
	}

	getLevel2Range(
		poolKey: PoolKey,
		priceLow: number,
		priceHigh: number,
		isBid: boolean,
	): Promise<string[][]> {
		const pool = this.#config.getPool(poolKey);

		return this.#deepBook.getLevel2Range(pool, priceLow, priceHigh, isBid);
	}

	getLevel2TicksFromMid(poolKey: PoolKey, ticks: number): Promise<string[][]> {
		const pool = this.#config.getPool(poolKey);

		return this.#deepBook.getLevel2TicksFromMid(pool, ticks);
	}

	vaultBalances(poolKey: PoolKey): Promise<number[]> {
		const pool = this.#config.getPool(poolKey);

		return this.#deepBook.vaultBalances(pool);
	}

	getPoolIdByAssets(baseType: string, quoteType: string): Promise<string> {
		return this.#deepBook.getPoolIdByAssets(baseType, quoteType);
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

	unregisterPoolAdmin(poolKey: PoolKey) {
		const pool = this.#config.getPool(poolKey);

		return this.#signAndExecuteCommand(this.#deepBookAdmin.unregisterPoolAdmin(pool));
	}

	updateDisabledVersions(poolKey: PoolKey) {
		const pool = this.#config.getPool(poolKey);

		return this.#signAndExecuteCommand(this.#deepBookAdmin.updateDisabledVersions(pool));
	}

	stake(poolKey: PoolKey, managerKey: string, amount: number) {
		const pool = this.#config.getPool(poolKey);

		const balanceManager = this.#getBalanceManager(managerKey);
		return this.#signAndExecuteCommand(this.#governance.stake(pool, balanceManager, amount));
	}

	unstake(poolKey: PoolKey, managerKey: string) {
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

	vote(poolKey: PoolKey, managerKey: string, proposal_id: string) {
		const pool = this.#config.getPool(poolKey);

		const balanceManager = this.#getBalanceManager(managerKey);
		return this.#signAndExecuteCommand(this.#governance.vote(pool, balanceManager, proposal_id));
	}

	// Flash Loans
	borrowAndReturnBaseAsset = (
		pool: PoolKey,
		borrowAmount: number,
		add: <T>(tx: Transaction, flashLoan: TransactionResult[1]) => T,
	) => {
		return this.#signAndExecuteCommand(
			this.#flashLoans.borrowAndReturnBaseAsset(this.#config.getPool(pool), borrowAmount, add),
		);
	};

	borrowAndReturnQuoteAsset = (
		pool: PoolKey,
		borrowAmount: number,
		add: <T>(tx: Transaction, flashLoan: TransactionResult[1]) => T,
	) => {
		return this.#signAndExecuteCommand(
			this.#flashLoans.borrowAndReturnQuoteAsset(this.#config.getPool(pool), borrowAmount, add),
		);
	};

	#getBalanceManager(managerKey: string): BalanceManager {
		if (!Object.hasOwn(this.#balanceManagers, managerKey)) {
			throw new Error(`Balance manager with key ${managerKey} not found.`);
		}

		return this.#balanceManagers[managerKey];
	}
}
