// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useMemo } from 'react';
import { Outlet } from 'react-router-dom';
import { of, filter, switchMap, from, defer, repeat } from 'rxjs';

import { API_ENV_TO_INFO } from '_app/ApiProvider';
import BsIcon from '_components/bs-icon';
import Header from '_components/header';
import Loading from '_components/loading';
import Logo from '_components/logo';
import { useInitializedGuard, useAppDispatch, useAppSelector } from '_hooks';
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
    const selectedApiEnv = useAppSelector(({ app }) => app.apiEnv);
    const netColor = useMemo(
        () =>
            selectedApiEnv
                ? { color: API_ENV_TO_INFO[selectedApiEnv].color }
                : {},
        [selectedApiEnv]
    );
    return (
        <Loading loading={guardChecking}>
            <div className={st.container}>
                <div className={st['outer-container']}>
                    <Logo txt={true} />
                    {selectedApiEnv ? (
                        <div className={st.network} style={netColor}>
                            <BsIcon
                                icon="circle-fill"
                                className={st['network-icon']}
                            />
                            <span className={st['network-name']}>
                                {API_ENV_TO_INFO[selectedApiEnv].name}
                            </span>
                        </div>
                    ) : null}
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
