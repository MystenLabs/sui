// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { act, renderHook, waitFor } from '@testing-library/react';
import type { Mock } from 'vitest';

import {
	useAccounts,
	useConnectWallet,
	useCurrentAccount,
	useCurrentWallet,
} from '../../src/index.js';
import { createMockAccount } from '../mocks/mockAccount.js';
import { createWalletProviderContextWrapper, registerMockWallet } from '../test-utils.js';

describe('useConnectWallet', () => {
	test('throws an error when a user fails to connect their wallet', async () => {
		const { unregister, mockWallet } = registerMockWallet({ walletName: 'Mock Wallet 1' });

		const wrapper = createWalletProviderContextWrapper();
		const { result } = renderHook(
			() => ({
				connectWallet: useConnectWallet(),
				currentWallet: useCurrentWallet(),
				currentAccount: useCurrentAccount(),
			}),
			{ wrapper },
		);

		const connectFeature = mockWallet.features['standard:connect'];
		const mockConnect = connectFeature.connect as Mock;

		mockConnect.mockRejectedValueOnce(() => {
			throw new Error('User rejected request');
		});

		result.current.connectWallet.mutate({ wallet: mockWallet });

		await waitFor(() => expect(result.current.connectWallet.isError).toBe(true));
		expect(result.current.currentWallet.isDisconnected).toBe(true);
		expect(result.current.currentAccount).toBeFalsy();

		act(() => {
			unregister();
		});
	});

	test('connecting to a wallet works successfully', async () => {
		const { unregister, mockWallet } = registerMockWallet({ walletName: 'Mock Wallet 1' });

		const wrapper = createWalletProviderContextWrapper();
		const { result } = renderHook(
			() => ({
				connectWallet: useConnectWallet(),
				accounts: useAccounts(),
				currentWallet: useCurrentWallet(),
				currentAccount: useCurrentAccount(),
			}),
			{ wrapper },
		);

		result.current.connectWallet.mutate({ wallet: mockWallet });

		await waitFor(() => expect(result.current.connectWallet.isSuccess).toBe(true));
		expect(result.current.currentWallet.isConnected).toBe(true);
		expect(result.current.currentWallet.currentWallet!.name).toBe('Mock Wallet 1');
		expect(result.current.accounts).toHaveLength(1);
		expect(result.current.currentAccount).toBeTruthy();

		act(() => {
			unregister();
		});
	});

	test('only Sui accounts from multi-chain wallets are connected', async () => {
		const { unregister, mockWallet } = registerMockWallet({
			walletName: 'Mock Wallet 1',
			accounts: [createMockAccount(), createMockAccount({ chains: ['solana:mainnet'] })],
		});

		const wrapper = createWalletProviderContextWrapper();
		const { result } = renderHook(
			() => ({
				connectWallet: useConnectWallet(),
				accounts: useAccounts(),
				currentWallet: useCurrentWallet(),
				currentAccount: useCurrentAccount(),
			}),
			{ wrapper },
		);

		result.current.connectWallet.mutate({ wallet: mockWallet });

		await waitFor(() => expect(result.current.connectWallet.isSuccess).toBe(true));
		expect(result.current.currentWallet.isConnected).toBe(true);
		expect(result.current.currentWallet.currentWallet!.name).toBe('Mock Wallet 1');
		expect(result.current.accounts).toHaveLength(1);
		expect(result.current.currentAccount).toBeTruthy();

		act(() => {
			unregister();
		});
	});
});
