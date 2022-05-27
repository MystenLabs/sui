// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { memo } from 'react';

import AccountAddress from '_components/account-address';
import Alert from '_components/alert';
import BsIcon from '_components/bs-icon';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import { useAppSelector } from '_hooks';

import type { ReactNode } from 'react';

import st from './ObjectsLayout.module.scss';

export type ObjectsLayoutProps = {
    children: ReactNode;
    emptyMsg: string;
    totalItems: number;
};

function ObjectsLayout({ children, emptyMsg, totalItems }: ObjectsLayoutProps) {
    const objectsLoading = useAppSelector(
        ({ suiObjects }) => suiObjects.loading
    );
    const objectsLastSync = useAppSelector(
        ({ suiObjects }) => suiObjects.lastSync
    );
    const objectsError = useAppSelector(({ suiObjects }) => suiObjects.error);
    const showError =
        !!objectsError &&
        (!objectsLastSync || Date.now() - objectsLastSync > 30 * 1000);
    const showEmptyNotice = !!(objectsLastSync && !totalItems);
    const showItems = !!(objectsLastSync && totalItems);
    const showLoading = objectsLoading && !objectsLastSync;
    return (
        <div className={st.container}>
            <div>
                <span className={st.title}>Active Account:</span>
                <AccountAddress />
            </div>
            <div className={st.items}>
                {showError ? (
                    <Alert className={st.alert}>
                        <strong>Sync error (data might be outdated).</strong>{' '}
                        <small>{objectsError.message}</small>
                    </Alert>
                ) : null}
                {showItems ? children : null}
                {showEmptyNotice ? (
                    <div className={st.empty}>
                        <BsIcon icon="droplet" className={st['empty-icon']} />
                        <div className={st['empty-text']}>{emptyMsg}</div>
                    </div>
                ) : null}
                {showLoading ? (
                    <div className={st.loader}>
                        <LoadingIndicator />
                    </div>
                ) : null}
            </div>
        </div>
    );
}

export default memo(ObjectsLayout);
