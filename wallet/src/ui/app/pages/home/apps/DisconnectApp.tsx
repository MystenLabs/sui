// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Content } from '_app/shared/bottom-menu-layout';
import AppsPlayGround from '_components/sui-apps';

import st from './AppsPage.module.scss';

function DisconnectAppPage() {
    return (
        <div className={st.container}>
            <Content>
                <section>
                    <AppsPlayGround />
                </section>
            </Content>
        </div>
    );
}

export default DisconnectAppPage;
