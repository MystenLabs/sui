// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMutation, useQueryClient } from '@tanstack/react-query';

import { autoLockMinutesQueryKey } from './useAutoLockMinutes';
import { useBackgroundClient } from './useBackgroundClient';

export function useAutoLockMinutesMutation() {
	const backgroundClient = useBackgroundClient();
	const queryClient = useQueryClient();
	return useMutation({
		mutationKey: ['set auto-lock minutes mutation'],
		// minutes null disables the auto-lock
		mutationFn: async ({ minutes }: { minutes: number | null }) =>
			backgroundClient.setAutoLockMinutes({ minutes }),
		onSuccess: () => {
			queryClient.invalidateQueries({ exact: true, queryKey: autoLockMinutesQueryKey });
		},
	});
}
