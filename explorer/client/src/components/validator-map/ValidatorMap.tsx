// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useTooltip, useTooltipInPortal } from '@visx/tooltip';
import React, { useCallback } from 'react';

import { WorldMap } from './WorldMap';

export function ValidatorMap() {
    const {
        tooltipData,
        tooltipLeft,
        tooltipTop,
        tooltipOpen,
        showTooltip,
        hideTooltip,
    } = useTooltip();

    // If you don't want to use a Portal, simply replace `TooltipInPortal` below with
    // `Tooltip` or `TooltipWithBounds` and remove `containerRef`
    const { containerRef, TooltipInPortal } = useTooltipInPortal({
        // use TooltipWithBounds
        detectBounds: true,
        // when tooltip containers are scrolled, this will correctly update the Tooltip position
        scroll: true,
    });

    const handleMouseOver = useCallback(
        (event: React.MouseEvent) => {
            showTooltip({
                tooltipLeft: event.clientX,
                tooltipTop: event.clientY,
                tooltipData: event.currentTarget.getAttribute('name') || '',
            });
        },
        [showTooltip]
    );

    return (
        <div>
            <WorldMap
                ref={containerRef}
                onMouseOver={handleMouseOver}
                onMouseOut={hideTooltip}
                width={500}
                height={500}
                nodes={[]}
            />

            {tooltipOpen && (
                <TooltipInPortal
                    // set this to random so it correctly updates with parent bounds
                    // key={Math.random()}
                    top={tooltipTop}
                    left={tooltipLeft}
                >
                    Data value <strong>{tooltipData}</strong>
                </TooltipInPortal>
            )}
        </div>
    );
}
