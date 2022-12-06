// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ReactNode } from 'react';

import { ReactComponent as ContentSuccessStatus } from '../assets/SVGIcons/12px/Check.svg';
import { ReactComponent as ContentFailedStatus } from '../assets/SVGIcons/12px/X.svg';

export type TxTypeProps = {
    isFail?: boolean;
    count?: string;
    children: ReactNode;
};

export function TxType({ isFail, count, children }: TxTypeProps) {
    return (
        <div className="flex items-center justify-center gap-1.5">
            {isFail ? (
                <ContentFailedStatus className="fill-issue-dark" />
            ) : (
                <ContentSuccessStatus className="fill-success" />
            )}
            {children}
            {count && <div>{count}</div>}
        </div>
    );
}
