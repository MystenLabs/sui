// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ChevronRight12, ChevronRight16 } from '@mysten/icons';
import clsx from 'clsx';
import { type ReactNode, useState } from 'react';

import { Card, type CardProps } from '~/ui/Card';
import { Divider } from '~/ui/Divider';
import { Heading } from '~/ui/Heading';
import { Text } from '~/ui/Text';

type Size = 'md' | 'sm';

interface TransactionCardSectionProps {
    children: ReactNode;
    collapsedOnLoad?: boolean;
    title?: string | ReactNode;
}

export function TransactionCardSection({
    title,
    collapsedOnLoad,
    children,
}: TransactionCardSectionProps) {
    const [expanded, setExpanded] = useState<boolean>(!collapsedOnLoad);

    const toggleExpanded = () => setExpanded((prevExpanded) => !prevExpanded);

    return (
        <div className="flex w-full flex-col gap-3">
            {title && (
                <div
                    role="button"
                    className="flex items-center gap-2"
                    onClick={toggleExpanded}
                >
                    {typeof title === 'string' ? (
                        <Text color="steel-darker" variant="body/semibold">
                            {title}
                        </Text>
                    ) : (
                        title
                    )}
                    <Divider />
                    <ChevronRight12
                        height={12}
                        width={12}
                        className={clsx(
                            'cursor-pointer text-gray-45',
                            !expanded && 'rotate-90'
                        )}
                    />
                </div>
            )}

            {expanded && children}
        </div>
    );
}

export interface TransactionCardProps extends Omit<CardProps, 'size'> {
    children: ReactNode;
    title?: string | ReactNode;
    footer?: ReactNode;
    collapsible?: boolean;
    size?: Size;
}

export function TransactionCard({
    title,
    footer,
    collapsible,
    size = 'md',
    children,
    ...cardProps
}: TransactionCardProps) {
    const [isExpanded, setIsExpanded] = useState(true);

    const handleExpandClick = () => {
        if (collapsible) {
            setIsExpanded((prevIsExpanded: boolean) => !prevIsExpanded);
        }
    };

    return (
        <div className="w-full">
            <Card
                rounded="2xl"
                border="gray45"
                bg="white"
                spacing="none"
                {...cardProps}
            >
                <div
                    className={clsx(
                        size === 'md' ? 'px-6 py-7' : 'px-4 py-4.5'
                    )}
                >
                    {title && (
                        <div
                            role={collapsible ? 'button' : undefined}
                            onClick={handleExpandClick}
                            className={clsx(
                                'flex justify-between',
                                isExpanded && 'mb-6'
                            )}
                        >
                            <Heading
                                variant={
                                    size === 'md'
                                        ? 'heading4/semibold'
                                        : 'heading6/semibold'
                                }
                                color="steel-darker"
                            >
                                {title}
                            </Heading>

                            {collapsible && (
                                <ChevronRight16
                                    className={clsx(
                                        'cursor-pointer text-steel',
                                        isExpanded && 'rotate-90'
                                    )}
                                />
                            )}
                        </div>
                    )}

                    {(isExpanded || !title) && (
                        <div className="flex flex-col gap-6">{children}</div>
                    )}
                </div>

                {footer && (
                    <div
                        className={clsx(
                            'rounded-b-2xl bg-sui/10 py-2.5',
                            size === 'md' ? 'px-6' : 'px-4'
                        )}
                    >
                        {footer}
                    </div>
                )}
            </Card>
        </div>
    );
}
