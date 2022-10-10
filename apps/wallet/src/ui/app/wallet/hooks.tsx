// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';

import { WALLET_ENCRYPTION_ENABLED } from './constants';
import { useAppSelector } from '_hooks';

export function useLockedGuard(requiredLockedStatus: boolean) {
    const navigate = useNavigate();
    const { pathname, search, state } = useLocation();
    const { isInitialized, isLocked } = useAppSelector(
        ({ account: { isInitialized, isLocked } }) => ({
            isInitialized,
            isLocked,
        })
    );
    const loading = isInitialized === null || isLocked === null;
    const guardAct =
        WALLET_ENCRYPTION_ENABLED &&
        !loading &&
        isInitialized &&
        requiredLockedStatus !== isLocked;
    useEffect(() => {
        if (guardAct) {
            navigate(
                requiredLockedStatus
                    ? '/'
                    : `/locked?url=${encodeURIComponent(pathname + search)}`,
                { replace: true, state }
            );
        }
    }, [guardAct, navigate, requiredLockedStatus, pathname, search, state]);
    return loading || guardAct;
}
