// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMutation } from '@tanstack/react-query';
import { useBackgroundClient } from './useBackgroundClient';

export type DeleteType = 'mnemonic';

export function useDeleteAccountSourceMutation() {
	const backgroundClient = useBackgroundClient();
	return useMutation({
		mutationKey: ['delete account source'],
		mutationFn: async ({ type }: { type: DeleteType }) => {
			return await backgroundClient.deleteAccountSourceByType({ type });
		},
	});
}
