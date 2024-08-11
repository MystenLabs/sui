// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { getFullnodeUrl, SuiClient } from '@mysten/sui/client';
import { act, renderHook, waitFor } from '@testing-library/react';

import { useSuiClientMutation } from '../../src/hooks/useSuiClientMutation.js';
import { createWalletProviderContextWrapper } from '../test-utils.js';

describe('useSuiClientMutation', () => {
	it('should fetch data', async () => {
		const suiClient = new SuiClient({ url: getFullnodeUrl('mainnet') });
		const wrapper = createWalletProviderContextWrapper({}, suiClient);

		const queryTransactionBlocks = vi.spyOn(suiClient, 'queryTransactionBlocks');

		queryTransactionBlocks.mockResolvedValueOnce({
			data: [{ digest: '0x123' }],
			hasNextPage: true,
			nextCursor: 'page2',
		});

		const { result } = renderHook(() => useSuiClientMutation('queryTransactionBlocks'), {
			wrapper,
		});

		act(() => {
			result.current.mutate({
				filter: {
					FromAddress: '0x123',
				},
			});
		});

		await waitFor(() => expect(result.current.status).toBe('success'));

		expect(queryTransactionBlocks).toHaveBeenCalledWith({
			filter: {
				FromAddress: '0x123',
			},
		});
		expect(result.current.isPending).toBe(false);
		expect(result.current.isError).toBe(false);
		expect(result.current.data).toEqual({
			data: [{ digest: '0x123' }],
			hasNextPage: true,
			nextCursor: 'page2',
		});
	});
});
