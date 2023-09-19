// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMutation } from '@tanstack/react-query';
import { useBackgroundClient } from './useBackgroundClient';
import { type BackgroundClient } from '../background-client';

export function useResetPasswordMutation() {
	const backgroundClient = useBackgroundClient();
	return useMutation({
		mutationKey: ['reset wallet password'],
		mutationFn: async (...args: Parameters<BackgroundClient['resetPassword']>) => {
			return await backgroundClient.resetPassword(...args);
		},
	});
}
