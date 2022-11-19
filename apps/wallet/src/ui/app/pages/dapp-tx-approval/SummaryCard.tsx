// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import cl from 'classnames';

import type { ReactNode } from 'react';

import st from './DappTxApprovalPage.module.scss';

type SummaryCardProps = {
    header?: string | React.ReactElement;
    transparentHeader?: true;
    children: ReactNode | ReactNode[];
};

export function SummaryCardHeader({
    header,
    transparentHeader,
}: {
    header: string | React.ReactElement;
    transparentHeader?: true;
}) {
    return (
        <div
            className={cl(st.header, transparentHeader && st.transparentHeader)}
        >
            {header}
        </div>
    );
}

export function SummaryCard({
    header,
    transparentHeader,
    children,
}: SummaryCardProps) {
    return (
        <div className={st.card}>
            {header ? (
                <SummaryCardHeader
                    header={header}
                    transparentHeader={transparentHeader}
                />
            ) : null}
            <div
                className={cl(
                    st.contentWrapper,
                    transparentHeader && st.packageInfo
                )}
            >
                {children}
            </div>
        </div>
    );
}
