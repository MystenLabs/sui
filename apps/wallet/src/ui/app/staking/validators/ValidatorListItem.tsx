// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { motion, AnimatePresence } from 'framer-motion';

import { ImageIcon } from '_app/shared/image-icon';
import { Text } from '_app/shared/text';
import { IconTooltip } from '_app/shared/tooltip';
import ExplorerLink from '_components/explorer-link';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';
import Icon, { SuiIcons } from '_components/icon';
import { useMiddleEllipsis } from '_hooks';

const TRUNCATE_MAX_LENGTH = 10;
const TRUNCATE_PREFIX_LENGTH = 6;

type ValidatorListItemProp = {
    selected?: boolean;
    label: string;
    value: string | number;
    validatorName: string;
    validatorAddress: string;
    logo: string | null;
};
export function ValidatorListItem({
    selected,
    validatorName,
    label,
    value,
    logo,
    validatorAddress,
}: ValidatorListItemProp) {
    const truncatedAddress = useMiddleEllipsis(
        validatorAddress,
        TRUNCATE_MAX_LENGTH,
        TRUNCATE_PREFIX_LENGTH
    );

    return (
        <AnimatePresence>
            <motion.div
                whileHover={{ scale: 0.97 }}
                animate={selected ? { scale: 0.97 } : { scale: 1 }}
            >
                <div
                    className={cl(
                        selected && 'bg-sui/10',
                        'flex justify-between w-full hover:bg-sui/10 py-3.5 px-2 rounded-lg group items-center gap-1'
                    )}
                    role="button"
                >
                    <div className="flex gap-2.5 items-center">
                        <div className="relative">
                            {selected && (
                                <Icon
                                    icon={SuiIcons.CheckFill}
                                    className="absolute text-success text-heading6 translate-x-4 -translate-y-1 rounded-full bg-white"
                                />
                            )}

                            <ImageIcon
                                src={logo}
                                label={validatorName}
                                fallback={validatorName}
                                circle
                            />
                        </div>
                        <div className="flex flex-col gap-1.5">
                            <Text
                                variant="body"
                                weight="semibold"
                                color="gray-90"
                            >
                                {validatorName}
                            </Text>
                            <ExplorerLink
                                type={ExplorerLinkType.validator}
                                validator={validatorAddress}
                                className={cl(
                                    selected && 'text-hero-dark',
                                    'text-steel-dark no-underline font-mono font-medium group-hover:text-hero-dark'
                                )}
                                showIcon={false}
                            >
                                {truncatedAddress}
                            </ExplorerLink>
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
                                className={cl(
                                    selected && '!opacity-100',
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
