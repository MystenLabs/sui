// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    X12 as ContentFailedStatus,
    Check12 as ContentSuccessStatus,
} from '@mysten/icons';
import { type TransactionKindName } from '@mysten/sui.js';

type TransactionTypeLabel = TransactionKindName | 'Batch';

export type TransactionTypeProps = {
    isSuccess?: boolean;
    count?: string;
    type?: TransactionTypeLabel;
};

export function TransactionType({
    isSuccess,
    count,
    type,
}: TransactionTypeProps) {
    return (
        <div className="flex items-center gap-1.5">
            {isSuccess ? (
                <ContentSuccessStatus className="text-success" />
            ) : (
                <ContentFailedStatus className="text-issue-dark" />
            )}
            {type}
            {count && (
                <div className="rounded-lg bg-gray-40 py-0.5 px-1.25">
                    {count}
                </div>
            )}
        </div>
    );
}
