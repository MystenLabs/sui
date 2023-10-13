// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { TransactionBlock } from '@mysten/sui.js/transactions';
import { act, renderHook, waitFor } from '@testing-library/react';
import type { Mock } from 'vitest';

import {
	WalletFeatureNotSupportedError,
	WalletNotConnectedError,
} from '../../src/errors/walletErrors.js';
import { useConnectWallet, useSignTransactionBlock } from '../../src/index.js';
import { suiFeatures } from '../mocks/mockFeatures.js';
import { createWalletProviderContextWrapper, registerMockWallet } from '../test-utils.js';

describe('useSignTransactionBlock', () => {
	test('throws an error when trying to sign a transaction block without a wallet connection', async () => {
		const wrapper = createWalletProviderContextWrapper();
		const { result } = renderHook(() => useSignTransactionBlock(), { wrapper });

		result.current.mutate({ transactionBlock: new TransactionBlock(), chain: 'sui:testnet' });

		await waitFor(() => expect(result.current.error).toBeInstanceOf(WalletNotConnectedError));
	});

	test('throws an error when trying to sign a transaction block with a wallet that lacks feature support', async () => {
		const { unregister, mockWallet } = registerMockWallet({
			walletName: 'Mock Wallet 1',
		});

		const wrapper = createWalletProviderContextWrapper();
		const { result } = renderHook(
			() => ({
				connectWallet: useConnectWallet(),
				signTransactionBlock: useSignTransactionBlock(),
			}),
			{ wrapper },
		);

		result.current.connectWallet.mutate({ wallet: mockWallet });
		await waitFor(() => expect(result.current.connectWallet.isSuccess).toBe(true));

		result.current.signTransactionBlock.mutate({
			transactionBlock: new TransactionBlock(),
			chain: 'sui:testnet',
		});
		await waitFor(() =>
			expect(result.current.signTransactionBlock.error).toBeInstanceOf(
				WalletFeatureNotSupportedError,
			),
		);

		act(() => unregister());
	});

	test('signing a transaction block from the currently connected account works successfully', async () => {
		const { unregister, mockWallet } = registerMockWallet({
			walletName: 'Mock Wallet 1',
			features: suiFeatures,
		});

		const wrapper = createWalletProviderContextWrapper();
		const { result } = renderHook(
			() => ({
				connectWallet: useConnectWallet(),
				signTransactionBlock: useSignTransactionBlock(),
			}),
			{ wrapper },
		);

		result.current.connectWallet.mutate({ wallet: mockWallet });

		await waitFor(() => expect(result.current.connectWallet.isSuccess).toBe(true));

		const signTransactionBlockFeature = mockWallet.features['sui:signTransactionBlock'];
		const signTransactionBlockMock = signTransactionBlockFeature!.signTransactionBlock as Mock;

		signTransactionBlockMock.mockReturnValueOnce({
			transactionBlockBytes: 'abc',
			signature: '123',
		});

		result.current.signTransactionBlock.mutate({
			transactionBlock: new TransactionBlock(),
			chain: 'sui:testnet',
		});

		await waitFor(() => expect(result.current.signTransactionBlock.isSuccess).toBe(true));
		expect(result.current.signTransactionBlock.data).toStrictEqual({
			transactionBlockBytes: 'abc',
			signature: '123',
		});

		act(() => unregister());
	});
});
