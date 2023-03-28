// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { motion } from 'framer-motion';

export interface ProgressBarProps {
    progress: number;
}

export function ProgressBar({ progress }: ProgressBarProps) {
    return (
        <div className="bg-gray-45 w-full rounded-full">
            <motion.div
                className="from-success via-success/50 to-success rounded-full bg-gradient-to-r py-1"
                initial={{ width: 0 }}
                animate={{
                    width: `${progress}%`,
                    type: 'spring',
                    transition: { delay: 0.25, duration: 2 },
                }}
            />
        </div>
    );
}
