// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isBasePayload } from '_payloads';
import type { BasePayload, Payload } from '_payloads';
import type { SuiTransactionBlockResponse } from '@mysten/sui/client';
import { type SuiSignTransactionBlockOutput } from '@mysten/wallet-standard';

export interface ExecuteTransactionResponse extends BasePayload {
	type: 'execute-transaction-response';
	result: SuiTransactionBlockResponse;
}

export function isExecuteTransactionResponse(
	payload: Payload,
): payload is ExecuteTransactionResponse {
	return isBasePayload(payload) && payload.type === 'execute-transaction-response';
}

export interface SignTransactionResponse extends BasePayload {
	type: 'sign-transaction-response';
	result: SuiSignTransactionBlockOutput;
}

export function isSignTransactionResponse(payload: Payload): payload is SignTransactionResponse {
	return isBasePayload(payload) && payload.type === 'sign-transaction-response';
}
