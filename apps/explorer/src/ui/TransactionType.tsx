// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ReactNode } from 'react';

import { ReactComponent as ContentSuccessStatus } from './icons/check_12x12.svg';
import { ReactComponent as ContentFailedStatus } from './icons/x.svg';

export type TransactionTypeProps = {
    isSuccess?: boolean;
    count?: string;
    children: ReactNode;
};

export function TransactionType({
    isSuccess,
    count,
    children,
}: TransactionTypeProps) {
    return (
        <div className="flex flex-col items-start">
            <div className="flex items-center justify-center gap-1.5">
                {isSuccess ? (
                    <ContentSuccessStatus className="text-success" />
                ) : (
                    <ContentFailedStatus className="text-issue-dark" />
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
