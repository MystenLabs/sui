// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { ThumbUpFill24, ThumbDownFill24 } from '@mysten/icons';
import clsx from 'clsx';

export function StatusIcon({ success }: { success: boolean }) {
	const Icon = success ? ThumbUpFill24 : ThumbDownFill24;

	return (
		<div className="sm:min-w-16 flex h-10 w-10 min-w-10 items-center justify-center rounded-xl bg-white/60 sm:h-16 sm:w-16 lg:h-18 lg:w-18 lg:min-w-18">
			<div
				className={clsx(
					'flex h-6 w-6 items-center justify-center rounded-full sm:h-10 sm:w-10',
					success ? 'bg-success' : 'bg-issue',
				)}
			>
				<Icon fill="currentColor" className="text-white sm:text-2xl" />
			</div>
		</div>
	);
}
