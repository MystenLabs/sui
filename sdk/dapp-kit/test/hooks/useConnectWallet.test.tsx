// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { renderHook, waitFor, act } from '@testing-library/react';
import { useConnectWallet, useWallet } from 'dapp-kit/src';
import { createWalletProviderContextWrapper, registerMockWallet } from '../test-utils.js';
import { WalletAlreadyConnectedError } from 'dapp-kit/src/errors/walletErrors.js';
import type { Mock } from 'vitest';

describe('useConnectWallet', () => {
	test('throws an error when connecting to a wallet when a connection is already active', async () => {
		const { unregister, mockWallet } = registerMockWallet('Mock Wallet 1');

		const wrapper = createWalletProviderContextWrapper();
		const { result } = renderHook(
			() => ({
				connectWallet: useConnectWallet(),
				walletInfo: useWallet(),
			}),
			{ wrapper },
		);

		result.current.connectWallet.mutate({ wallet: mockWallet });
		await waitFor(() => expect(result.current.connectWallet.isSuccess).toBe(true));

		result.current.connectWallet.mutate({ wallet: mockWallet });
		await waitFor(() =>
			expect(result.current.connectWallet.error).toBeInstanceOf(WalletAlreadyConnectedError),
		);

		act(() => {
			unregister();
		});
	});

	test('throws an error when a user fails to connect their wallet', async () => {
		const { unregister, mockWallet } = registerMockWallet('Mock Wallet 1');

		const wrapper = createWalletProviderContextWrapper();
		const { result } = renderHook(
			() => ({
				connectWallet: useConnectWallet(),
				walletInfo: useWallet(),
			}),
			{ wrapper },
		);

		const connectFeature = result.current.walletInfo.wallets[0].features['standard:connect'];
		const mockConnect = connectFeature.connect as Mock;

		mockConnect.mockRejectedValueOnce(() => {
			throw new Error('User rejected request');
		});

		result.current.connectWallet.mutate({ wallet: mockWallet });

		await waitFor(() => expect(result.current.connectWallet.isError).toBe(true));
		expect(result.current.walletInfo.connectionStatus).toBe('disconnected');

		act(() => {
			unregister();
		});
	});

	test('connecting to a wallet works successfully', async () => {
		const { unregister, mockWallet } = registerMockWallet('Mock Wallet 1');

		const wrapper = createWalletProviderContextWrapper();
		const { result } = renderHook(
			() => ({
				connectWallet: useConnectWallet(),
				walletInfo: useWallet(),
			}),
			{ wrapper },
		);

		result.current.connectWallet.mutate({ wallet: mockWallet });

		await waitFor(() => expect(result.current.connectWallet.isSuccess).toBe(true));
		expect(result.current.walletInfo.currentWallet?.name).toBe('Mock Wallet 1');
		expect(result.current.walletInfo.accounts).toHaveLength(1);
		expect(result.current.walletInfo.currentAccount).toBeTruthy();
		expect(result.current.walletInfo.connectionStatus).toBe('connected');

		const savedConnectionInfo = window.localStorage.getItem('sui-dapp-kit:wallet-connection-info');
		expect(savedConnectionInfo).toBeTruthy();
		expect(JSON.parse(savedConnectionInfo!)).toStrictEqual({
			walletName: 'Mock Wallet 1',
			accountAddress: result.current.walletInfo.currentAccount?.address,
		});

		act(() => {
			unregister();
		});
	});
});
