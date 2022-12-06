// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ReactNode } from 'react';

import { ReactComponent as ContentSuccessStatus } from '../assets/SVGIcons/12px/Check.svg';
import { ReactComponent as ContentFailedStatus } from '../assets/SVGIcons/12px/X.svg';

export type TxTypeProps = {
    isSuccess?: boolean;
    count?: string;
    children: ReactNode;
};

export function TxType({ isSuccess, count, children }: TxTypeProps) {
    return (
        <div className="flex flex-col items-start">
            <div className="flex items-center justify-center gap-1.5">
                {isSuccess ? (
                    <ContentSuccessStatus className="fill-success" />
                ) : (
                    <ContentFailedStatus className="fill-issue-dark" />
                )}
                {children}
                {count && (
                    <div className="rounded-lg bg-gray-40 py-0.5 px-1.25">
                        {count}
                    </div>
                )}
            </div>
        </div>
    );
}
