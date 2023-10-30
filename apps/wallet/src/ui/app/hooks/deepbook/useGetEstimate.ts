// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useActiveAccount } from '_app/hooks/useActiveAccount';
import { type WalletSigner } from '_app/WalletSigner';
import {
	DEEPBOOK_KEY,
	DEFAULT_WALLET_FEE_ADDRESS,
	ESTIMATED_GAS_FEES_PERCENTAGE,
	ONE_SUI_DEEPBOOK,
	WALLET_FEES_PERCENTAGE,
} from '_pages/swap/constants';
import { useDeepBookContext } from '_shared/deepBook/context';
import { FEATURES } from '_src/shared/experimentation/features';
import { useFeatureValue } from '@growthbook/growthbook-react';
import { useGetObject } from '@mysten/core';
import { useSuiClient } from '@mysten/dapp-kit';
import { type DeepBookClient } from '@mysten/deepbook';
import { TransactionBlock } from '@mysten/sui.js/builder';
import { type CoinStruct, type SuiClient } from '@mysten/sui.js/client';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import BigNumber from 'bignumber.js';

const MAX_COINS_PER_REQUEST = 10;

async function getCoinsByBalance({
	coinType,
	balance,
	suiClient,
	address,
}: {
	coinType: string;
	balance: string;
	suiClient: SuiClient;
	address: string;
}) {
	let cursor: string | undefined | null = null;
	let currentBalance = 0n;
	let hasNextPage = true;
	const coins = [];
	const bigIntBalance = BigInt(new BigNumber(balance).integerValue(BigNumber.ROUND_UP).toString());

	while (currentBalance < bigIntBalance && hasNextPage) {
		const { data, nextCursor } = await suiClient.getCoins({
			owner: address,
			coinType,
			cursor,
			limit: MAX_COINS_PER_REQUEST,
		});

		if (!data || !data.length) {
			break;
		}

		for (const coin of data) {
			currentBalance += BigInt(coin.balance);
			coins.push(coin);

			if (currentBalance >= bigIntBalance) {
				break;
			}
		}

		cursor = nextCursor;
		hasNextPage = !!nextCursor;
	}

	if (!coins.length) {
		throw new Error('No coins found in balance');
	}

	return coins;
}

function formatBalanceToLotSize(balance: string, lotSize: number) {
	const balanceBigNumber = new BigNumber(balance);
	const lotSizeBigNumber = new BigNumber(lotSize);
	const remainder = balanceBigNumber.mod(lotSizeBigNumber);

	if (remainder.isEqualTo(0)) {
		return balanceBigNumber.toString();
	}

	const roundedDownBalance = balanceBigNumber.minus(remainder);
	return roundedDownBalance.abs().toString();
}

