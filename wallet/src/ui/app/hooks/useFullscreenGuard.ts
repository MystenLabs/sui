// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect } from 'react';

import useAppSelector from './useAppSelector';
import { AppType } from '_redux/slices/app/AppType';
import { openInNewTab } from '_shared/utils';

export default function useFullscreenGuard() {
    const appType = useAppSelector((state) => state.app.appType);
    useEffect(() => {
        if (appType === AppType.popup) {
            openInNewTab().finally(() => window.close());
        }
    }, [appType]);
    return appType === AppType.unknown;
}
