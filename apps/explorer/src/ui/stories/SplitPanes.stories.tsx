// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';

import { SplitPanes, type SplitPanesProps } from '../SplitPanes';

export default {
    component: SplitPanes,
} as Meta;

const splitPanels = [
    {
        panel: (
            <div
                key={1}
                style={{ whiteSpace: 'nowrap' }}
                className="h-full w-[1000px] bg-success-light"
            >
                {'long text here '.repeat(5)}
            </div>
        ),
        defaultSize: 10,
        collapsible: true,
        collapsibleButton: true,
    },
    {
        panel: (
            <div key={2} className="h-full w-[1000px] bg-issue-light">
                Second
            </div>
        ),
        defaultSize: 40,
    },
    {
        panel: (
            <div key={3} className="h-full w-[1000px] w-full bg-sui">
                Third
            </div>
        ),
        defaultSize: 50,
    },
];

const SplitPanesStory: StoryObj<SplitPanesProps> = {
    render: (props) => (
        <div className="h-[500px] w-[1000px]">
            <SplitPanes {...props} splitPanels={splitPanels} />
        </div>
    ),
};

export const HorizontalSplitPanes: StoryObj<SplitPanesProps> = {
    ...SplitPanesStory,
    args: {
        direction: 'horizontal',
    },
};

export const VerticalSplitPanes: StoryObj<SplitPanesProps> = {
    ...SplitPanesStory,
    args: {
        direction: 'vertical',
    },
};

export const SplitPanesWithStateSaveOnRefresh: StoryObj<SplitPanesProps> = {
    ...SplitPanesStory,
    args: {
        direction: 'horizontal',
        autoSaveId: 'split-panes',
    },
};
