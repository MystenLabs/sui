// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMutation } from '@tanstack/react-query';
import { useBackgroundClient } from './useBackgroundClient';
import { toast } from 'react-hot-toast';

export function useCreateAccountsMutation() {
	const backgroundService = useBackgroundClient();
	return useMutation({
		mutationKey: ['create accounts'],
		mutationFn: (...params: Parameters<typeof backgroundService.createAccounts>) =>
			backgroundService.createAccounts(...params),
		onError: (error) => {
			toast.error((error as Error)?.message || 'Failed to create account. (Unknown error)');
		},
		onSuccess: (result) => {
			toast.success(`Account${result.length === 1 ? '' : 's'} created`);
		},
	});
}
