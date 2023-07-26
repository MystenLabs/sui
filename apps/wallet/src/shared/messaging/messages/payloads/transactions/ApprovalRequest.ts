// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
	type SuiSignAndExecuteTransactionBlockInput,
	type SuiSignMessageOutput,
} from '@mysten/wallet-standard';

import type { SignedTransaction, SuiTransactionBlockResponse } from '@mysten/sui.js';

export type TransactionDataType = {
	type: 'transaction';
	data: string;
	account: string;
	justSign?: boolean;
	requestType?: SuiSignAndExecuteTransactionBlockInput['requestType'];
	options?: SuiSignAndExecuteTransactionBlockInput['options'];
};

export type SignMessageDataType = {
	type: 'sign-message';
	message: string;
	accountAddress: string;
};

export type ApprovalRequest = {
	id: string;
	approved: boolean | null;
	origin: string;
	originFavIcon?: string;
	txResult?: SuiTransactionBlockResponse | SuiSignMessageOutput;
	txResultError?: string;
	txSigned?: SignedTransaction;
	createdDate: string;
	tx: TransactionDataType | SignMessageDataType;
};

export interface SignMessageApprovalRequest extends Omit<ApprovalRequest, 'txResult' | 'tx'> {
	tx: SignMessageDataType;
	txResult?: SuiSignMessageOutput;
}

export interface TransactionApprovalRequest extends Omit<ApprovalRequest, 'txResult' | 'tx'> {
	tx: TransactionDataType;
	txResult?: SuiTransactionBlockResponse;
}

export function isSignMessageApprovalRequest(
	request: ApprovalRequest,
): request is SignMessageApprovalRequest {
	return request.tx.type === 'sign-message';
}

export function isTransactionApprovalRequest(
	request: ApprovalRequest,
): request is TransactionApprovalRequest {
	return request.tx.type !== 'sign-message';
}
