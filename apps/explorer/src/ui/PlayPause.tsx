// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { AutorefreshPause24, AutorefreshPlay24 } from '@mysten/icons';
import { motion } from 'framer-motion';

export interface PlayPauseProps {
	paused?: boolean;
	onChange(): void;
	animateDuration?: number;
}

export function PlayPause({ paused, onChange, animateDuration }: PlayPauseProps) {
	const Icon = paused ? AutorefreshPlay24 : AutorefreshPause24;

	return (
		<button
			type="button"
			aria-label={paused ? 'Paused' : 'Playing'}
			onClick={onChange}
			className="relative cursor-pointer border-none bg-transparent text-steel hover:text-steel-darker"
		>
			{animateDuration && !paused && (
				<motion.svg className="absolute -rotate-90 text-hero" viewBox="0 0 16 16">
					<motion.circle
						fill="none"
						cx="8"
						cy="8"
						r="7"
						strokeLinecap="round"
						strokeWidth={2}
						stroke="currentColor"
						pathLength={0}
						animate={{
							pathLength: 1,
							type: 'spring',
							transition: { duration: animateDuration + 0.5, repeat: Infinity },
						}}
					/>
				</motion.svg>
			)}
			<Icon />
		</button>
	);
}
