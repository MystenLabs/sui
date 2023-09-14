// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { renderHook, waitFor, act } from '@testing-library/react';
import { useConnectWallet, useDisconnectWallet, useWallet } from 'dapp-kit/src';
import { createWalletProviderContextWrapper, registerMockWallet } from '../test-utils.js';
import { WalletNotConnectedError } from 'dapp-kit/src/errors/walletErrors.js';

describe('useDisconnectWallet', () => {
	test('that an error is thrown when trying to disconnect with no active connection', async () => {
		const wrapper = createWalletProviderContextWrapper();
		const { result } = renderHook(() => useDisconnectWallet(), { wrapper });

		result.current.mutate();

		await waitFor(() => expect(result.current.error).toBeInstanceOf(WalletNotConnectedError));
	});

	test('that disconnecting works successfully', async () => {
		const { unregister, mockWallet } = registerMockWallet({ walletName: 'Mock Wallet 1' });

		const wrapper = createWalletProviderContextWrapper();
		const { result } = renderHook(
			() => ({
				connectWallet: useConnectWallet(),
				disconnectWallet: useDisconnectWallet(),
				walletInfo: useWallet(),
			}),
			{ wrapper },
		);

		result.current.connectWallet.mutate({ wallet: mockWallet });

		await waitFor(() => expect(result.current.connectWallet.isSuccess).toBe(true));
		expect(result.current.walletInfo.connectionStatus).toBe('connected');

		expect(window.localStorage.getItem('sui-dapp-kit:wallet-connection-info')).toBeTruthy();

		result.current.disconnectWallet.mutate();
		await waitFor(() => expect(result.current.disconnectWallet.isSuccess).toBe(true));

		expect(result.current.walletInfo.currentWallet).toBeNull();
		expect(result.current.walletInfo.accounts).toStrictEqual([]);
		expect(result.current.walletInfo.currentAccount).toBeNull();
		expect(result.current.walletInfo.connectionStatus).toBe('disconnected');

		expect(window.localStorage.getItem('sui-dapp-kit:wallet-connection-info')).toBeFalsy();

		act(() => {
			unregister();
		});
	});
});
