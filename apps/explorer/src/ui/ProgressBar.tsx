// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { motion } from 'framer-motion';

export interface ProgressBarProps {
    progress: number;
}

export function ProgressBar({ progress }: ProgressBarProps) {
    return (
        <div className="w-full rounded-full bg-white">
            <motion.div
                className="rounded-full bg-gradient-to-r from-success via-success/50 to-success py-1"
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
