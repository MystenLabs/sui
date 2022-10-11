// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import PageMainLayout from '_app/shared/page-main-layout';
import { useLockedGuard } from '_app/wallet/hooks';
import Loading from '_components/loading';
import { useInitializedGuard } from '_hooks';
import ImportPage from '_pages/initialize/import';
import PageLayout from '_pages/layout';

import st from './ForgotPasswordPage.module.scss';

export default function ForgotPasswordPage() {
    const initGuardLoading = useInitializedGuard(true);
    const lockedGuardLoading = useLockedGuard(true);
    const guardsLoading = initGuardLoading || lockedGuardLoading;
    return (
        <Loading loading={guardsLoading}>
            <PageLayout limitToPopUpSize={true}>
                <PageMainLayout className={st.main}>
                    <ImportPage mode="forgot" />
                </PageMainLayout>
            </PageLayout>
        </Loading>
    );
}
