// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { act, renderHook, waitFor } from '@testing-library/react';

import { useAutoConnectionStatus } from '../../src/hooks/wallet/useAutoConnectionStatus.js';
import { useConnectWallet } from '../../src/index.js';
import { createMockAccount } from '../mocks/mockAccount.js';
import { suiFeatures } from '../mocks/mockFeatures.js';
import { createWalletProviderContextWrapper, registerMockWallet } from '../test-utils.js';

describe('useAutoConnectStatus', () => {
	test('returns "disabled" when the auto-connect functionality is disabled', async () => {
		const wrapper = createWalletProviderContextWrapper();
		const { result } = renderHook(() => useAutoConnectionStatus(), { wrapper });
		expect(result.current).toBe('disabled');
	});

	test(`returns "idle" when we haven't yet made an auto-connection attempt`, async () => {
		const wrapper = createWalletProviderContextWrapper({ autoConnect: true });
		const { result } = renderHook(() => useAutoConnectionStatus(), { wrapper });
		expect(result.current).toBe('idle');
	});

	test('returns "settled" when we have made a successful auto-connection attempt', async () => {
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
		const { result: updatedResult } = renderHook(() => useAutoConnectionStatus(), { wrapper });

		await waitFor(() => expect(updatedResult.current).toBe('settled'));

		act(() => unregister());
	});
});
