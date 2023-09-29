// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { ExplorerLinkType } from '_src/ui/app/components/explorer-link/ExplorerLinkType';
import { useExplorerLink } from '_src/ui/app/hooks/useExplorerLink';
import { ArrowUpRight12 } from '@mysten/icons';
import { useEffect, useState } from 'react';

import { Text } from '../../text';
import { Card } from '../Card';

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

export function ExplorerLinkCard({ digest, timestamp }: { digest?: string; timestamp?: string }) {
	const shouldShowExplorerLink = useShouldShowExplorerLink(timestamp, digest);
	const explorerHref = useExplorerLink({
		type: ExplorerLinkType.transaction,
		transactionID: digest!,
	});
	if (!shouldShowExplorerLink) return null;
	return (
		<Card as="a" href={explorerHref!} target="_blank">
			<div className="flex items-center justify-center gap-1 tracking-wider w-full">
				<Text variant="captionSmall" weight="semibold">
					View on Explorer
				</Text>
				<ArrowUpRight12 className="text-steel text-pSubtitle" />
			</div>
		</Card>
	);
}
