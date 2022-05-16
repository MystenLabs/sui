// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect } from 'react';
import { useNavigate } from 'react-router-dom';

import useAppSelector from './useAppSelector';

export default function useInitializedGuard(initializedRequired: boolean) {
    const loading = useAppSelector((state) => state.account.loading);
    const isInitialized = useAppSelector((state) => !!state.account.mnemonic);
    const navigate = useNavigate();
    useEffect(() => {
        if (!loading && initializedRequired !== isInitialized) {
            navigate(isInitialized ? '/' : '/welcome', { replace: true });
        }
    }, [loading, initializedRequired, isInitialized, navigate]);
    return loading;
}
