// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import cl from 'classnames';

import type { ReactNode } from 'react';

import st from './TransactionRequest.module.scss';

type SummaryCardProps = {
    header?: ReactNode;
    transparentHeader?: boolean;
    children: ReactNode;
};

export function SummaryCard({
    transparentHeader,
    children,
    header,
}: SummaryCardProps) {
    return (
        <div className={cl(st.card, transparentHeader && st.packageInfo)}>
            <div
                className={cl(
                    st.header,
                    transparentHeader && st.transparentHeader
                )}
            >
                {header}
            </div>
            <div className={st.contentWrapper}>{children}</div>
        </div>
    );
}
