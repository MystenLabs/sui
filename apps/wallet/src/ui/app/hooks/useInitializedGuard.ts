// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect } from 'react';
import { useNavigate } from 'react-router-dom';

import { useAccounts } from './useAccounts';
import { useRestrictedGuard } from './useRestrictedGuard';

export default function useInitializedGuard(initializedRequired: boolean, enabled = true) {
	const restricted = useRestrictedGuard();
	const { data: allAccounts, isPending } = useAccounts();
	const isInitialized = !!allAccounts?.length;
	const navigate = useNavigate();
	const guardAct = !restricted && !isPending && initializedRequired !== isInitialized && enabled;
	useEffect(() => {
		if (guardAct) {
			navigate(isInitialized ? '/' : '/accounts/welcome', { replace: true });
		}
	}, [guardAct, isInitialized, navigate]);
	return isPending || guardAct;
}
