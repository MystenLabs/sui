// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getAmount } from '_helpers';
import type { SuiTransactionBlockResponse } from '@mysten/sui/client';
import { SUI_TYPE_ARG } from '@mysten/sui/utils';
import { useMemo } from 'react';

export function useGetTransferAmount({
	txn,
	activeAddress,
}: {
	txn: SuiTransactionBlockResponse;
	activeAddress: string;
}) {
	const { effects, events } = txn;
	// const { coins } = getEventsSummary(events!, activeAddress);

	const suiTransfer = useMemo(() => {
		const txdetails = txn.transaction?.data.transaction!;
		return getAmount(txdetails, effects!, events!)?.map(
			({ amount, coinType, recipientAddress }) => {
				return {
					amount: amount || 0,
					coinType: coinType || SUI_TYPE_ARG,
					receiverAddress: recipientAddress,
				};
			},
		);
	}, [txn, effects, events]);

	// MUSTFIX(chris)
	// const transferAmount = useMemo(() => {
	//     return suiTransfer?.length
	//         ? suiTransfer
	//         : coins.filter(
	//               ({ receiverAddress }) => receiverAddress === activeAddress
	//           );
	// }, [suiTransfer, coins, activeAddress]);

	// return suiTransfer ?? transferAmount;
	return suiTransfer;
}
