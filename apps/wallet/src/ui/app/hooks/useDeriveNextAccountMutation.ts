// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMutation } from '@tanstack/react-query';
import { toast } from 'react-hot-toast';

import { useBackgroundClient } from './useBackgroundClient';

export function useDeriveNextAccountMutation() {
	const backgroundClient = useBackgroundClient();
	return useMutation({
		mutationFn: () => {
			return backgroundClient.deriveNextAccount();
		},
		onSuccess: () => {
			toast.success('New account created');
		},
		onError: (e) => {
			toast.error((e as Error).message || 'Failed to create new account');
		},
	});
}
