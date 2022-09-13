// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { Content } from '_app/shared/bottom-menu-layout';
import AppsPlayGround from '_components/apps-playground';

import st from './AppsPage.module.scss';

function AppsPage() {
    return (
        <div className={st.container}>
            <Content>
                <h4 className={st.activeSectionTitle}>Playground</h4>

                <section className={st.nftGalleryContainer}>
                    <AppsPlayGround />
                </section>
            </Content>
        </div>
    );
}

export default AppsPage;
