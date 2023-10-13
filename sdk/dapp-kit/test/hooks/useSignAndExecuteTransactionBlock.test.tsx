// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getFullnodeUrl, SuiClient } from '@mysten/sui.js/client';
import { TransactionBlock } from '@mysten/sui.js/transactions';
import { act, renderHook, waitFor } from '@testing-library/react';
import type { Mock } from 'vitest';

import {
	WalletFeatureNotSupportedError,
	WalletNotConnectedError,
} from '../../src/errors/walletErrors.js';
import { useConnectWallet, useSignAndExecuteTransactionBlock } from '../../src/index.js';
import { suiFeatures } from '../mocks/mockFeatures.js';
import { createWalletProviderContextWrapper, registerMockWallet } from '../test-utils.js';

describe('useSignAndExecuteTransactionBlock', () => {
	test('throws an error when trying to sign and execute a transaction block without a wallet connection', async () => {
		const wrapper = createWalletProviderContextWrapper();
		const { result } = renderHook(() => useSignAndExecuteTransactionBlock(), { wrapper });

		result.current.mutate({ transactionBlock: new TransactionBlock(), chain: 'sui:testnet' });

		await waitFor(() => expect(result.current.error).toBeInstanceOf(WalletNotConnectedError));
	});

	test('throws an error when trying to sign and execute a transaction block with a wallet that lacks feature support', async () => {
		const { unregister, mockWallet } = registerMockWallet({
			walletName: 'Mock Wallet 1',
		});

		const wrapper = createWalletProviderContextWrapper();
		const { result } = renderHook(
			() => ({
				connectWallet: useConnectWallet(),
				useSignAndExecuteTransactionBlock: useSignAndExecuteTransactionBlock(),
			}),
			{ wrapper },
		);

		result.current.connectWallet.mutate({ wallet: mockWallet });
		await waitFor(() => expect(result.current.connectWallet.isSuccess).toBe(true));

		result.current.useSignAndExecuteTransactionBlock.mutate({
			transactionBlock: new TransactionBlock(),
			chain: 'sui:testnet',
		});
		await waitFor(() =>
			expect(result.current.useSignAndExecuteTransactionBlock.error).toBeInstanceOf(
				WalletFeatureNotSupportedError,
			),
		);

		act(() => unregister());
	});

	test('signing and executing a transaction block from the currently connected account works successfully', async () => {
		const { unregister, mockWallet } = registerMockWallet({
			walletName: 'Mock Wallet 1',
			features: suiFeatures,
		});

		const suiClient = new SuiClient({ url: getFullnodeUrl('localnet') });
		const executeTransactionBlock = vi.spyOn(suiClient, 'executeTransactionBlock');

		executeTransactionBlock.mockReturnValueOnce(Promise.resolve({ digest: '123' }));

		const wrapper = createWalletProviderContextWrapper({}, suiClient);
		const { result } = renderHook(
			() => ({
				connectWallet: useConnectWallet(),
				useSignAndExecuteTransactionBlock: useSignAndExecuteTransactionBlock(),
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

		result.current.useSignAndExecuteTransactionBlock.mutate({
			transactionBlock: new TransactionBlock(),
			chain: 'sui:testnet',
		});

		await waitFor(() =>
			expect(result.current.useSignAndExecuteTransactionBlock.isSuccess).toBe(true),
		);
		expect(result.current.useSignAndExecuteTransactionBlock.data).toStrictEqual({
			digest: '123',
		});
		expect(suiClient.executeTransactionBlock).toHaveBeenCalledWith({
			transactionBlock: 'abc',
			signature: '123',
		});

		act(() => unregister());
	});
});
