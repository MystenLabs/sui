// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useMemo } from 'react';
import { useNavigate } from 'react-router-dom';

import useAppSelector from './useAppSelector';

export default function useInitializedGuard(initializedRequired: boolean) {
    const loading = useAppSelector((state) => state.account.loading);
    const isInitialized = useAppSelector((state) => !!state.account.mnemonic);
    const navigate = useNavigate();
    const guardAct = useMemo(
        () => !loading && initializedRequired !== isInitialized,
        [loading, initializedRequired, isInitialized]
    );
    useEffect(() => {
        if (guardAct) {
            navigate(isInitialized ? '/' : '/welcome', { replace: true });
        }
    }, [guardAct, isInitialized, navigate]);
    return loading || guardAct;
}
