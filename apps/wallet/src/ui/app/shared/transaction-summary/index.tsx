// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { type TransactionSummary as TransactionSummaryType } from '@mysten/core';

import LoadingIndicator from '../../components/loading/LoadingIndicator';
import { BalanceChanges } from './cards/BalanceChanges';
import { ExplorerLinkCard } from './cards/ExplorerLink';
import { GasSummary } from './cards/GasSummary';
import { ObjectChanges } from './cards/ObjectChanges';

export function TransactionSummary({
    summary,
    isLoading,
    /* todo: remove this, we're using it until we update tx approval page */
    showGasSummary = false,
}: {
    summary: TransactionSummaryType;
    isLoading?: boolean;
    showGasSummary?: boolean;
}) {
    return (
        <section className="-mx-5 bg-sui/10">
            {isLoading ? (
                <div className="flex items-center justify-center p-10">
                    <LoadingIndicator />
                </div>
            ) : (
                <div>
                    <div className="px-5 py-10">
                        <div className="flex flex-col gap-4">
                            <BalanceChanges changes={summary?.balanceChanges} />
                            <ObjectChanges changes={summary?.objectSummary} />
                            {showGasSummary && (
                                <GasSummary gasSummary={summary?.gas} />
                            )}
                            <ExplorerLinkCard
                                digest={summary?.digest}
                                timestamp={summary?.timestamp}
                            />
                        </div>
                    </div>
                </div>
            )}
        </section>
    );
}
