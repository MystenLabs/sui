// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Disclosure } from '@headlessui/react';
import { ChevronRight12, ChevronRight16 } from '@mysten/icons';
import clsx from 'clsx';
import { type ReactNode } from 'react';

import { Card, type CardProps } from '~/ui/Card';
import { Divider } from '~/ui/Divider';
import { Heading } from '~/ui/Heading';
import { Text } from '~/ui/Text';

type Size = 'md' | 'sm';

interface TransactionBlockCardHeaderProps {
    open: boolean;
    size: Size;
    title?: string | ReactNode;
    collapsible?: boolean;
}

function TransactionBlockCardHeader({
    open,
    size,
    title,
    collapsible,
}: TransactionBlockCardHeaderProps) {
    if (!title) {
        return null;
    }

    const headerContent = (
        <div
            className={clsx(
                'flex w-full justify-between',
                open && size === 'md' && 'pb-6',
                open && size === 'sm' && 'pb-4.5'
            )}
        >
            {typeof title === 'string' ? (
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
            ) : (
                title
            )}

            {collapsible && (
                <ChevronRight16
                    className={clsx(
                        'cursor-pointer text-steel',
                        open && 'rotate-90'
                    )}
                />
            )}
        </div>
    );

    if (collapsible) {
        return (
            <Disclosure.Button as="div" className="cursor-pointer">
                {headerContent}
            </Disclosure.Button>
        );
    }

    return <>{headerContent}</>;
}

interface TransactionBlockCardSectionProps {
    children: ReactNode;
    defaultOpen?: boolean;
    title?: string | ReactNode;
}

export function TransactionBlockCardSection({
    title,
    defaultOpen = true,
    children,
}: TransactionBlockCardSectionProps) {
    return (
        <div className="flex w-full flex-col gap-3">
            <Disclosure defaultOpen={defaultOpen}>
                {({ open }) => (
                    <>
                        {title && (
                            <Disclosure.Button>
                                <div className="flex items-center gap-2">
                                    {typeof title === 'string' ? (
                                        <Text
                                            color="steel-darker"
                                            variant="body/semibold"
                                        >
                                            {title}
                                        </Text>
                                    ) : (
                                        title
                                    )}
                                    <Divider />
                                    <ChevronRight12
                                        className={clsx(
                                            'h-4 w-4 cursor-pointer text-gray-45',
                                            open && 'rotate-90'
                                        )}
                                    />
                                </div>
                            </Disclosure.Button>
                        )}

                        <Disclosure.Panel>{children}</Disclosure.Panel>
                    </>
                )}
            </Disclosure>
        </div>
    );
}

export interface TransactionBlockCardProps extends Omit<CardProps, 'size'> {
    children: ReactNode;
    title?: string | ReactNode;
    footer?: ReactNode;
    collapsible?: boolean;
    size?: Size;
}

export function TransactionBlockCard({
    title,
    footer,
    collapsible,
    size = 'md',
    children,
    ...cardProps
}: TransactionBlockCardProps) {
    return (
        <div className="relative w-full">
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
                    <Disclosure defaultOpen>
                        {({ open }) => (
                            <>
                                <TransactionBlockCardHeader
                                    open={open}
                                    size={size}
                                    title={title}
                                    collapsible={collapsible}
                                />

                                <Disclosure.Panel>{children}</Disclosure.Panel>
                            </>
                        )}
                    </Disclosure>
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
