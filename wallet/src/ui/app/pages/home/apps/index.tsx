// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Content } from '_app/shared/bottom-menu-layout';
import AppsPlayGround, { AppFiltersPortal } from '_components/sui-apps';

import st from './AppsPage.module.scss';

function AppsPage() {
    return (
        <div className={st.container}>
            <Content>
                <section>
                    <AppFiltersPortal />
                    <AppsPlayGround />
                </section>
            </Content>
        </div>
    );
}

export default AppsPage;
