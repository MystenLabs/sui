// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMutation } from '@tanstack/react-query';

import { type BackgroundClient } from '../background-client';
import { useBackgroundClient } from './useBackgroundClient';

export function useUnlockMutation() {
	const backgroundClient = useBackgroundClient();
	return useMutation({
		mutationKey: ['accounts', 'unlock', 'account source or account'],
		mutationFn: async (inputs: Parameters<BackgroundClient['unlockAccountSourceOrAccount']>['0']) =>
			backgroundClient.unlockAccountSourceOrAccount(inputs),
	});
}
