// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { memo } from 'react';

import AccountAddress from '_components/account-address';
import Alert from '_components/alert';
import Icon from '_components/icon';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import { useObjectsState } from '_hooks';

import type { ReactNode } from 'react';

import st from './ObjectsLayout.module.scss';

export type ObjectsLayoutProps = {
    children: ReactNode;
    emptyMsg: string;
    totalItems: number;
};

function ObjectsLayout({ children, emptyMsg, totalItems }: ObjectsLayoutProps) {
    const { loading, error, showError, syncedOnce } = useObjectsState();
    const showEmptyNotice = syncedOnce && !totalItems;
    const showItems = syncedOnce && totalItems;
    return (
        <div className={st.container}>
            <div>
                <span className={st.title}>Active Account:</span>
                <AccountAddress />
            </div>
            <div className={st.items}>
                {showError && error ? (
                    <Alert className={st.alert}>
                        <strong>Sync error (data might be outdated).</strong>{' '}
                        <small>{error.message}</small>
                    </Alert>
                ) : null}
                {showItems ? children : null}
                {showEmptyNotice ? (
                    <div className={st.empty}>
                        <Icon icon="droplet" className={st['empty-icon']} />
                        <div className={st['empty-text']}>{emptyMsg}</div>
                    </div>
                ) : null}
                {loading ? (
                    <div className={st.loader}>
                        <LoadingIndicator />
                    </div>
                ) : null}
            </div>
        </div>
    );
}

export default memo(ObjectsLayout);
