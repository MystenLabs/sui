// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type AccountType } from '_src/background/accounts/Account';
import { type ZkLoginProvider } from '_src/background/accounts/zklogin/providers';
import { isZkLoginAccountSerializedUI } from '_src/background/accounts/zklogin/ZkLoginAccount';
import { useMemo } from 'react';

import { useAccounts } from './useAccounts';

export function useCountAccountsByType() {
	const { data: accounts, isPending } = useAccounts();
	const countPerType = useMemo(
		() =>
			accounts?.reduce<
				Partial<Record<AccountType, { total: number; extra?: Record<ZkLoginProvider, number> }>>
			>((acc, anAccount) => {
				acc[anAccount.type] = acc[anAccount.type] || { total: 0 };
				acc[anAccount.type]!.total++;
				if (isZkLoginAccountSerializedUI(anAccount)) {
					acc[anAccount.type]!.extra =
						acc[anAccount.type]!.extra || ({} as Record<ZkLoginProvider, number>);
					acc[anAccount.type]!.extra![anAccount.provider] =
						acc[anAccount.type]!.extra![anAccount.provider] || 0;
					acc[anAccount.type]!.extra![anAccount.provider]++;
				}
				return acc;
			}, {}) || {},
		[accounts],
	);
	return { data: countPerType, isPending };
}
