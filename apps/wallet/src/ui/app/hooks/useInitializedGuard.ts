// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect } from 'react';
import { useNavigate } from 'react-router-dom';

import { useAccounts } from './useAccounts';
import { useRestrictedGuard } from './useRestrictedGuard';

export default function useInitializedGuard(initializedRequired: boolean) {
	const restricted = useRestrictedGuard();
	const { data: allAccounts, isLoading } = useAccounts();
	const isInitialized = !!allAccounts?.length;
	const navigate = useNavigate();
	const guardAct = !restricted && !isLoading && initializedRequired !== isInitialized;
	useEffect(() => {
		if (guardAct) {
			navigate(isInitialized ? '/' : '/welcome', { replace: true });
		}
	}, [guardAct, isInitialized, navigate]);
	return isLoading || guardAct;
}
