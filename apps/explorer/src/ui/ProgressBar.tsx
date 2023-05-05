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

const flashPointContainerVariant: Variants = {
    initial: { opacity: 0, left: 0 },
    animate: {
        left: 0,
        opacity: 1,
    },
};

const getFlashPointVariant = (progress: number): Variants => ({
    initial: {
        left: 0,
        opacity: 80,
    },
    animate: {
        left: `${progress}%`,
        opacity: 0,
        transition: {
            repeatDelay: 0.5,
            duration: 3,
            repeat: Infinity,
            ease: [0, 0, 0.35, 1],
        },
    },
});

export interface ProgressBarProps {
    progress: number;
    animate?: boolean;
}

export function ProgressBar({ progress, animate }: ProgressBarProps) {
    const isAnimated = animate && progress > ANIMATION_START_THRESHOLD;

    return (
        <div className="relative w-full rounded-full bg-white">
            <motion.div
                variants={getProgressBarVariant(progress)}
                className={clsx(
                    'rounded-full py-1',
                    isAnimated
                        ? 'bg-success'
                        : 'bg-gradient-to-r from-success via-success/50 to-success'
                )}
                initial="initial"
                animate="animate"
            >
                {isAnimated && (
                    <motion.div
                        variants={flashPointContainerVariant}
                        className="aboslute left-0 motion-reduce:hidden"
                    >
                        <motion.div
                            variants={getFlashPointVariant(progress)}
                            className="absolute top-1/2 z-10 h-1 w-1 -translate-y-1/2 bg-white shadow-glow"
                        />
                    </motion.div>
                )}
            </motion.div>
        </div>
    );
}
