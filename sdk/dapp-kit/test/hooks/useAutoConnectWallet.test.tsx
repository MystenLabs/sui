// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { act, renderHook, waitFor } from '@testing-library/react';

import { useAutoConnectWallet } from '../../src/hooks/wallet/useAutoConnectWallet.js';
import { useConnectWallet } from '../../src/index.js';
import { createMockAccount } from '../mocks/mockAccount.js';
import { suiFeatures } from '../mocks/mockFeatures.js';
import { createWalletProviderContextWrapper, registerMockWallet } from '../test-utils.js';

function withResolvers<T = any>() {
	let resolve, reject;
	const promise = new Promise<T>((res, rej) => {
		resolve = res;
		reject = rej;
	});
	return { promise, reject: reject!, resolve: resolve! };
}

describe('useAutoConnectWallet', () => {
	test('returns "disabled" when the auto-connect functionality is disabled', async () => {
		const wrapper = createWalletProviderContextWrapper();
		const { result } = renderHook(() => useAutoConnectWallet(), { wrapper });
		expect(result.current).toBe('disabled');
	});

	test(`returns "attempted" immediately when there is no last connected wallet`, async () => {
		const wrapper = createWalletProviderContextWrapper({ autoConnect: true });
		const { result } = renderHook(() => useAutoConnectWallet(), { wrapper });
		expect(result.current).toBe('attempted');
	});

	test('returns "attempted" when we have made a successful auto-connection attempt', async () => {
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

		const { promise, resolve } = withResolvers();
		mockWallet.mocks.connect.mockImplementation(async () => {
			console.log('Waiting to connect');
			return promise;
		});

		// Render our component tree again and auto-connect to our previously connected wallet account.
		const { result: updatedResult } = renderHook(() => useAutoConnectWallet(), { wrapper });
		expect(updatedResult.current).toBe('idle');
		resolve({ accounts: mockWallet.accounts });

		await waitFor(() => expect(updatedResult.current).toBe('attempted'));

		act(() => unregister());
	});
});
