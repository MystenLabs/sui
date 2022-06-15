// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect } from 'react';
import { Outlet } from 'react-router-dom';
import { of, filter, switchMap, from, defer, repeat } from 'rxjs';

import Header from '_components/header';
import Loading from '_components/loading';
import Logo from '_components/logo';
import NetworkSwitch from '_components/network-switch';
import { useInitializedGuard, useAppDispatch } from '_hooks';
import { fetchAllOwnedObjects } from '_redux/slices/sui-objects';

import st from './Home.module.scss';

const POLL_SUI_OBJECTS_INTERVAL = 4000;

const HomePage = () => {
    const guardChecking = useInitializedGuard(true);
    const dispatch = useAppDispatch();
    useEffect(() => {
        const sub = of(guardChecking)
            .pipe(
                filter(() => !guardChecking),
                switchMap(() =>
                    defer(() => from(dispatch(fetchAllOwnedObjects()))).pipe(
                        repeat({ delay: POLL_SUI_OBJECTS_INTERVAL })
                    )
                )
            )
            .subscribe();
        return () => sub.unsubscribe();
    }, [guardChecking, dispatch]);

    return (
        <Loading loading={guardChecking}>
            <div className={st.container}>
                <div className={st['outer-container']}>
                    <Logo txt={true} />
                    <NetworkSwitch />
                </div>
                <div className={st['inner-container']}>
                    <Header />
                    <Outlet />
                </div>
            </div>
        </Loading>
    );
};

export default HomePage;
