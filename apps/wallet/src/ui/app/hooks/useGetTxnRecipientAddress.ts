// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getAmount } from '_helpers';
import { type SuiTransactionBlockResponse } from '@mysten/sui/client';
import { useMemo } from 'react';

type Props = {
	txn: SuiTransactionBlockResponse;
	address: string;
};

export function useGetTxnRecipientAddress({ txn, address }: Props) {
	const events = txn.events!;

	// const eventsSummary = useMemo(() => {
	//     const { coins } = getEventsSummary(events, address);
	//     return coins;
	// }, [events, address]);

	const transaction = txn.transaction?.data.transaction!;
	const amountByRecipient = getAmount(transaction, txn.effects!, events);

	const recipientAddress = useMemo(() => {
		const transferObjectRecipientAddress =
			amountByRecipient &&
			amountByRecipient?.find(({ recipientAddress }) => recipientAddress !== address)
				?.recipientAddress;
		// MUSTFIX(chris)
		// const receiverAddr =
		//     eventsSummary &&
		//     eventsSummary.find(
		//         ({ receiverAddress }) => receiverAddress !== address
		//     )?.receiverAddress;

		return null ?? transferObjectRecipientAddress ?? txn.transaction?.data.sender;
	}, [address, amountByRecipient, txn]);
	// }, [address, amountByRecipient, eventsSummary, txn]);

	return recipientAddress;
}
