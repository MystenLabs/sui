// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { motion } from 'framer-motion';

export interface ProgressBarProps {
    progress: number;
}

export function ProgressBar({ progress }: ProgressBarProps) {
    return (
        <div className="w-full rounded-full bg-gray-45">
            <motion.div
                className="relative rounded-full bg-gradient-to-r from-success via-success/50 to-success py-1"
                initial={{ width: 0 }}
                animate={{
                    width: `${progress}%`,
                    type: 'spring',
                    transition: { delay: 0.25, duration: 0.5 },
                }}
            >
                <motion.div
                    className="absolute left-1/2 top-1/2 h-1 w-1 -translate-x-1/2 -translate-y-1/2 rounded-full bg-white opacity-80 shadow-glow"
                    animate={{
                        left: '95%',
                        transition: {
                            delay: 0.25,
                            duration: 1,
                            repeat: Infinity,
                        },
                    }}
                />
            </motion.div>
        </div>
    );
}
