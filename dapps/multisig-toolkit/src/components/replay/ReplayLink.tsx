// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export function ReplayLink({
	id,
	digest,
	module,
	network,
	text,
	isAddress,
}: {
	text: string;
	network: string;
	digest?: string;
	id?: string;
	module?: string;
	isAddress?: boolean;
}) {
	const url = () => {
		if (digest) {
			return `https://suiexplorer.com/txblock/${digest}?network=${network}`;
		}
		if (module) {
			return `https://suiexplorer.com/${isAddress ? 'address' : 'object'}/${id}?module=${module}&network=${network}`;
		}

		return `https://suiexplorer.com/${isAddress ? 'address' : 'object'}/${id}?network=${network}`;
	};

	return (
		<a href={url()} className="underline" target="_blank" rel="noreferrer">
			{text}
		</a>
	);
}
