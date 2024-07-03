// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { Text } from '_app/shared/text';
import { DescriptionItem } from '_pages/approval-request/transaction-request/DescriptionList';
import { DEFAULT_WALLET_FEE_ADDRESS, WALLET_FEES_PERCENTAGE } from '_pages/swap/constants';
import { getUSDCurrency } from '_pages/swap/utils';
import { GAS_TYPE_ARG } from '_redux/slices/sui-objects/Coin';
import { FEATURES } from '_shared/experimentation/features';
import { useFeatureValue } from '@growthbook/growthbook-react';
import { useBalanceInUSD, useFormatCoin } from '@mysten/core';
import { type BalanceChange } from '@mysten/sui/client';

export function GasFeeSection({
	activeCoinType,
	totalGas,
	isValid,
	balanceChanges,
}: {
	activeCoinType: string | null;
	isValid: boolean;
	totalGas: string;
	balanceChanges: BalanceChange[];
}) {
	const walletFeeAddress = useFeatureValue(FEATURES.WALLET_FEE_ADDRESS, DEFAULT_WALLET_FEE_ADDRESS);
	const estimatedAccessFeesBalance = balanceChanges.find(
		(change) =>
			'owner' in change &&
			typeof change.owner === 'object' &&
			'AddressOwner' in change.owner &&
			change.owner.AddressOwner === walletFeeAddress,
	)?.amount;
	const [formattedEstimatedFees, balanceSymbol] = useFormatCoin(
		estimatedAccessFeesBalance,
		activeCoinType,
	);
	const usdValue = useBalanceInUSD(activeCoinType || '', estimatedAccessFeesBalance || '');
	const [gas, symbol] = useFormatCoin(totalGas, GAS_TYPE_ARG);

	return (
		<div className="flex flex-col border border-hero-darkest/20 rounded-xl px-5 py-3 gap-2 border-solid">
			<DescriptionItem
				title={
					<Text variant="bodySmall" weight="medium" color="steel-dark">
						Access Fees ({WALLET_FEES_PERCENTAGE}%)
					</Text>
				}
			>
				<Text variant="bodySmall" weight="medium" color="steel-darker">
					{formattedEstimatedFees
						? `${formattedEstimatedFees} ${balanceSymbol} (${getUSDCurrency(usdValue)})`
						: '--'}
				</Text>
			</DescriptionItem>

			<div className="bg-gray-40 h-px w-full" />

			<DescriptionItem
				title={
					<Text variant="bodySmall" weight="medium" color="steel-dark">
						Estimated Gas Fee
					</Text>
				}
			>
				<Text variant="bodySmall" weight="medium" color="steel-darker">
					{totalGas && isValid ? `${gas} ${symbol}` : '--'}
				</Text>
			</DescriptionItem>
		</div>
	);
}
