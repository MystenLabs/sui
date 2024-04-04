// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ampli } from '_src/shared/analytics/ampli';
import { useState } from 'react';
import { useNavigate } from 'react-router-dom';

import { Text } from '../../shared/text';
import Close from './close.svg';
import { useBuyNLargeAsset } from './useBuyNLargeAsset';
import { useConfig } from './useConfig';

const SEEN_KEY = 'buy-n-large-seen';

export function BuyNLargeHomePanel() {
	const navigate = useNavigate();
	const [seen, setSeen] = useState(() => {
		const stored = localStorage.getItem(SEEN_KEY);
		if (stored) {
			return JSON.parse(stored);
		}
		return false;
	});
	const config = useConfig();

	const { asset } = useBuyNLargeAsset();

	if (seen || !config || !config.enabled || !asset) return null;

	return (
		<div>
			<div
				role="button"
				onClick={() => {
					navigate(
						`/nft-details?${new URLSearchParams({
							objectId: asset.data?.objectId ?? '',
						}).toString()}`,
					);

					ampli.clickedCollectibleCard({
						objectId: asset?.data?.objectId ?? '',
						collectibleType: asset?.data?.type ?? '',
						sourceScreen: 'HomePanel',
					});
				}}
				className="bg-[#2249E3] flex flex-row items-center rounded-xl px-4 py-3 gap-4 w-full"
			>
				<div className="w-8 h-8">
					<img src={config.homeImage} alt="" className="w-full h-full object-contain" />
				</div>

				<div className="flex-1">
					<Text variant="body" weight="medium" color="white">
						{config.homeDescription}
					</Text>
				</div>

				<div>
					<button
						type="button"
						aria-label="Close"
						className="bg-transparent p-0 m-0 border-none"
						onClick={(e) => {
							e.preventDefault();
							e.stopPropagation();
							localStorage.setItem(SEEN_KEY, JSON.stringify(true));
							setSeen(true);
						}}
					>
						<Close className="text-content-onColor" width={16} height={16} />
					</button>
				</div>
			</div>
		</div>
	);
}
