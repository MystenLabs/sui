// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Tab as HeadlessTab } from '@headlessui/react';
import { cva } from 'class-variance-authority';
import clsx from 'clsx';
import { createContext, useContext } from 'react';

import { type ExtractProps } from './types';

type TabSize = 'md' | 'lg' | 'sm';

const TabSizeContext = createContext<TabSize | null | undefined>(null);

export const TabPanels = HeadlessTab.Panels;

export type TabPanelProps = ExtractProps<typeof HeadlessTab.Panel> & {
    noGap?: boolean;
};

export function TabPanel({ noGap = false, ...props }: TabPanelProps) {
    return <HeadlessTab.Panel className={noGap ? '' : 'my-4'} {...props} />;
}

export type TabGroupProps = ExtractProps<typeof HeadlessTab.Group> & {
    size?: TabSize;
};

export function TabGroup({ size, ...props }: TabGroupProps) {
    return (
        <TabSizeContext.Provider value={size}>
            <HeadlessTab.Group as="div" {...props} />
        </TabSizeContext.Provider>
    );
}

const tabStyles = cva(
    [
        'border-b border-transparent ui-selected:border-gray-65 font-semibold text-steel-dark disabled:text-steel-dark hover:text-steel-darker active:text-steel -mb-px',
    ],
    {
        variants: {
            size: {
                lg: 'text-heading4 ui-selected:text-steel-darker pb-2',
                md: 'text-body ui-selected:text-steel-darker pb-2',
                sm: 'text-captionSmall font-medium pb-0.5 disabled:opacity-40 ui-selected:text-steel-darker',
            },
        },
        defaultVariants: {
            size: 'md',
        },
    }
);

export type TabProps = ExtractProps<typeof HeadlessTab>;

export function Tab({ ...props }: TabProps) {
    const size = useContext(TabSizeContext);

    return <HeadlessTab className={tabStyles({ size })} {...props} />;
}

export type TabListProps = ExtractProps<typeof HeadlessTab.List> & {
    fullWidth?: boolean;
    disableBottomBorder?: boolean;
    lessSpacing?: boolean;
};

export function TabList({
    fullWidth,
    disableBottomBorder,
    lessSpacing,
    ...props
}: TabListProps) {
    return (
        <HeadlessTab.List
            className={clsx(
                'flex border-gray-45',
                lessSpacing ? 'gap-2' : 'gap-6',
                fullWidth && 'flex-1',
                !disableBottomBorder && 'border-b'
            )}
            {...props}
        />
    );
}
