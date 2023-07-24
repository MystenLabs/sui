// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { ThumbUpFill32 } from '@mysten/icons';
import clsx from 'clsx';

export function StatusIcon({ success }: { success: boolean }) {
	return (
		<div
			className={clsx(
				'flex h-12 w-12 items-center  justify-center rounded-full border-2 border-dotted p-1',
				success ? 'border-success' : 'border-issue',
			)}
		>
			<div
				className={clsx(
					'flex h-8 w-8 items-center justify-center rounded-full',
					success ? 'bg-success' : 'bg-issue',
				)}
			>
				<ThumbUpFill32
					fill="currentColor"
					className={clsx('text-2xl text-white', !success && 'rotate-180')}
				/>
			</div>
		</div>
	);
}
