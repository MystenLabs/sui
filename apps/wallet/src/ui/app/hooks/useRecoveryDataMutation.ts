// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMutation } from '@tanstack/react-query';
import { useBackgroundClient } from './useBackgroundClient';
import { useForgotPasswordContext } from '../pages/accounts/forgot-password/ForgotPasswordPage';
import { type PasswordRecoveryData } from '_src/shared/messaging/messages/payloads/MethodPayload';

export function useRecoveryDataMutation() {
	const backgroundClient = useBackgroundClient();
	const { add } = useForgotPasswordContext();
	return useMutation({
		mutationKey: ['add recovery data'],
		mutationFn: async (data: PasswordRecoveryData) => {
			await backgroundClient.verifyPasswordRecoveryData({ data });
			add(data);
		},
	});
}
