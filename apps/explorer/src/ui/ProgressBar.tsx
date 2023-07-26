// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import clsx from 'clsx';
import { motion, type Variants } from 'framer-motion';

const ANIMATION_START = 0.25;
const ANIMATION_START_THRESHOLD = 20;

const getProgressBarVariant = (progress: number): Variants => ({
	initial: {
		width: 0,
	},
	animate: {
		transition: {
			delay: ANIMATION_START,
			duration: 0.5,
			delayChildren: ANIMATION_START * 8,
		},
		width: `${progress}%`,
	},
});

export interface ProgressBarProps {
	progress: number;
	animate?: boolean;
}

export function ProgressBar({ progress, animate }: ProgressBarProps) {
	const isAnimated = animate && progress > ANIMATION_START_THRESHOLD;

	return (
		<div className="relative w-full rounded-full bg-success-light">
			<motion.div
				variants={getProgressBarVariant(progress)}
				className={clsx(
					'rounded-full py-1',
					isAnimated ? 'bg-success' : 'bg-gradient-to-r from-success via-success/50 to-success',
				)}
				initial="initial"
				animate="animate"
			/>
			{isAnimated && (
				<motion.div
					initial={{
						left: 0,
					}}
					animate={{
						left: `${Math.floor(progress)}%`,
						opacity: [0, 1, 0],
						transition: {
							delay: 1.5,
							repeatDelay: 1,
							duration: 6,
							repeat: Infinity,
							ease: [0.16, 1, 0.3, 1],
						},
					}}
					className="absolute top-1/2 z-10 h-0 w-0 -translate-y-1/2 bg-white opacity-0 shadow-glow"
				/>
			)}
		</div>
	);
}
