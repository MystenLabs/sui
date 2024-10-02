// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isBasePayload } from '_payloads';
import type { BasePayload, Payload } from '_payloads';
import { type SignedTransaction } from '_src/ui/app/WalletSigner';
import type { SuiTransactionBlockResponse } from '@mysten/sui/client';
import { type SuiSignMessageOutput } from '@mysten/wallet-standard';

export interface TransactionRequestResponse extends BasePayload {
	type: 'transaction-request-response';
	txID: string;
	approved: boolean;
	txResult?: SuiTransactionBlockResponse | SuiSignMessageOutput;
	txResultError?: string;
	txSigned?: SignedTransaction;
}

export function isTransactionRequestResponse(
	payload: Payload,
): payload is TransactionRequestResponse {
	return isBasePayload(payload) && payload.type === 'transaction-request-response';
}
