// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { renderHook, waitFor, act } from '@testing-library/react';
import { useConnectWallet, useCurrentAccount } from 'dapp-kit/src';
import { createWalletProviderContextWrapper, registerMockWallet } from '../test-utils.js';
import { useSwitchAccount } from 'dapp-kit/src/hooks/wallet/useSwitchAccount.js';
import {
	WalletAccountNotFoundError,
	WalletNotConnectedError,
} from 'dapp-kit/src/errors/walletErrors.js';
import { createMockAccount } from '../mocks/mockAccount.js';

describe('useSwitchAccount', () => {
	test('throws an error when trying to switch accounts with no active connection', async () => {
		const wrapper = createWalletProviderContextWrapper();
		const { result } = renderHook(() => useSwitchAccount(), { wrapper });

		result.current.mutate({ account: createMockAccount() });
		await waitFor(() => expect(result.current.error).toBeInstanceOf(WalletNotConnectedError));
	});

	test('throws an error when trying to switch to a non-authorized account', async () => {
		const { unregister, mockWallet } = registerMockWallet({ walletName: 'Mock Wallet 1' });

		const wrapper = createWalletProviderContextWrapper();
		const { result } = renderHook(
			() => ({
				connectWallet: useConnectWallet(),
				switchAccount: useSwitchAccount(),
			}),
			{ wrapper },
		);

		result.current.connectWallet.mutate({ wallet: mockWallet });
		await waitFor(() => expect(result.current.connectWallet.isSuccess).toBe(true));

		result.current.switchAccount.mutate({ account: createMockAccount() });
		await waitFor(() =>
			expect(result.current.switchAccount.error).toBeInstanceOf(WalletAccountNotFoundError),
		);

		act(() => unregister());
	});

	test('switching accounts works successfully', async () => {
		const { unregister, mockWallet } = registerMockWallet({
			walletName: 'Mock Wallet 1',
			accounts: [createMockAccount(), createMockAccount(), createMockAccount()],
		});

		const wrapper = createWalletProviderContextWrapper();
		const { result } = renderHook(
			() => ({
				connectWallet: useConnectWallet(),
				switchAccount: useSwitchAccount(),
				currentAccount: useCurrentAccount(),
			}),
			{ wrapper },
		);

		result.current.connectWallet.mutate({ wallet: mockWallet });
		await waitFor(() => expect(result.current.connectWallet.isSuccess).toBe(true));
		expect(result.current.currentAccount).toBeTruthy();

		result.current.switchAccount.mutate({ account: mockWallet.accounts[1] });
		await waitFor(() => expect(result.current.switchAccount.isSuccess).toBe(true));
		expect(result.current.currentAccount!.address).toBe(mockWallet.accounts[1].address);

		act(() => unregister());
	});
});
