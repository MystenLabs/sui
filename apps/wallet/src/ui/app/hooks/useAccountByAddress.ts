// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMemo } from 'react';

import { useAccounts } from './useAccounts';

export function useAccountByAddress(accountAddress?: string | null) {
	const allAccountsData = useAccounts();
	const account = useMemo(
		() =>
			(accountAddress && allAccountsData.data?.find(({ address }) => address === accountAddress)) ||
			null,
		[allAccountsData.data, accountAddress],
	);
	return { ...allAccountsData, data: account };
}
