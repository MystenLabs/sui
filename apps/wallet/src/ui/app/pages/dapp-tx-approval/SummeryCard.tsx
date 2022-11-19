// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import cl from 'classnames';

import type { ReactNode } from 'react';

import st from './DappTxApprovalPage.module.scss';

type SummeryCardProps = {
    header?: string | React.ReactElement;
    transparentHeader?: true;
    children: ReactNode | ReactNode[];
};

export function SummeryCardHeader({
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

export function SummeryCard({
    header,
    transparentHeader,
    children,
}: SummeryCardProps) {
    return (
        <div className={st.card}>
            {header ? (
                <SummeryCardHeader
                    header={header}
                    transparentHeader={transparentHeader}
                />
            ) : null}
            <div className={st.contentWrapper}>{children}</div>
        </div>
    );
}
