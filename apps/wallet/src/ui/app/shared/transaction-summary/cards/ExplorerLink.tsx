// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { ArrowUpRight12 } from '@mysten/icons';
import { useEffect, useState } from 'react';

import { Card } from '../Card';
import ExplorerLink from '_src/ui/app/components/explorer-link';
import { ExplorerLinkType } from '_src/ui/app/components/explorer-link/ExplorerLinkType';

const TIME_TO_WAIT_FOR_EXPLORER = 60 * 1000;

function useShouldShowExplorerLink(timestamp?: string, digest?: string) {
    const [shouldShow, setShouldShow] = useState(false);
    useEffect(() => {
        if (!digest) return;
        const diff = Date.now() - new Date(Number(timestamp)).getTime();
        // if we have a timestamp, wait at least 1m from the timestamp, otherwise wait 1m from now
        const showAfter = timestamp
            ? Math.max(0, TIME_TO_WAIT_FOR_EXPLORER - diff)
            : TIME_TO_WAIT_FOR_EXPLORER;
        const timeout = setTimeout(() => setShouldShow(true), showAfter);
        return () => clearTimeout(timeout);
    }, [timestamp, digest]);

    return shouldShow;
}

export function ExplorerLinkCard({
    digest,
    timestamp,
}: {
    digest?: string;
    timestamp?: string;
}) {
    const shouldShowExplorerLink = useShouldShowExplorerLink(timestamp, digest);
    if (!shouldShowExplorerLink) return null;
    return (
        <Card>
            <div className="flex items-center justify-center gap-1">
                <ExplorerLink
                    type={ExplorerLinkType.transaction}
                    transactionID={digest!}
                    title="View on Sui Explorer"
                    className="text-sui-dark text-pSubtitleSmall font-semibold no-underline uppercase tracking-wider"
                    showIcon={false}
                >
                    View on Explorer
                </ExplorerLink>
                <ArrowUpRight12 className="text-steel text-pSubtitle" />
            </div>
        </Card>
    );
}
