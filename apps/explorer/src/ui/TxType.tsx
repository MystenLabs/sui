// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ReactNode } from 'react';

export type TxTypeProps = {
    isFail?: boolean;
    count?: string;
    children: ReactNode;
};

export function TxType({ isFail, count, children }: TxTypeProps) {
    return (
        <div className="flex">
            <div>{isFail ? 'Fail' : 'Success'}</div>
            {children}
            {count && <div>{count}</div>}
        </div>
    );
}
