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
    noHoverHidden?: boolean;
}

function ResizeHandle({
    isHorizontal,
    isCollapsed,
    collapsibleButton,
    togglePanelCollapse,
    noHoverHidden,
}: ResizeHandleProps) {
    const [isDragging, setIsDragging] = useState(false);

    const ChevronButton = isHorizontal ? ChevronLeft12 : ChevronUp12;

    return (
        <PanelResizeHandle
            className={clsx(
                'group/container',
                isHorizontal
                    ? {
                          'px-3': !isCollapsed,
                          'px-6': isCollapsed,
                      }
                    : {
                          'py-3': !isCollapsed,
                          'py-6': isCollapsed,
                      }
            )}
            onDragging={setIsDragging}
        >
            <div
                className={clsx(
                    'relative border border-gray-45 group-hover/container:border-hero',
                    isHorizontal ? 'h-full w-px' : 'h-px',
                    noHoverHidden && !isCollapsed && 'border-transparent'
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
                                ? 'absolute left-1/2 top-10 -translate-x-2/4'
                                : 'absolute left-10 top-1/2 -translate-y-2/4',
                            noHoverHidden &&
                                !isCollapsed &&
                                'hidden group-hover/container:flex',
                        ])}
                    >
                        <ChevronButton
                            className={clsx(
                                'h-4 w-4 text-gray-45 group-hover/button:!text-white group-hover/container:text-hero',
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
    noHoverHidden?: boolean;
}

function SplitPanel({
    panel,
    direction,
    renderResizeHandle,
    collapsibleButton,
    noHoverHidden,
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
                    noHoverHidden={noHoverHidden}
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
