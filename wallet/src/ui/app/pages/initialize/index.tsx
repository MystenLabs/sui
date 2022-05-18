// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Outlet } from 'react-router-dom';

import Loading from '_components/loading';
import Logo from '_components/logo';
import { useFullscreenGuard, useInitializedGuard } from '_hooks';

import st from './InitializePage.module.scss';

const InitializePage = () => {
    const checkingInitialized = useInitializedGuard(false);
    const checkingFullscreen = useFullscreenGuard();
    const guardChecking = checkingFullscreen || checkingInitialized;
    return (
        <Loading loading={guardChecking}>
            <div className={st.container}>
                <div className={st.header}>
                    <Logo size="normal" />
                </div>
                <div className={st.content}>
                    <Outlet />
                </div>
            </div>
        </Loading>
    );
};

export default InitializePage;
