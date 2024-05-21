// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getFullnodeUrl, SuiClient } from '@mysten/sui/client';
import { Transaction } from '@mysten/sui/transactions';
import { act, renderHook, waitFor } from '@testing-library/react';
import { expect, type Mock } from 'vitest';

import {
	WalletFeatureNotSupportedError,
	WalletNotConnectedError,
} from '../../src/errors/walletErrors.js';
import { useConnectWallet, useSignAndExecuteTransaction } from '../../src/index.js';
import { suiFeatures } from '../mocks/mockFeatures.js';
import { createWalletProviderContextWrapper, registerMockWallet } from '../test-utils.js';

describe('useSignAndExecuteTransaction', () => {
	test('throws an error when trying to sign and execute a transaction without a wallet connection', async () => {
		const wrapper = createWalletProviderContextWrapper();
		const { result } = renderHook(() => useSignAndExecuteTransaction(), { wrapper });

		result.current.mutate({ transaction: new Transaction(), chain: 'sui:testnet' });

		await waitFor(() => expect(result.current.error).toBeInstanceOf(WalletNotConnectedError));
	});

	test('throws an error when trying to sign and execute a transaction with a wallet that lacks feature support', async () => {
		const { unregister, mockWallet } = registerMockWallet({
			walletName: 'Mock Wallet 1',
		});

		const wrapper = createWalletProviderContextWrapper();
		const { result } = renderHook(
			() => ({
				connectWallet: useConnectWallet(),
				useSignAndExecuteTransaction: useSignAndExecuteTransaction(),
			}),
			{ wrapper },
		);

		result.current.connectWallet.mutate({ wallet: mockWallet });
		await waitFor(() => expect(result.current.connectWallet.isSuccess).toBe(true));

		result.current.useSignAndExecuteTransaction.mutate({
			transaction: new Transaction(),
			chain: 'sui:testnet',
		});
		await waitFor(() =>
			expect(result.current.useSignAndExecuteTransaction.error).toBeInstanceOf(
				WalletFeatureNotSupportedError,
			),
		);

		act(() => unregister());
	});

	test('signing and executing a transaction from the currently connected account works successfully', async () => {
		const { unregister, mockWallet } = registerMockWallet({
			walletName: 'Mock Wallet 1',
			features: suiFeatures,
		});

		const suiClient = new SuiClient({ url: getFullnodeUrl('localnet') });
		const mockSignMessageFeature = mockWallet.features['sui:signTransaction'];
		const signTransaction = mockSignMessageFeature!.signTransaction as Mock;

		signTransaction.mockReturnValueOnce({
			bytes: 'abc',
			signature: '123',
		});

		const reportEffectsFeature = mockWallet.features['sui:reportTransactionEffects'];
		const reportEffects = reportEffectsFeature!.reportTransactionEffects as Mock;

		reportEffects.mockImplementation(async () => {});

		const executeTransaction = vi.spyOn(suiClient, 'executeTransactionBlock');

		executeTransaction.mockResolvedValueOnce({
			digest: '123',
			rawEffects: [10, 20, 30],
		});

		const wrapper = createWalletProviderContextWrapper({}, suiClient);
		const { result } = renderHook(
			() => ({
				connectWallet: useConnectWallet(),
				useSignAndExecuteTransaction: useSignAndExecuteTransaction(),
			}),
			{ wrapper },
		);

		result.current.connectWallet.mutate({ wallet: mockWallet });

		await waitFor(() => expect(result.current.connectWallet.isSuccess).toBe(true));

		const signTransactionFeature = mockWallet.features['sui:signTransaction'];
		const signTransactionMock = signTransactionFeature!.signTransaction as Mock;

		signTransactionMock.mockReturnValueOnce({
			transactionBytes: 'abc',
			signature: '123',
		});

		result.current.useSignAndExecuteTransaction.mutate({
			transaction: new Transaction(),
			chain: 'sui:testnet',
		});

		await waitFor(() => expect(result.current.useSignAndExecuteTransaction.isSuccess).toBe(true));
		expect(result.current.useSignAndExecuteTransaction.data).toStrictEqual({
			bytes: 'abc',
			digest: '123',
			effects: 'ChQe',
			signature: '123',
		});
		expect(reportEffects).toHaveBeenCalledWith({
			effects: 'ChQe',
		});

		const call = signTransaction.mock.calls[0];

		expect(call[0].account).toStrictEqual(mockWallet.accounts[0]);
		expect(call[0].chain).toBe('sui:testnet');
		expect(await call[0].transaction.toJSON()).toEqual(await new Transaction().toJSON());

		act(() => unregister());
	});
});
