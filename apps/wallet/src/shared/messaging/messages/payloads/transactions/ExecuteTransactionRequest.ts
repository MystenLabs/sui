// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiSignTransactionBlockInput } from '@mysten/wallet-standard';

import { type TransactionDataType } from './ApprovalRequest';
import { isBasePayload } from '_payloads';

import type { BasePayload, Payload } from '_payloads';

export interface ExecuteTransactionRequest extends BasePayload {
	type: 'execute-transaction-request';
	transaction: TransactionDataType;
}

export function isExecuteTransactionRequest(
	payload: Payload,
): payload is ExecuteTransactionRequest {
	return isBasePayload(payload) && payload.type === 'execute-transaction-request';
}

export type SuiSignTransactionSerialized = Omit<
	SuiSignTransactionBlockInput,
	'transaction' | 'account'
> & {
	transaction: string;
	account: string;
};

export interface SignTransactionRequest extends BasePayload {
	type: 'sign-transaction-request';
	transaction: SuiSignTransactionSerialized;
}

export function isSignTransactionRequest(payload: Payload): payload is SignTransactionRequest {
	return isBasePayload(payload) && payload.type === 'sign-transaction-request';
}
