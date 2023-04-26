// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ChevronLeft12, ChevronUp12 } from '@mysten/icons';
import clsx from 'clsx';
import { type ReactNode, useRef, useState } from 'react';
import {
    type Direction,
    type ImperativePanelHandle,
    Panel,
    PanelGroup,
    type PanelGroupProps,
    type PanelProps,
    PanelResizeHandle,
} from 'react-resizable-panels';

interface ResizeHandleProps {
    isHorizontal: boolean;
    isCollapsed: boolean;
    togglePanelCollapse: () => void;
    collapsibleButton?: boolean;
}

function ResizeHandle({
    isHorizontal,
    isCollapsed,
    collapsibleButton,
    togglePanelCollapse,
}: ResizeHandleProps) {
    const [isDragging, setIsDragging] = useState(false);

    const ChevronButton = isHorizontal ? ChevronLeft12 : ChevronUp12;

    return (
        <PanelResizeHandle
            className={clsx('group/container', isHorizontal ? 'px-3' : 'py-3')}
            onDragging={setIsDragging}
        >
            <div
                className={clsx(
                    'relative border border-gray-45 group-hover/container:border-hero',
                    isHorizontal ? 'h-full w-px' : 'h-px'
                )}
            >
                {collapsibleButton && (
                    <button
                        type="button"
                        onClick={togglePanelCollapse}
                        data-is-dragging={isDragging}
                        className={clsx([
                            'group/button',
                            'flex h-6 w-6 cursor-pointer items-center justify-center rounded-full',
                            'border-2 border-gray-45 bg-white text-gray-70 group-hover/container:border-hero-dark',
                            'hover:bg-hero-dark hover:text-white',
                            isHorizontal
                                ? 'absolute left-1/2 top-[5%] -translate-x-2/4'
                                : 'absolute left-[5%] top-1/2 -translate-y-2/4',
                        ])}
                    >
                        <ChevronButton
                            height={12}
                            width={12}
                            className={clsx(
                                'text-gray-45 group-hover/button:text-white group-hover/container:text-hero',
                                isCollapsed && 'rotate-180'
                            )}
                        />
                    </button>
                )}
            </div>
        </PanelResizeHandle>
    );
}

interface SplitPanelProps extends PanelProps {
    panel: ReactNode;
    direction: Direction;
    renderResizeHandle: boolean;
    collapsibleButton?: boolean;
}

function SplitPanel({
    panel,
    direction,
    renderResizeHandle,
    collapsibleButton,
    ...props
}: SplitPanelProps) {
    const ref = useRef<ImperativePanelHandle>(null);
    const [isCollapsed, setIsCollapsed] = useState(false);

    const togglePanelCollapse = () => {
        const panelRef = ref.current;

        if (panelRef) {
            if (isCollapsed) {
                panelRef.expand();
            } else {
                panelRef.collapse();
            }
        }
    };

    return (
        <>
            <Panel {...props} ref={ref} onCollapse={setIsCollapsed}>
                {panel}
            </Panel>
            {renderResizeHandle && (
                <ResizeHandle
                    isCollapsed={isCollapsed}
                    isHorizontal={direction === 'horizontal'}
                    togglePanelCollapse={togglePanelCollapse}
                    collapsibleButton={collapsibleButton}
                />
            )}
        </>
    );
}

export interface SplitPanesProps extends PanelGroupProps {
    splitPanels: Omit<SplitPanelProps, 'renderResizeHandle' | 'direction'>[];
}

export function SplitPanes({ splitPanels, ...props }: SplitPanesProps) {
    const { direction } = props;

    return (
        <PanelGroup {...props}>
            {splitPanels.map((panel, index) => (
                <SplitPanel
                    key={index}
                    order={index}
                    renderResizeHandle={index < splitPanels.length - 1}
                    direction={direction}
                    {...panel}
                />
            ))}
        </PanelGroup>
    );
}