async function getPlaceMarketOrderTxn({
	deepBookClient,
	poolId,
	accountCapId,
	address,
	isAsk,
	lotSize,
	baseBalance,
	quoteBalance,
	quoteCoins,
	walletFeeAddress,
}: {
	deepBookClient: DeepBookClient;
	poolId: string;
	accountCapId: string;
	address: string;
	isAsk: boolean;
	lotSize: number;
	baseBalance: string;
	quoteBalance: string;
	baseCoins: CoinStruct[];
	quoteCoins: CoinStruct[];
	walletFeeAddress: string;
}) {
	const txb = new TransactionBlock();
	const accountCap = accountCapId || deepBookClient.createAccountCap(txb);

	let balanceToSwap;
	let walletFeeCoin;
	let txnResult;

	if (isAsk) {
		const bigNumberBaseBalance = new BigNumber(baseBalance);

		if (bigNumberBaseBalance.isLessThan(ONE_SUI_DEEPBOOK)) {
			balanceToSwap = bigNumberBaseBalance.minus(
				bigNumberBaseBalance.times(ESTIMATED_GAS_FEES_PERCENTAGE / 100),
			);
		} else {
			balanceToSwap = bigNumberBaseBalance;
		}

		const walletFee = balanceToSwap
			.times(WALLET_FEES_PERCENTAGE / 100)
			.integerValue(BigNumber.ROUND_DOWN)
			.toString();

		balanceToSwap = formatBalanceToLotSize(balanceToSwap.minus(walletFee).toString(), lotSize);
		const swapCoin = txb.splitCoins(txb.gas, [balanceToSwap]);
		walletFeeCoin = txb.splitCoins(txb.gas, [walletFee]);
		txnResult = await deepBookClient.placeMarketOrder(
			accountCap,
			poolId,
			BigInt(balanceToSwap),
			'ask',
			swapCoin,
			undefined,
			undefined,
			address,
			txb,
		);
	} else {
		const primaryCoinInput = txb.object(quoteCoins[0].coinObjectId);
		const restCoins = quoteCoins.slice(1);

		if (restCoins.length) {
			txb.mergeCoins(
				primaryCoinInput,
				restCoins.map((coin) => txb.object(coin.coinObjectId)),
			);
		}

		const walletFee = new BigNumber(quoteBalance)
			.times(WALLET_FEES_PERCENTAGE / 100)
			.integerValue(BigNumber.ROUND_DOWN)
			.toString();

		balanceToSwap = new BigNumber(quoteBalance).minus(walletFee).toString();

		const [swapCoin, walletCoin] = txb.splitCoins(primaryCoinInput, [balanceToSwap, walletFee]);

		txnResult = await deepBookClient.swapExactQuoteForBase(
			poolId,
			swapCoin,
			BigInt(balanceToSwap),
			address,
			undefined,
			txb,
		);

		walletFeeCoin = walletCoin;
	}

	if (!accountCapId) {
		txnResult.transferObjects([accountCap], address);
	}

	if (walletFeeCoin) txnResult.transferObjects([walletFeeCoin], walletFeeAddress);

	return txnResult;
}

export function useGetEstimate({
	accountCapId,
	signer,
	coinType,
	poolId,
	baseBalance,
	quoteBalance,
	isAsk,
}: {
	accountCapId: string;
	signer: WalletSigner | null;
	coinType: string;
	poolId: string;
	baseBalance: string;
	quoteBalance: string;
	isAsk: boolean;
}) {
	const walletFeeAddress = useFeatureValue(FEATURES.WALLET_FEE_ADDRESS, DEFAULT_WALLET_FEE_ADDRESS);
	const queryClient = useQueryClient();
	const suiClient = useSuiClient();
	const activeAccount = useActiveAccount();
	const activeAddress = activeAccount?.address;
	const deepBookClient = useDeepBookContext().client;

	const { data } = useGetObject(poolId);
	const objectFields =
		data?.data?.content?.dataType === 'moveObject' ? data?.data?.content?.fields : null;

	const lotSize = (objectFields as Record<string, string>)?.lot_size;

	return useQuery({
		// eslint-disable-next-line @tanstack/query/exhaustive-deps
		queryKey: [
			DEEPBOOK_KEY,
			'get-estimate',
			poolId,
			accountCapId,
			coinType,
			activeAddress,
			baseBalance,
			quoteBalance,
			isAsk,
			lotSize,
		],
		queryFn: async () => {
			const [baseCoins, quoteCoins] = await Promise.all([
				getCoinsByBalance({
					coinType,
					balance: baseBalance,
					suiClient,
					address: activeAddress!,
				}),
				getCoinsByBalance({
					coinType,
					balance: quoteBalance,
					suiClient,
					address: activeAddress!,
				}),
			]);

			if ((isAsk && !baseCoins.length) || (!isAsk && !quoteCoins.length)) {
				throw new Error('No coins found in balance');
			}

			const txn = await getPlaceMarketOrderTxn({
				deepBookClient,
				poolId,
				accountCapId,
				address: activeAddress!,
				isAsk,
				lotSize: Number(lotSize),
				baseCoins,
				quoteCoins,
				baseBalance,
				quoteBalance,
				walletFeeAddress,
			});

			if (!accountCapId) {
				await queryClient.invalidateQueries({ queryKey: ['get-owned-objects'] });
			}

			const dryRunResponse = await signer!.dryRunTransactionBlock({ transactionBlock: txn });

			return {
				txn,
				dryRunResponse,
			};
		},
		enabled:
			!!baseBalance &&
			baseBalance !== '0' &&
			!!quoteBalance &&
			quoteBalance !== '0' &&
			!!signer &&
			!!activeAddress,
	});
}
