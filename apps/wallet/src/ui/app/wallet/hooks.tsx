// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect } from 'react';
import { useLocation, useNavigate, useSearchParams } from 'react-router-dom';

import { useAccounts } from '../hooks/useAccounts';
import { useActiveAccount } from '../hooks/useActiveAccount';

export function useLockedGuard(requiredLockedStatus: boolean) {
	const navigate = useNavigate();
	const [searchParams] = useSearchParams();
	const { pathname, search, state } = useLocation();
	const { data: allAccounts, isLoading: isAccountsLoading } = useAccounts();
	const activeAccount = useActiveAccount();
	const loading = isAccountsLoading || !activeAccount;
	const isInitialized = !!allAccounts?.length;
	const isLocked = activeAccount?.isLocked || false;
	const guardAct = !loading && isInitialized && requiredLockedStatus !== isLocked;
	const nextUrl = searchParams.get('url') || '/';
	useEffect(() => {
		if (guardAct) {
			navigate(
				requiredLockedStatus ? nextUrl : `/tokens?url=${encodeURIComponent(pathname + search)}`,
				{ replace: true, state },
			);
		}
	}, [guardAct, navigate, requiredLockedStatus, pathname, search, state, nextUrl]);

	return loading || guardAct;
}
