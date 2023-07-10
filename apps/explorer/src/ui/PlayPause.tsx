// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { AutorefreshPause24, AutorefreshPlay24 } from '@mysten/icons';
import { useContext, useEffect, useRef, useState } from 'react';

import { ActivityContext } from '~/components/Activity';

export interface PlayPauseProps {
	paused?: boolean;
	onChange(): void;
	animateDuration: number;
}

export function PlayPause({ paused, onChange, animateDuration }: PlayPauseProps) {
	const activityContext = useContext(ActivityContext);

	const [progress, setProgress] = useState(0);

	if (!activityContext) {
		throw new Error('PlayPause must be used within ActivityContext.Provider');
	}

	const { startAnimationTimestamp } = activityContext.transactionTable;
	const animationFrameId = useRef<number>();

	const Icon = paused ? AutorefreshPlay24 : AutorefreshPause24;

	useEffect(() => {
		if (paused) {
			if (animationFrameId.current) {
				cancelAnimationFrame(animationFrameId.current);
			}
			setProgress(0);
		}
	}, [paused]);

	useEffect(() => {
		const onTick = (timestamp: number) => {
			const progressTime = timestamp - startAnimationTimestamp;
			const progressPercentage = Math.min((progressTime / animateDuration) * 100, 100);

			setProgress(progressPercentage);

			if (startAnimationTimestamp > timestamp || progressPercentage < 100) {
				animationFrameId.current = requestAnimationFrame(onTick);
			}
		};

		animationFrameId.current = requestAnimationFrame(onTick);

		return () => {
			if (animationFrameId.current) {
				cancelAnimationFrame(animationFrameId.current);
			}
		};
	}, [animateDuration, startAnimationTimestamp]);

	return (
		<button
			type="button"
			aria-label={paused ? 'Paused' : 'Playing'}
			onClick={onChange}
			className="relative cursor-pointer border-none bg-transparent text-steel hover:text-steel-darker"
		>
			<svg className="absolute -rotate-90 text-hero" viewBox="0 0 16 16">
				<circle
					fill="none"
					cx="8"
					cy="8"
					r="7"
					strokeLinecap="round"
					strokeWidth={2}
					stroke="currentColor"
					style={{
						strokeDasharray: 2 * Math.PI * 7,
						strokeDashoffset: 2 * Math.PI * 7 - (progress / 100) * (2 * Math.PI * 7),
					}}
				/>
			</svg>
			<Icon />
		</button>
	);
}
