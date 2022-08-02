// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';
import { ParentSizeModern } from '@visx/responsive';
import { useTooltip, useTooltipInPortal } from '@visx/tooltip';
import React, { useCallback } from 'react';

import { ReactComponent as ForwardArrowDark } from '../../assets/SVGIcons/forward-arrow-dark.svg';
import { WorldMap } from './WorldMap';

import styles from './ValidatorMap.module.css';

export function ValidatorMap() {
    const { data } = useQuery(['validator-map'], async () => {
        return [];
    });

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
        <div className={styles.card}>
            <div className={styles.container}>
                <div className={styles.contents}>
                    <div>
                        <div className={styles.title}>Total Nodes</div>
                        <div className={styles.stat}>15,123</div>
                    </div>
                    <div>
                        <div className={styles.title}>Average APY</div>
                        <div className={styles.stat}>4.96%</div>
                    </div>
                </div>

                <a
                    href="https://bit.ly/sui_validator"
                    className={styles.button}
                >
                    <span>Become a Validator</span>
                    <ForwardArrowDark />
                </a>
            </div>

            <div className={styles.mapcontainer}>
                <div className={styles.map}>
                    <ParentSizeModern>
                        {(parent) => (
                            <WorldMap
                                ref={containerRef}
                                onMouseOver={handleMouseOver}
                                onMouseOut={hideTooltip}
                                width={parent.width}
                                height={parent.height}
                                nodes={data}
                            />
                        )}
                    </ParentSizeModern>

                    {tooltipOpen && (
                        <TooltipInPortal top={tooltipTop} left={tooltipLeft}>
                            Data value <strong>{tooltipData}</strong>
                        </TooltipInPortal>
                    )}
                </div>
            </div>
        </div>
    );
}
