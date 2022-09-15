// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect, memo } from 'react';

import { useAppDispatch } from '_hooks';
import { setNavFilterTag } from '_redux/slices/app';

function AppFilters() {
    const dispatch = useAppDispatch();
    useEffect(() => {
        setTimeout(() => {
            dispatch(
                setNavFilterTag([
                    {
                        name: 'Playground',
                        link: 'apps',
                    },
                    {
                        name: 'Active Connections',
                        link: 'apps/connected',
                    },
                ])
            );
        }, 30);
    }, [dispatch]);

    return null;
}

export default memo(AppFilters);
