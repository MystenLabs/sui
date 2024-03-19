// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { act, renderHook, waitFor } from '@testing-library/react';
import type { Mock } from 'vitest';

import {
	WalletFeatureNotSupportedError,
	WalletNotConnectedError,
} from '../../src/errors/walletErrors.js';
import { useConnectWallet, useSignPersonalMessage } from '../../src/index.js';
import { signMessageFeature, suiFeatures } from '../mocks/mockFeatures.js';
import { createWalletProviderContextWrapper, registerMockWallet } from '../test-utils.js';

describe('useSignPersonalMessage', () => {
	test('throws an error when trying to sign a message without a wallet connection', async () => {
		const wrapper = createWalletProviderContextWrapper();
		const { result } = renderHook(() => useSignPersonalMessage(), { wrapper });

		result.current.mutate({ message: new Uint8Array() });

		await waitFor(() => expect(result.current.error).toBeInstanceOf(WalletNotConnectedError));
	});

	test('throws an error when trying to sign a message with a wallet that lacks message signing feature support', async () => {
		const { unregister, mockWallet } = registerMockWallet({
			walletName: 'Mock Wallet 1',
		});

		const wrapper = createWalletProviderContextWrapper();
		const { result } = renderHook(
			() => ({
				connectWallet: useConnectWallet(),
				signPersonalMessage: useSignPersonalMessage(),
			}),
			{ wrapper },
		);

		result.current.connectWallet.mutate({ wallet: mockWallet });
		await waitFor(() => expect(result.current.connectWallet.isSuccess).toBe(true));

		result.current.signPersonalMessage.mutate({ message: new Uint8Array() });
		await waitFor(() =>
			expect(result.current.signPersonalMessage.error).toBeInstanceOf(
				WalletFeatureNotSupportedError,
			),
		);

		act(() => unregister());
	});

	test('falls back to the `sui:signMessage` feature with a wallet that lacks support for `sui:signPersonalMessage`.', async () => {
		const { unregister, mockWallet } = registerMockWallet({
			walletName: 'Mock Wallet 1',
			features: signMessageFeature,
		});

		const wrapper = createWalletProviderContextWrapper();
		const { result } = renderHook(
			() => ({
				connectWallet: useConnectWallet(),
				signPersonalMessage: useSignPersonalMessage(),
			}),
			{ wrapper },
		);

		result.current.connectWallet.mutate({ wallet: mockWallet });
		await waitFor(() => expect(result.current.connectWallet.isSuccess).toBe(true));

		const mockSignMessageFeature = mockWallet.features['sui:signMessage'];
		const signMessageMock = mockSignMessageFeature!.signMessage as Mock;

		signMessageMock.mockReturnValueOnce({ messageBytes: 'abc', signature: '123' });

		result.current.signPersonalMessage.mutate({
			message: new Uint8Array().fill(123),
		});

		await waitFor(() => expect(result.current.signPersonalMessage.isSuccess).toBe(true));
		expect(result.current.signPersonalMessage.data).toStrictEqual({
			bytes: 'abc',
			signature: '123',
		});

		act(() => unregister());
	});

	test('signing a personal message from the currently connected account works successfully', async () => {
		const { unregister, mockWallet } = registerMockWallet({
			walletName: 'Mock Wallet 1',
			features: suiFeatures,
		});

		const wrapper = createWalletProviderContextWrapper();
		const { result } = renderHook(
			() => ({
				connectWallet: useConnectWallet(),
				signPersonalMessage: useSignPersonalMessage(),
			}),
			{ wrapper },
		);

		result.current.connectWallet.mutate({ wallet: mockWallet });

		await waitFor(() => expect(result.current.connectWallet.isSuccess).toBe(true));

		const signPersonalMessageFeature = mockWallet.features['sui:signPersonalMessage'];
		const signPersonalMessageMock = signPersonalMessageFeature!.signPersonalMessage as Mock;

		signPersonalMessageMock.mockReturnValueOnce({ bytes: 'abc', signature: '123' });

		result.current.signPersonalMessage.mutate({
			message: new Uint8Array().fill(123),
		});

		await waitFor(() => expect(result.current.signPersonalMessage.isSuccess).toBe(true));
		expect(result.current.signPersonalMessage.data).toStrictEqual({
			bytes: 'abc',
			signature: '123',
		});

		act(() => unregister());
	});
});
