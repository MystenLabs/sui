// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { renderHook } from '@testing-library/react';
import { useWallet } from 'dapp-kit/src';

describe('useWallet', () => {
	test('throws an error when rendered without a provider', () => {
		expect(() => renderHook(() => useWallet())).toThrowError(
			'Could not find WalletContext. Ensure that you have set up the WalletProvider.',
		);
	});
});
