// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { MutationKey } from '@tanstack/react-query';

export const walletMutationKeys = {
	all: { baseScope: 'wallet' },
	connectWallet: formMutationKeyFn('connect-wallet'),
	autoconnectWallet: formMutationKeyFn('autoconnect-wallet'),
	disconnectWallet: formMutationKeyFn('disconnect-wallet'),
	signPersonalMessage: formMutationKeyFn('sign-personal-message'),
	signTransaction: formMutationKeyFn('sign-transaction'),
	signAndExecuteTransaction: formMutationKeyFn('sign-and-execute-transaction'),
	switchAccount: formMutationKeyFn('switch-account'),
	reportTransactionEffects: formMutationKeyFn('report-transaction-effects'),
};

function formMutationKeyFn(baseEntity: string) {
	return function mutationKeyFn(additionalKeys: MutationKey = []) {
		return [{ ...walletMutationKeys.all, baseEntity }, ...additionalKeys];
	};
}
