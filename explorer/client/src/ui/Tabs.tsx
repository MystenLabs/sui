// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Tab as HeadlessTab } from '@headlessui/react';
import { cva } from 'class-variance-authority';
import { createContext, useContext, type ComponentType } from 'react';

type ExtractProps<T> = T extends ComponentType<infer P> ? P : T;

type TabSize = 'md' | 'lg';

const TabSizeContext = createContext<TabSize | null | undefined>(null);

export const TabPanels = HeadlessTab.Panels;

export type TabPanelProps = ExtractProps<typeof HeadlessTab.Panel>;

export function TabPanel(props: TabPanelProps) {
    return <HeadlessTab.Panel className="my-4" {...props} />;
}

export type TabGroupProps = ExtractProps<typeof HeadlessTab.List> & {
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
        'border-b border-sui-grey-45 ui-selected:border-sui-grey-65 font-semibold text-sui-grey-70 pb-2 -mb-px',
        // TODO: remove once we have a CSS reset:
        'bg-transparent border-0 border-solid outline-none px-0 cursor-pointer',
    ],
    {
        variants: {
            size: {
                lg: 'text-heading4 ui-selected:text-sui-grey-100',
                md: 'text-body ui-selected:text-sui-grey-90',
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

export type TabListProps = ExtractProps<typeof HeadlessTab.List>;

export function TabList(props: TabListProps) {
    return (
        <HeadlessTab.List
            className="flex gap-6 border-b border-sui-grey-45 border-solid border-0"
            {...props}
        />
    );
}
