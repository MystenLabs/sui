// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import cl from 'classnames';

import type { ReactNode } from 'react';

import st from './DappTxApprovalPage.module.scss';

type SummaryCardProps = {
    header?: ReactNode;
    transparentHeader?: boolean;
    children: ReactNode;
};

export function SummaryCardHeader({
    children,
    transparentHeader,
}: {
    children: ReactNode;
    transparentHeader?: boolean;
}) {
    return (
        <div
            className={cl(st.header, transparentHeader && st.transparentHeader)}
        >
            {children}
        </div>
    );
}

export function SummaryCardContent({ children }: { children: ReactNode }) {
    return <div className={st.contentWrapper}>{children}</div>;
}

export function SummaryCard({ transparentHeader, children }: SummaryCardProps) {
    return (
        <div className={cl(st.card, transparentHeader && st.packageInfo)}>
            {children}
        </div>
    );
}
