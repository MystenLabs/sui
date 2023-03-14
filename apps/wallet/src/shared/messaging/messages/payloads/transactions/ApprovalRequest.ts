// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    type SuiSignMessageOutput,
    type SuiSignMessageOptions,
    type SuiSignAndExecuteTransactionOptions,
} from '@mysten/wallet-standard';

import type {
    SignedTransaction,
    SuiAddress,
    SuiTransactionResponse,
} from '@mysten/sui.js';

export type TransactionDataType = {
    type: 'transaction';
    data: string;
    account: SuiAddress;
    justSign?: boolean;
    options?: SuiSignAndExecuteTransactionOptions;
};

export type SignMessageDataType = {
    type: 'sign-message';
    message: string;
    accountAddress: SuiAddress;
    options?: SuiSignMessageOptions;
};

export type ApprovalRequest = {
    id: string;
    approved: boolean | null;
    origin: string;
    originFavIcon?: string;
    txResult?: SuiTransactionResponse | SuiSignMessageOutput;
    txResultError?: string;
    txSigned?: SignedTransaction;
    createdDate: string;
    tx: TransactionDataType | SignMessageDataType;
};

export interface SignMessageApprovalRequest
    extends Omit<ApprovalRequest, 'txResult' | 'tx'> {
    tx: SignMessageDataType;
    txResult?: SuiSignMessageOutput;
}

export interface TransactionApprovalRequest
    extends Omit<ApprovalRequest, 'txResult' | 'tx'> {
    tx: TransactionDataType;
    txResult?: SuiTransactionResponse;
}

export function isSignMessageApprovalRequest(
    request: ApprovalRequest
): request is SignMessageApprovalRequest {
    return request.tx.type === 'sign-message';
}

export function isTransactionApprovalRequest(
    request: ApprovalRequest
): request is TransactionApprovalRequest {
    return request.tx.type !== 'sign-message';
}
