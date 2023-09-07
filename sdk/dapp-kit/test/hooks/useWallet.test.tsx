// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { renderHook } from '@testing-library/react';
import { useWallet } from 'dapp-kit/src';
import { createWalletProviderContextWrapper, registerMockWallet } from '../test-utils.js';

describe('useWallet', () => {
	test('that an error is thrown when rendered without a provider', () => {
		expect(() => renderHook(() => useWallet())).toThrowError(
			'Could not find WalletContext. Ensure that you have set up the WalletProvider.',
		);
	});

	test('that the correct wallet and account information is returned on initial render', () => {
		const wrapper = createWalletProviderContextWrapper();
		const { result } = renderHook(() => useWallet(), { wrapper });

		expect(result.current).toStrictEqual({
			accounts: [],
			currentAccount: null,
			wallets: [],
			currentWallet: null,
			connectionStatus: 'disconnected',
		});
	});

	test('that the list of wallets is ordered correctly by preference', () => {
		const unregister1 = registerMockWallet('Mock Wallet 1');
		const unregister2 = registerMockWallet('Mock Wallet 2');
		const unregister3 = registerMockWallet('Mock Wallet 3');

		const wrapper = createWalletProviderContextWrapper({
			preferredWallets: ['Mock Wallet 2', 'Mock Wallet 1'],
		});
		const { result } = renderHook(() => useWallet(), { wrapper });
		const walletNames = result.current.wallets.map((wallet) => wallet.name);

		expect(walletNames).toStrictEqual(['Mock Wallet 2', 'Mock Wallet 1', 'Mock Wallet 3']);

		unregister1();
		unregister2();
		unregister3();
	});

	test('that the list of wallets is correctly filtered by required features', () => {
		const unregister1 = registerMockWallet('Mock Wallet 1', {
			'my-dapp:super-cool-feature': {
				version: '1.0.0',
				superCoolFeature: () => {},
			},
		});
		const unregister2 = registerMockWallet('Mock Wallet 2');

		const wrapper = createWalletProviderContextWrapper({
			requiredFeatures: ['my-dapp:super-cool-feature'],
		});
		const { result } = renderHook(() => useWallet(), { wrapper });
		const walletNames = result.current.wallets.map((wallet) => wallet.name);

		expect(walletNames).toStrictEqual(['Mock Wallet 1']);

		unregister1();
		unregister2();
	});
});
