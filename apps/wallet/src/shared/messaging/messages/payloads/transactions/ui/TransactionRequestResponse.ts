// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiSignMessageOutput } from '@mysten/wallet-standard';

import { isBasePayload } from '_payloads';

import type { SignedTransaction, SuiTransactionBlockResponse } from '@mysten/sui.js';
import type { BasePayload, Payload } from '_payloads';

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
