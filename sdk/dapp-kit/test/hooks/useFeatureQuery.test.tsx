// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { act, renderHook, waitFor } from '@testing-library/react';

import { useConnectWallet, useCurrentWallet, useDisconnectWallet, useFeatureQuery } from '../../src/index.js';
import { createWalletProviderContextWrapper, registerMockWallet } from '../test-utils.js';

describe('useFeatureQuery', () => {
	test('queries the current wallet successfully', async () => {
		const feature = vi.fn().mockImplementation(() => 'response');

		const { unregister, mockWallet } = registerMockWallet({
			walletName: 'Mock Wallet 1',
			features: {
				'test:feature': { feature },
			},
		});

		const wrapper = createWalletProviderContextWrapper();
		const { result } = renderHook(
			() => ({
				wallet: useCurrentWallet(),
				connectWallet: useConnectWallet(),
				disconnectWallet: useDisconnectWallet(),
				// @ts-expect-error: The feature is untyped:
				feature: useFeatureQuery('test:feature', ['foo', 'bar']),
			}),
			{ wrapper },
		);

		result.current.connectWallet.mutate({ wallet: mockWallet });

		await waitFor(() => expect(result.current.connectWallet.isSuccess).toBe(true));
		await waitFor(() => expect(result.current.feature.isSuccess).toBe(true));
		expect(result.current.feature.data).toBe('response');
		expect(feature).toBeCalledTimes(1);
		expect(feature).toBeCalledWith('foo', 'bar');

		mockWallet.emit();

		// The feature should be called again after the wallet emits a change event:
		await waitFor(() => expect(feature).toBeCalledTimes(2));

		// After disconnecting, the state should reset:
		result.current.disconnectWallet.mutate();
		await waitFor(() => expect(result.current.wallet.isConnected).toBe(false));
		expect(result.current.feature.isFetched).toBe(false);
		expect(result.current.feature.data).toBeUndefined();

		act(() => {
			unregister();
		});
	});
});
