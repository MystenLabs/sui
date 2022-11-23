// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Tab as HeadlessTab } from '@headlessui/react';
import { cva } from 'class-variance-authority';
import clsx from 'clsx';
import { createContext, useContext } from 'react';

import { type ExtractProps } from './types';

type TabSize = 'md' | 'lg';

const TabSizeContext = createContext<TabSize | null | undefined>(null);

export const TabPanels = HeadlessTab.Panels;

export type TabPanelProps = ExtractProps<typeof HeadlessTab.Panel>;

export function TabPanel(props: TabPanelProps) {
    return <HeadlessTab.Panel className="my-4" {...props} />;
}

export type TabGroupProps = ExtractProps<typeof HeadlessTab.Group> & {
    size?: TabSize;
};

export function TabGroup({ size, ...props }: TabGroupProps) {
    return (
        <TabSizeContext.Provider value={size}>
            <HeadlessTab.Group {...props} />
        </TabSizeContext.Provider>
    );
}

const tabStyles = cva(
    [
        'border-b border-transparent ui-selected:border-gray-65 font-semibold text-steel-dark hover:text-steel-darker active:text-steel pb-2 -mb-px',
        // TODO: remove once we have a CSS reset:
        'bg-transparent border-0 border-solid outline-none px-0 cursor-pointer',
    ],
    {
        variants: {
            size: {
                lg: 'text-heading4 ui-selected:text-gray-100',
                md: 'text-body ui-selected:text-gray-90',
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
};

export function TabList({
    fullWidth,
    disableBottomBorder,
    ...props
}: TabListProps) {
    return (
        <HeadlessTab.List
            className={clsx(
                'flex gap-6 border-gray-45 border-solid border-0',
                fullWidth && 'flex-1',
                !disableBottomBorder && 'border-b'
            )}
            {...props}
        />
    );
}
