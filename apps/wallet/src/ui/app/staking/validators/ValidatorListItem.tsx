// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { CheckFill16 } from '@mysten/icons';
import { cx } from 'class-variance-authority';
import { motion, AnimatePresence } from 'framer-motion';

import { ValidatorLogo } from './ValidatorLogo';
import { Text } from '_app/shared/text';

type ValidatorListItemProp = {
    selected?: boolean;
    value: string | number;
    validatorAddress: string;
};
export function ValidatorListItem({
    selected,
    value,
    validatorAddress,
}: ValidatorListItemProp) {
    return (
        <AnimatePresence>
            <motion.div
                whileHover={{ scale: 0.98 }}
                animate={selected ? { scale: 0.98 } : { scale: 1 }}
            >
                <div
                    className={cx(
                        selected ? 'bg-sui/10' : '',
                        'flex justify-between w-full hover:bg-sui/10 py-3.5 px-2 rounded-lg group items-center gap-1'
                    )}
                    role="button"
                >
                    <div className="flex gap-2.5 items-center justify-start">
                        <div className="relative flex gap-0.5 w-full">
                            {selected && (
                                <CheckFill16
                                    fill="fillCurrent"
                                    className="text-success text-heading6 absolute translate-x-4 -translate-y-1 rounded-full bg-white"
                                />
                            )}
                            <ValidatorLogo
                                validatorAddress={validatorAddress}
                                showAddress
                                iconSize="md"
                                size="body"
                                showActiveStatus
                            />
                        </div>
                    </div>
                    <div className="flex gap-0.5 items-center">
                        <div className="flex gap-0.5 leading-none">
                            <Text
                                variant="body"
                                weight="semibold"
                                color="steel-darker"
                            >
                                {value}
                            </Text>
                            <div
                                className={cx(
                                    selected ? '!opacity-100' : '',
                                    'text-steel items-baseline text-subtitle h-3 flex opacity-0 group-hover:opacity-100'
                                )}
                            ></div>
                        </div>
                    </div>
                </div>
            </motion.div>
        </AnimatePresence>
    );
}
