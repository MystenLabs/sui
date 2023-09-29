// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type AccountType } from '_src/background/accounts/Account';
import { type ZkProvider } from '_src/background/accounts/zk/providers';
import { isZkAccountSerializedUI } from '_src/background/accounts/zk/ZkAccount';
import { useMemo } from 'react';

import { useAccounts } from './useAccounts';

export function useCountAccountsByType() {
	const { data: accounts, isLoading } = useAccounts();
	const countPerType = useMemo(
		() =>
			accounts?.reduce<
				Partial<Record<AccountType, { total: number; extra?: Record<ZkProvider, number> }>>
			>((acc, anAccount) => {
				acc[anAccount.type] = acc[anAccount.type] || { total: 0 };
				acc[anAccount.type]!.total++;
				if (isZkAccountSerializedUI(anAccount)) {
					acc[anAccount.type]!.extra =
						acc[anAccount.type]!.extra || ({} as Record<ZkProvider, number>);
					acc[anAccount.type]!.extra![anAccount.provider] =
						acc[anAccount.type]!.extra![anAccount.provider] || 0;
					acc[anAccount.type]!.extra![anAccount.provider]++;
				}
				return acc;
			}, {}) || {},
		[accounts],
	);
	return { data: countPerType, isLoading };
}
