// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Route, Routes } from 'react-router-dom';

import { Content } from '_app/shared/bottom-menu-layout';
import AppsPlayGround, {
    ConnectedAppsCard,
    AppFiltersPortal,
} from '_components/sui-apps';

import st from './AppsPage.module.scss';

function AppsPage() {
    return (
        <div className={st.container}>
            <Content>
                <section>
                    <AppFiltersPortal />
                    <Routes>
                        <Route path="/" element={<AppsPlayGround />} />
                        <Route
                            path="/connected"
                            element={<ConnectedAppsCard />}
                        />
                    </Routes>
                </section>
            </Content>
        </div>
    );
}

export default AppsPage;
