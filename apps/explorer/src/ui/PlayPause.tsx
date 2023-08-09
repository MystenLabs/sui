// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { AutorefreshPause24, AutorefreshPlay24 } from '@mysten/icons';
import { motion } from 'framer-motion';
import { useEffect } from 'react';

const getAnimationVariants = (duration: number) => ({
	initial: {
		pathLength: 0,
	},
	animate: {
		pathLength: 1,
		transition: {
			duration: duration,
		},
	},
});

export interface PlayPauseProps {
	paused?: boolean;
	onChange(): void;
	animate?: {
		duration: number;
		start: boolean;
		setStart: (bool: boolean) => void;
	};
}

export function PlayPause({ paused, onChange, animate }: PlayPauseProps) {
	const Icon = paused ? AutorefreshPlay24 : AutorefreshPause24;

	const isAnimating = animate?.start && !paused;

	useEffect(() => {
		let timer: NodeJS.Timeout;

		if (isAnimating) {
			timer = setTimeout(() => {
				animate.setStart(false);
			}, animate.duration * 1000);
		}

		return () => clearTimeout(timer);
	}, [animate, isAnimating]);

	return (
		<button
			type="button"
			aria-label={paused ? 'Paused' : 'Playing'}
			onClick={onChange}
			className="relative cursor-pointer border-none bg-transparent text-steel hover:text-steel-darker"
		>
			{isAnimating && (
				<motion.svg className="absolute -rotate-90 text-hero" viewBox="0 0 16 16">
					<motion.circle
						fill="none"
						cx="8"
						cy="8"
						r="7"
						strokeLinecap="round"
						strokeWidth={2}
						stroke="currentColor"
						variants={getAnimationVariants(animate.duration)}
						initial="initial"
						animate="animate"
					/>
				</motion.svg>
			)}
			<Icon />
		</button>
	);
}
