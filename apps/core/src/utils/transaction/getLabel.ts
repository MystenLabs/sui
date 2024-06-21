// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SuiTransactionBlockResponse } from '@mysten/sui/client';

// todo: add more logic for deriving transaction label
export const getLabel = (transaction: SuiTransactionBlockResponse, currentAddress?: string) => {
	const isSender = transaction.transaction?.data.sender === currentAddress;
	// Rename to "Send" to Transaction
	return isSender ? 'Transaction' : 'Receive';
};
