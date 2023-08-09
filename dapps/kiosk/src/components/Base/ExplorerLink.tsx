// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react';

type LinkOptions = { address: string; text: string } | { object: string; text: string };

/**
 * A link to explorer (should track env and set correct network).
 */
export function ExplorerLink(opts: LinkOptions) {
	const [copied, setCopied] = useState<boolean>(false);
	const link =
		'address' in opts
			? `https://suiexplorer.com/address/${opts.address}?network=testnet`
			: `https://suiexplorer.com/object/${opts.object}?network=testnet`;

	const copyToClipboard = async () => {
		await navigator.clipboard.writeText('address' in opts ? opts.address : opts.object);
		setCopied(true);
		setTimeout(() => {
			setCopied(false);
		}, 3000);
	};
	return (
		<>
			<a href={link} className="underline" target="_blank" rel="noreferrer">
				{opts.text}
			</a>
			<button
				className="!p-1 ml-3 text-xs ease-in-out duration-300 rounded border border-transparent bg-gray-200"
				onClick={copyToClipboard}
			>
				{copied ? 'copied' : 'copy'}
			</button>
		</>
	);
}
