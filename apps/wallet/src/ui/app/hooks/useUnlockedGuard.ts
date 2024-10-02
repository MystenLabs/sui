// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect } from 'react';
import { useNavigate } from 'react-router-dom';

import { useAccounts } from '../hooks/useAccounts';
import { useActiveAccount } from '../hooks/useActiveAccount';

export function useUnlockedGuard() {
	const navigate = useNavigate();
	const { data: allAccounts, isPending: isAccountsLoading } = useAccounts();
	const activeAccount = useActiveAccount();
	const loading = isAccountsLoading || !activeAccount;
	const isInitialized = !!allAccounts?.length;
	const isLocked = !!activeAccount?.isLocked;
	const guardAct = !loading && isInitialized && isLocked;
	useEffect(() => {
		if (guardAct) {
			navigate(`/tokens`, { replace: true });
		}
	}, [guardAct, navigate]);

	return loading || guardAct;
}
