// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMutation } from '@tanstack/react-query';
import { useBackgroundClient } from './useBackgroundClient';

export function useCreateAccountsMutation() {
	const backgroundService = useBackgroundClient();
	return useMutation({
		mutationKey: ['create accounts'],
		mutationFn: (...params: Parameters<typeof backgroundService.createAccounts>) =>
			backgroundService.createAccounts(...params),
	});
}
