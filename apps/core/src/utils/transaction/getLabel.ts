// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SuiTransactionBlockResponse, getTransactionSender } from '@mysten/sui.js';

// todo: add more logic for deriving transaction label
export const getLabel = (transaction: SuiTransactionBlockResponse, currentAddress?: string) => {
	const isSender = getTransactionSender(transaction) === currentAddress;
	// Rename to "Send" to Transaction
	return isSender ? 'Transaction' : 'Receive';
};
