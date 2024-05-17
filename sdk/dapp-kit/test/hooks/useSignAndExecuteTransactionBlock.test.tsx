// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getFullnodeUrl, SuiClient } from '@mysten/sui/client';
import { TransactionBlock } from '@mysten/sui/transactions';
import { act, renderHook, waitFor } from '@testing-library/react';
import { expect, type Mock } from 'vitest';

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
		const mockSignMessageFeature = mockWallet.features['sui:signTransactionBlock:v2'];
		const signTransactionBlock = mockSignMessageFeature!.signTransactionBlock as Mock;

		signTransactionBlock.mockReturnValueOnce({
			bytes: 'abc',
			signature: '123',
		});

		const reportEffectsFeature = mockWallet.features['sui:reportTransactionBlockEffects'];
		const reportEffects = reportEffectsFeature!.reportTransactionBlockEffects as Mock;

		reportEffects.mockImplementation(async () => {});

		const executeTransactionBlock = vi.spyOn(suiClient, 'executeTransactionBlock');

		executeTransactionBlock.mockResolvedValueOnce({
			digest: '123',
			rawEffects: [10, 20, 30],
		});

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
			bytes: 'abc',
			digest: '123',
			effects: 'ChQe',
			signature: '123',
		});
		expect(reportEffects).toHaveBeenCalledWith({
			effects: 'ChQe',
		});

		const call = signTransactionBlock.mock.calls[0];

		expect(call[0].account).toStrictEqual(mockWallet.accounts[0]);
		expect(call[0].chain).toBe('sui:testnet');
		expect(await call[0].transactionBlock.toJSON()).toEqual(await new TransactionBlock().toJSON());

		act(() => unregister());
	});
});
