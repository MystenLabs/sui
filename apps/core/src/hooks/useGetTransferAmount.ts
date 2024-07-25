// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SuiTransactionBlockResponse } from '@mysten/sui/client';
import { SUI_TYPE_ARG } from '@mysten/sui/utils';
import { useMemo } from 'react';

import { getTotalGasUsed } from '../utils/transaction';

export function useGetTransferAmount(txnData: SuiTransactionBlockResponse) {
	const { balanceChanges } = txnData;
	const sender = txnData.transaction?.data.sender;
	const gas = txnData.effects && getTotalGasUsed(txnData.effects);
	const changes = useMemo(
		() =>
			balanceChanges
				? balanceChanges?.map(({ coinType, owner, amount }) => ({
						coinType,
						address:
							owner === 'Immutable'
								? 'Immutable'
								: 'AddressOwner' in owner
									? owner.AddressOwner
									: 'ObjectOwner' in owner
										? owner.ObjectOwner
										: '',
						amount:
							coinType === SUI_TYPE_ARG && BigInt(amount) < 0n
								? BigInt(amount) + BigInt(gas ?? 0n)
								: BigInt(amount),
					}))
				: [],
		[balanceChanges, gas],
	);
	// take absolute value of the first balance change entry for display
	const [change] = changes;
	const amount = change?.amount ? (change.amount < 0n ? -change.amount : change.amount) : 0n;

	return {
		balanceChanges: changes,
		coinType: change?.coinType,
		gas,
		sender,
		amount,
	};
}
