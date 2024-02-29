// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useReplayData } from './useReplayData';

export function ReplayLink({
	id,
	digest,
	module,
	text,
	isAddress,
}: {
	text: string;
	digest?: string;
	id?: string;
	module?: string;
	isAddress?: boolean;
}) {
	const { network } = useReplayData();

	const url = () => {
		if (digest) {
			return `https://suiexplorer.com/txblock/${digest}?network=${network || 'mainnet'}`;
		}
		if (module) {
			return `https://suiexplorer.com/${
				isAddress ? 'address' : 'object'
			}/${id}?module=${module}&network=${network || 'mainnet'}`;
		}

		return `https://suiexplorer.com/${isAddress ? 'address' : 'object'}/${id}?network=${
			network || 'mainnet'
		}`;
	};

	return (
		<a href={url()} className="underline" target="_blank" rel="noreferrer">
			{text}
		</a>
	);
}
