// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Content } from '_app/shared/bottom-menu-layout';
import { ConnectedAppsCard, AppFiltersPortal } from '_components/sui-apps';

import st from './AppsPage.module.scss';

function SuiConnectedAppsPage() {
    return (
        <div className={st.container}>
            <Content>
                <section>
                    <AppFiltersPortal />
                    <ConnectedAppsCard />
                </section>
            </Content>
        </div>
    );
}

export default SuiConnectedAppsPage;
