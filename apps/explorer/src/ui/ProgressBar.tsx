// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import clsx from 'clsx';
import { motion } from 'framer-motion';

export interface ProgressBarProps {
    progress: number;
    animate?: boolean;
}

export function ProgressBar({ progress, animate }: ProgressBarProps) {
    return (
        <div className="relative w-full rounded-full bg-white">
            {animate && (
                <motion.div
                    initial={{
                        boxShadow: '0 0 10px 10px rgba(255, 255, 255, 1)',
                    }}
                    className="absolute left-0 top-1/2 z-10 h-1 w-1 -translate-y-1/2 bg-white opacity-80"
                    animate={{
                        left: '100%',
                        opacity: 0.1,
                    }}
                    transition={{
                        delay: 0.25,
                        repeatDelay: 0.1,
                        duration: 3,
                        repeat: Infinity,
                        ease: [0, 0, 0.35, 1],
                    }}
                />
            )}
            <motion.div
                className={clsx(
                    'rounded-full py-1',
                    animate
                        ? 'bg-success'
                        : 'bg-gradient-to-r from-success via-success/50 to-success'
                )}
                initial={{ width: 0 }}
                animate={{
                    width: `${progress}%`,
                    type: 'spring',
                    transition: { delay: 0.25, duration: 0.5 },
                }}
            />
        </div>
    );
}
