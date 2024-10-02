// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { act, renderHook, waitFor } from '@testing-library/react';

import {
	useAccounts,
	useConnectWallet,
	useCurrentAccount,
	useCurrentWallet,
	useDisconnectWallet,
	useWallets,
} from '../../src/index.js';
import { createMockAccount } from '../mocks/mockAccount.js';
import { suiFeatures, superCoolFeature } from '../mocks/mockFeatures.js';
import { createWalletProviderContextWrapper, registerMockWallet } from '../test-utils.js';

describe('WalletProvider', () => {
	test('the correct wallet and account information is returned on initial render', () => {
		const wrapper = createWalletProviderContextWrapper();
		const { result } = renderHook(
			() => ({
				wallets: useWallets(),
				currentWallet: useCurrentWallet(),
				currentAccount: useCurrentAccount(),
			}),
			{ wrapper },
		);

		expect(result.current.currentWallet.isConnected).toBeFalsy();
		expect(result.current.currentAccount).toBeFalsy();
		expect(result.current.wallets).toHaveLength(0);
	});

	test('the list of wallets is ordered correctly by preference', () => {
		const { unregister: unregister1 } = registerMockWallet({
			walletName: 'Mock Wallet 1',
			features: suiFeatures,
		});
		const { unregister: unregister2 } = registerMockWallet({
			walletName: 'Mock Wallet 2',
			features: suiFeatures,
		});
		const { unregister: unregister3 } = registerMockWallet({
			walletName: 'Mock Wallet 3',
			features: suiFeatures,
		});

		const wrapper = createWalletProviderContextWrapper({
			preferredWallets: ['Mock Wallet 2', 'Mock Wallet 1'],
		});
		const { result } = renderHook(() => useWallets(), { wrapper });
		const walletNames = result.current.map((wallet) => wallet.name);

		expect(walletNames).toStrictEqual(['Mock Wallet 2', 'Mock Wallet 1', 'Mock Wallet 3']);

		act(() => {
			unregister1();
			unregister2();
			unregister3();
		});
	});

	test('the unsafe burner wallet is registered when enableUnsafeBurner is set', async () => {
		const wrapper = createWalletProviderContextWrapper({
			enableUnsafeBurner: true,
		});
		const { result } = renderHook(() => useWallets(), { wrapper });
		const walletNames = result.current.map((wallet) => wallet.name);

		expect(walletNames).toStrictEqual(['Unsafe Burner Wallet']);
	});

	test('unregistered wallets are removed from the list of wallets', async () => {
		const { unregister: unregister1 } = registerMockWallet({
			walletName: 'Mock Wallet 1',
			features: suiFeatures,
		});
		const { unregister: unregister2 } = registerMockWallet({
			walletName: 'Mock Wallet 2',
			features: suiFeatures,
		});
		const { unregister: unregister3 } = registerMockWallet({
			walletName: 'Mock Wallet 3',
			features: suiFeatures,
		});

		const wrapper = createWalletProviderContextWrapper();
		const { result } = renderHook(() => useWallets(), { wrapper });

		act(() => unregister2());

		const walletNames = result.current.map((wallet) => wallet.name);
		expect(walletNames).toStrictEqual(['Mock Wallet 1', 'Mock Wallet 3']);

		act(() => {
			unregister1();
			unregister3();
		});
	});

	test('the list of wallets is correctly filtered by required features', () => {
		const { unregister: unregister1 } = registerMockWallet({
			walletName: 'Mock Wallet 1',
			features: superCoolFeature,
		});
		const { unregister: unregister2 } = registerMockWallet({ walletName: 'Mock Wallet 2' });

		const wrapper = createWalletProviderContextWrapper({
			walletFilter: (wallet) => !!wallet.features['my-dapp:super-cool-feature'],
		});
		const { result } = renderHook(() => useWallets(), { wrapper });
		const walletNames = result.current.map((wallet) => wallet.name);

		expect(walletNames).toStrictEqual(['Mock Wallet 1']);

		act(() => {
			unregister1();
			unregister2();
		});
	});

	test('accounts are properly updated when changed from a wallet', async () => {
		const { unregister, mockWallet } = registerMockWallet({
			walletName: 'Mock Wallet 1',
			accounts: [createMockAccount(), createMockAccount(), createMockAccount()],
		});

		const wrapper = createWalletProviderContextWrapper();
		const { result } = renderHook(
			() => ({
				connectWallet: useConnectWallet(),
				currentAccount: useCurrentAccount(),
				accounts: useAccounts(),
			}),
			{ wrapper },
		);

		result.current.connectWallet.mutate({ wallet: mockWallet });

		await waitFor(() => expect(result.current.connectWallet.isSuccess).toBe(true));

		// Simulate deleting the account we're currently connected to.
		act(() => {
			mockWallet.deleteFirstAccount();
		});

		expect(result.current.currentAccount).toBeTruthy();
		await waitFor(() => {
			expect(result.current.currentAccount!.address).toBe(result.current.accounts[0].address);
		});

		expect(result.current.accounts).toHaveLength(2);

		act(() => unregister());
	});

	describe('wallet auto-connection', () => {
		test('auto-connecting to a wallet works successfully', async () => {
			const { unregister, mockWallet } = registerMockWallet({
				walletName: 'Mock Wallet 1',
				accounts: [createMockAccount(), createMockAccount()],
				features: suiFeatures,
			});

			const wrapper = createWalletProviderContextWrapper({
				autoConnect: true,
			});
			const { result, unmount } = renderHook(() => useConnectWallet(), { wrapper });

			// Manually connect a wallet so we have a wallet to auto-connect to later.
			result.current.mutate({
				wallet: mockWallet,
				accountAddress: mockWallet.accounts[1].address,
			});

			await waitFor(() => expect(result.current.isSuccess).toBe(true));

			// Now unmount our component tree to simulate someone leaving the page.
			unmount();

			// Render our component tree again and auto-connect to our previously connected wallet account.
			const { result: updatedResult } = renderHook(
				() => ({
					currentWallet: useCurrentWallet(),
					currentAccount: useCurrentAccount(),
				}),
				{ wrapper },
			);

			await waitFor(() => expect(updatedResult.current.currentWallet.isConnected).toBe(true));
			expect(updatedResult.current.currentWallet.currentWallet!.name).toStrictEqual(
				'Mock Wallet 1',
			);

			expect(updatedResult.current.currentAccount).toBeTruthy();
			expect(updatedResult.current.currentAccount!.address).toStrictEqual(
				mockWallet.accounts[1].address,
			);

			act(() => unregister());
		});

		test('auto-connecting to an id-based wallet works', async () => {
			const wallet1 = registerMockWallet({
				id: '1',
				walletName: 'Mock Wallet',
				features: suiFeatures,
			});

			const wallet2 = registerMockWallet({
				id: '2',
				walletName: 'Mock Wallet',
				features: suiFeatures,
			});

			const wrapper = createWalletProviderContextWrapper({
				autoConnect: true,
			});
			const { result, unmount } = renderHook(() => useConnectWallet(), { wrapper });

			result.current.mutate({ wallet: wallet1.mockWallet });

			await waitFor(() => expect(result.current.isSuccess).toBe(true));

			// Now unmount our component tree to simulate someone leaving the page.
			unmount();

			// Render our component tree again and auto-connect to our previously connected wallet account.
			const { result: updatedResult } = renderHook(
				() => ({
					currentWallet: useCurrentWallet(),
					currentAccount: useCurrentAccount(),
				}),
				{ wrapper },
			);

			await waitFor(() => expect(updatedResult.current.currentWallet.isConnected).toBe(true));
			expect(updatedResult.current.currentWallet.currentWallet!.id).toStrictEqual('1');
			expect(updatedResult.current.currentAccount).toBeTruthy();

			act(() => {
				wallet1.unregister();
				wallet2.unregister();
			});
		});

		test('wallet connection info is removed upon disconnection', async () => {
			const { unregister, mockWallet } = registerMockWallet({
				walletName: 'Mock Wallet 1',
			});
			const wrapper = createWalletProviderContextWrapper({
				autoConnect: true,
			});

			const { result, unmount } = renderHook(
				() => ({
					connectWallet: useConnectWallet(),
					disconnectWallet: useDisconnectWallet(),
					currentAccount: useCurrentAccount(),
				}),
				{ wrapper },
			);

			result.current.connectWallet.mutate({
				wallet: mockWallet,
			});
			await waitFor(() => expect(result.current.connectWallet.isSuccess).toBe(true));

			// By disconnecting, we should remove any wallet connection info that we have stored.
			result.current.disconnectWallet.mutate();
			await waitFor(() => expect(result.current.disconnectWallet.isSuccess).toBe(true));

			// Now unmount our component tree to simulate someone leaving the page.
			unmount();

			// Render our component tree again and assert that we weren't able to auto-connect.
			const { result: updatedResult } = renderHook(() => useCurrentWallet(), { wrapper });
			expect(updatedResult.current.isConnected).toBeFalsy();

			act(() => unregister());
		});
	});
});
