// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMutation } from '@tanstack/react-query';

import { type BackgroundClient } from '../background-client';
import { useBackgroundClient } from './useBackgroundClient';

export function useResetPasswordMutation() {
	const backgroundClient = useBackgroundClient();
	return useMutation({
		mutationKey: ['reset wallet password'],
		mutationFn: async (...args: Parameters<BackgroundClient['resetPassword']>) => {
			return await backgroundClient.resetPassword(...args);
		},
	});
}
