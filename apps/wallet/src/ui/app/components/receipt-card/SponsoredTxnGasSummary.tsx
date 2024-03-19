// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { GAS_SYMBOL, GAS_TYPE_ARG } from '_redux/slices/sui-objects/Coin';
import { Text } from '_src/ui/app/shared/text';
import { useFormatCoin } from '@mysten/core';

import { TxnAddressLink } from './TxnAddressLink';

type SponsoredTxnGasSummaryProps = {
	totalGas: number;
	sponsor: string;
};

export function SponsoredTxnGasSummary({ totalGas, sponsor }: SponsoredTxnGasSummaryProps) {
	const [sponsorTotalAmount, sponsorTotalAmountSymbol] = useFormatCoin(totalGas, GAS_TYPE_ARG);

	return (
		<div className="flex flex-col w-full gap-3.5 border-t border-solid border-steel/20 border-x-0 border-b-0 py-3.5 first:pt-0">
			<Text variant="body" weight="medium" color="steel">
				Gas Fees
			</Text>
			<div className="flex justify-between items-center w-full">
				<Text variant="body" weight="medium" color="steel-darker">
					You Paid
				</Text>
				<Text variant="body" weight="medium" color="steel-darker">
					0 {GAS_SYMBOL}
				</Text>
			</div>
			<div className="flex justify-between items-center w-full">
				<Text variant="body" weight="medium" color="steel-darker">
					Paid by Sponsor
				</Text>
				<Text variant="body" weight="medium" color="steel-darker">
					{sponsorTotalAmount} {sponsorTotalAmountSymbol}
				</Text>
			</div>
			<div className="flex justify-between items-center w-full">
				<Text variant="body" weight="medium" color="steel-darker">
					Sponsor
				</Text>
				<TxnAddressLink address={sponsor} />
			</div>
		</div>
	);
}
