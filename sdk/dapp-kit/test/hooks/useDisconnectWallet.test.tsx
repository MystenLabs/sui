// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { renderHook, waitFor, act } from '@testing-library/react';
import {
	useConnectWallet,
	useDisconnectWallet,
	useCurrentAccount,
	useCurrentWallet,
	useConnectionStatus,
} from 'dapp-kit/src';
import { createWalletProviderContextWrappe, registerMockWallet } from '../test-utils.js';
import { WalletNotConnectedError } from 'dapp-kit/src/errors/walletErrors.js';

describe('useDisconnectWallet', () => {
	test('that an error is thrown when trying to disconnect with no active connection', async () => {
		const wrapper = createWalletProviderContextWrappe();
		const { result } = renderHook(() => useDisconnectWallet(), { wrapper });

		result.current.mutate();

		await waitFor(() => expect(result.current.error).toBeInstanceOf(WalletNotConnectedError));
	});

	test('that disconnecting works successfully', async () => {
		const { unregister, mockWallet } = registerMockWallet({ walletName: 'Mock Wallet 1' });

		const wrapper = createWalletProviderContextWrappe();
		const { result } = renderHook(
			() => ({
				connectWallet: useConnectWallet(),
				disconnectWallet: useDisconnectWallet(),
				currentWallet: useCurrentWallet(),
				currentAccount: useCurrentAccount(),
				connectionStatus: useConnectionStatus(),
			}),
			{ wrapper },
		);

		result.current.connectWallet.mutate({ wallet: mockWallet });

		await waitFor(() => expect(result.current.connectWallet.isSuccess).toBe(true));
		expect(result.current.connectionStatus).toBe('connected');

		result.current.disconnectWallet.mutate();
		await waitFor(() => expect(result.current.disconnectWallet.isSuccess).toBe(true));

		expect(result.current.currentWallet).toBeFalsy();
		expect(result.current.currentWallet?.accounts).toBeFalsy();
		expect(result.current.currentAccount).toBeFalsy();
		expect(result.current.connectionStatus).toBe('disconnected');

		act(() => {
			unregister();
		});
	});
});
