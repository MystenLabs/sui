// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMemo } from 'react';

import useAppSelector from './useAppSelector';
import { accountsAdapterSelectors } from '../redux/slices/account';

export function useAccounts(addressesFilters?: string[]) {
	const accounts = useAppSelector(accountsAdapterSelectors.selectAll);
	return useMemo(() => {
		if (!addressesFilters?.length) {
			return accounts;
		}
		return accounts.filter((anAccount) => addressesFilters.includes(anAccount.address));
	}, [accounts, addressesFilters]);
}
