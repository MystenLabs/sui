// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Outlet } from 'react-router-dom';

import Loading from '_components/loading';
import { useInitializedGuard } from '_hooks';
import PageLayout from '_pages/layout';

import st from './InitializePage.module.scss';

const InitializePage = () => {
    const checkingInitialized = useInitializedGuard(false);
    return (
        <PageLayout forceFullscreen={true}>
            <Loading loading={checkingInitialized}>
                <div className={st.container}>
                    <div className={st.content}>
                        <Outlet />
                    </div>
                </div>
            </Loading>
        </PageLayout>
    );
};

export default InitializePage;
