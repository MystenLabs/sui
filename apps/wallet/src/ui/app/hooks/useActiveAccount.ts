// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMemo } from 'react';

import { useAccounts } from './useAccounts';

export function useActiveAccount() {
	const { data: allAccounts } = useAccounts();
	return useMemo(() => {
		if (!allAccounts) {
			return null;
		}
		const selected = allAccounts.find(({ selected }) => selected);
		if (selected) {
			return selected;
		}
		return allAccounts[0] || null;
	}, [allAccounts]);
}
