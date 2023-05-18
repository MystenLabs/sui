// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ArrowShowAndHideDown12 } from '@mysten/icons';
import cl from 'classnames';
import { AnimatePresence, motion } from 'framer-motion';
import { useState } from 'react';

import type { ReactNode } from 'react';

type CollapseProps = {
    title: string;
    initialIsOpen?: boolean;
    children: ReactNode | ReactNode[];
};

export function Collapse({
    title,
    children,
    initialIsOpen = false,
}: CollapseProps) {
    const [isOpen, setIsOpen] = useState(initialIsOpen);
    return (
        <div className="flex flex-nowrap flex-col items-stretch">
            <div
                className={cl(
                    'group cursor-pointer text-steel-darker hover:text-hero',
                    'ease-ease-in-out-cubic duration-200',
                    'border-0 border-b border-solid border-b-gray-45 hover:border-b-hero',
                    'flex flex-nowrap flex-row pb-2'
                )}
                onClick={() => setIsOpen(!isOpen)}
            >
                <div className="flex-1 truncate font-semibold text-caption uppercase tracking-wider">
                    {title}
                </div>
                <ArrowShowAndHideDown12
                    className={cl(
                        'text-steel group-hover:text-hero text-caption',
                        'ease-ease-in-out-cubic duration-200',
                        !isOpen && '-rotate-90'
                    )}
                />
            </div>
            <AnimatePresence initial={false}>
                {isOpen ? (
                    <motion.div
                        initial={{ height: 0, opacity: 0 }}
                        animate={{
                            height: 'auto',
                            opacity: 1,
                        }}
                        transition={{ duration: 0.2, ease: [0.65, 0, 0.35, 1] }}
                        exit={{
                            height: 0,
                            opacity: 0,
                            transition: {
                                ease: [0.65, 0, 0.35, 1],
                                duration: 0.1,
                                height: { delay: 0.1 },
                            },
                        }}
                    >
                        <div className="pt-3">{children}</div>
                    </motion.div>
                ) : null}
            </AnimatePresence>
        </div>
    );
}
