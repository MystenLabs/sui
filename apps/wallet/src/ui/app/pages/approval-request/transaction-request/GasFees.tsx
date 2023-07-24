// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { formatAddress } from '@mysten/sui.js';
import { type TransactionBlock } from '@mysten/sui.js/transactions';

import { DescriptionItem, DescriptionList } from './DescriptionList';
import { SummaryCard } from './SummaryCard';
import { useTransactionData, useTransactionGasBudget } from '_src/ui/app/hooks';
import { GAS_SYMBOL } from '_src/ui/app/redux/slices/sui-objects/Coin';

interface Props {
	sender?: string;
	transaction: TransactionBlock;
}

export function GasFees({ sender, transaction }: Props) {
	const { data: transactionData } = useTransactionData(sender, transaction);
	const { data: gasBudget, isLoading, isError } = useTransactionGasBudget(sender, transaction);
	const isSponsored =
		transactionData?.gasConfig.owner && transactionData.sender !== transactionData.gasConfig.owner;
	return (
		<SummaryCard
			header="Estimated Gas Fees"
			badge={
				isSponsored ? (
					<div className="bg-white text-success px-1.5 py-0.5 text-captionSmallExtra rounded-full font-medium uppercase">
						Sponsored
					</div>
				) : null
			}
			initialExpanded
		>
			<DescriptionList>
				<DescriptionItem title="You Pay">
					{isLoading
						? 'Estimating...'
						: isError
						? 'Gas estimation failed'
						: `${isSponsored ? 0 : gasBudget} ${GAS_SYMBOL}`}
				</DescriptionItem>
				{isSponsored && (
					<>
						<DescriptionItem title="Sponsor Pays">
							{gasBudget ? `${gasBudget} ${GAS_SYMBOL}` : '-'}
						</DescriptionItem>
						<DescriptionItem title="Sponsor">
							{formatAddress(transactionData!.gasConfig.owner!)}
						</DescriptionItem>
					</>
				)}
			</DescriptionList>
		</SummaryCard>
	);
}
