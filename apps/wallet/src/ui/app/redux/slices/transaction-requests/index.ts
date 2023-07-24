// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
	type SignedMessage,
	type SignedTransaction,
	type SuiTransactionBlockResponse,
} from '@mysten/sui.js';
import { TransactionBlock } from '@mysten/sui.js/transactions';
import { fromB64 } from '@mysten/sui.js/utils';
import { createAsyncThunk, createEntityAdapter, createSlice } from '@reduxjs/toolkit';

import { type WalletSigner } from '_src/ui/app/WalletSigner';
import { getSignerOperationErrorMessage } from '_src/ui/app/helpers/errorMessages';

import type { PayloadAction } from '@reduxjs/toolkit';
import type { ApprovalRequest } from '_payloads/transactions/ApprovalRequest';
import type { RootState } from '_redux/RootReducer';
import type { AppThunkConfig } from '_store/thunk-extras';

const txRequestsAdapter = createEntityAdapter<ApprovalRequest>({
	sortComparer: (a, b) => {
		const aDate = new Date(a.createdDate);
		const bDate = new Date(b.createdDate);
		return aDate.getTime() - bDate.getTime();
	},
});

export const respondToTransactionRequest = createAsyncThunk<
	{
		txRequestID: string;
		approved: boolean;
		txResponse: SuiTransactionBlockResponse | null;
	},
	{
		txRequestID: string;
		approved: boolean;
		signer: WalletSigner;
		clientIdentifier?: string;
	},
	AppThunkConfig
>(
	'respond-to-transaction-request',
	async (
		{ txRequestID, approved, signer, clientIdentifier },
		{ extra: { background }, getState },
	) => {
		const state = getState();
		const txRequest = txRequestsSelectors.selectById(state, txRequestID);
		if (!txRequest) {
			throw new Error(`TransactionRequest ${txRequestID} not found`);
		}
		let txSigned: SignedTransaction | undefined = undefined;
		let txResult: SuiTransactionBlockResponse | SignedMessage | undefined = undefined;
		let txResultError: string | undefined;
		if (approved) {
			try {
				if (txRequest.tx.type === 'sign-message') {
					txResult = await signer.signMessage(
						{
							message: fromB64(txRequest.tx.message),
						},
						clientIdentifier,
					);
				} else if (txRequest.tx.type === 'transaction') {
					const tx = TransactionBlock.from(txRequest.tx.data);
					if (txRequest.tx.justSign) {
						// Just a signing request, do not submit
						txSigned = await signer.signTransactionBlock(
							{
								transactionBlock: tx,
							},
							clientIdentifier,
						);
					} else {
						txResult = await signer.signAndExecuteTransactionBlock(
							{
								transactionBlock: tx,
								options: txRequest.tx.options,
								requestType: txRequest.tx.requestType,
							},
							clientIdentifier,
						);
					}
				} else {
					throw new Error(
						// eslint-disable-next-line @typescript-eslint/no-explicit-any
						`Unexpected type: ${(txRequest.tx as any).type}`,
					);
				}
			} catch (error) {
				txResultError = getSignerOperationErrorMessage(error);
			}
		}
		background.sendTransactionRequestResponse(
			txRequestID,
			approved,
			txResult,
			txResultError,
			txSigned,
		);
		return { txRequestID, approved: approved, txResponse: null };
	},
);

const slice = createSlice({
	name: 'transaction-requests',
	initialState: txRequestsAdapter.getInitialState({
		initialized: false,
	}),
	reducers: {
		setTransactionRequests: (state, { payload }: PayloadAction<ApprovalRequest[]>) => {
			// eslint-disable-next-line @typescript-eslint/ban-ts-comment
			// @ts-ignore
			txRequestsAdapter.setAll(state, payload);
			state.initialized = true;
		},
	},
	extraReducers: (build) => {
		build.addCase(respondToTransactionRequest.fulfilled, (state, { payload }) => {
			const { txRequestID, approved: allowed, txResponse } = payload;
			txRequestsAdapter.updateOne(state, {
				id: txRequestID,
				changes: {
					approved: allowed,
					txResult: txResponse || undefined,
				},
			});
		});
	},
});

export default slice.reducer;

export const { setTransactionRequests } = slice.actions;

export const txRequestsSelectors = txRequestsAdapter.getSelectors(
	(state: RootState) => state.transactionRequests,
);
