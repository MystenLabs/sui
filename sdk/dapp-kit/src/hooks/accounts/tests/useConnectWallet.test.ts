// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { renderHook } from '@testing-library/react';

describe('useConnectWallet', () => {
	test('throws without a SuiClientContext', () => {
		expect(() => renderHook(() => useSuiClient())).toThrowError(
			'Could not find SuiClientContext. Ensure that you have set up the SuiClientProvider',
		);
	});

	test('returns a SuiClient', () => {
		const suiClient = new SuiClient({ url: getFullnodeUrl('localnet') });
		const wrapper = createSuiClientContextWrapper(suiClient);
		const { result } = renderHook(() => useSuiClient(), { wrapper });

		expect(result.current).toBe(suiClient);
	});
});
