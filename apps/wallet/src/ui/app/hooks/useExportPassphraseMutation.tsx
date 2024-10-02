// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type MethodPayload } from '_src/shared/messaging/messages/payloads/MethodPayload';
import { entropyToMnemonic, toEntropy } from '_src/shared/utils/bip39';
import { useMutation } from '@tanstack/react-query';

import { useBackgroundClient } from './useBackgroundClient';

export function useExportPassphraseMutation() {
	const backgroundClient = useBackgroundClient();
	return useMutation({
		mutationKey: ['export passphrase'],
		mutationFn: async (args: MethodPayload<'getAccountSourceEntropy'>['args']) =>
			entropyToMnemonic(
				toEntropy((await backgroundClient.getAccountSourceEntropy(args)).entropy),
			).split(' '),
	});
}
