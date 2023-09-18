// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { renderHook, waitFor, act } from '@testing-library/react';
import {
	useConnectWallet,
	useDisconnectWallet,
	useCurrentAccount,
	useCurrentWallet,
} from 'dapp-kit/src';
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
				currentWallet: useCurrentWallet(),
				currentAccount: useCurrentAccount(),
			}),
			{ wrapper },
		);

		result.current.connectWallet.mutate({ wallet: mockWallet });

		await waitFor(() => expect(result.current.connectWallet.isSuccess).toBe(true));
		expect(result.current.currentWallet).toBeTruthy();
		expect(result.current.currentAccount).toBeTruthy();

		result.current.disconnectWallet.mutate();
		await waitFor(() => expect(result.current.disconnectWallet.isSuccess).toBe(true));

		expect(result.current.currentWallet).toBeFalsy();
		expect(result.current.currentAccount).toBeFalsy();

		act(() => {
			unregister();
		});
	});
});
