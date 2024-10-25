// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeatureValue } from '@growthbook/growthbook-react';
import { useEffect, useState } from 'react';

import { Text } from '../shared/text';
import Close from './buynlarge/close.svg';

const STAKE_SEEN_KEY = 'stake-warning-seen';

type StakeWarningItem = {
	enabled: boolean;
	link: string;
	text: string;
	endDate: string;
};

export function StakeWarning() {
	const [today, setToday] = useState(new Date());
	const [seen, setSeen] = useState<boolean>(() => {
		const stored = localStorage.getItem(STAKE_SEEN_KEY);
		if (stored) {
			return JSON.parse(stored);
		}
		return false;
	});

	const warning = useFeatureValue<StakeWarningItem | null>('wallet-stake-warning', null);

	useEffect(() => {
		// We update every minute to make sure the warning is removed after the end date
		const interval = setInterval(() => {
			setToday(new Date());
		}, 1000 * 60);

		return () => clearInterval(interval);
	}, []);

	if (
		seen ||
		!warning ||
		!warning.enabled ||
		today.getTime() > new Date(warning.endDate).getTime()
	) {
		return null;
	}

	return (
		<a
			target="_blank"
			rel="noreferrer"
			href={warning.link}
			className="flex flex-row items-center rounded-xl px-4 py-3 gap-4 w-full no-underline"
			style={{
				backgroundColor: '#211C35',
			}}
		>
			<div className="flex-1">
				<Text variant="body" weight="medium" color="white">
					{warning.text.replace(
						'{endDate}',
						new Date(warning.endDate).toLocaleString(undefined, {
							dateStyle: 'full',
							timeStyle: 'short',
						}),
					)}
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
						localStorage.setItem(STAKE_SEEN_KEY, JSON.stringify(true));
						setSeen(true);
					}}
				>
					<Close className="text-content-onColor" width={16} height={16} />
				</button>
			</div>
		</a>
	);
}
