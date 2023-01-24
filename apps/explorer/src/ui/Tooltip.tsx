// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    useFloating,
    autoUpdate,
    offset,
    flip,
    shift,
    useHover,
    useFocus,
    useDismiss,
    useRole,
    useInteractions,
    FloatingPortal,
    arrow,
} from '@floating-ui/react';
import { AnimatePresence, motion } from 'framer-motion';
import { useRef, useState } from 'react';

import { ReactComponent as InfoSvg } from './icons/info.svg';

import type { Placement } from '@floating-ui/react';
import type { ReactNode, CSSProperties } from 'react';

const TOOLTIP_DELAY = 150;

interface TooltipProps {
    tip: ReactNode;
    children: ReactNode;
    placement?: Placement;
}

export function Tooltip({ tip, children, placement = 'top' }: TooltipProps) {
    const [open, setOpen] = useState(false);
    const arrowRef = useRef(null);

    const {
        x,
        y,
        reference,
        floating,
        strategy,
        context,
        middlewareData,
        placement: finalPlacement,
    } = useFloating({
        placement,
        open,
        onOpenChange: setOpen,
        whileElementsMounted: autoUpdate,
        middleware: [
            offset(5),
            flip(),
            shift(),
            arrow({ element: arrowRef, padding: 6 }),
        ],
    });

    const { getReferenceProps, getFloatingProps } = useInteractions([
        useHover(context, { move: true, delay: TOOLTIP_DELAY }),
        useFocus(context),
        useRole(context, { role: 'tooltip' }),
        useDismiss(context),
    ]);

    const animateProperty =
        finalPlacement.startsWith('top') || finalPlacement.startsWith('bottom')
            ? 'y'
            : 'x';

    const animateValue =
        finalPlacement.startsWith('bottom') ||
        finalPlacement.startsWith('right')
            ? 'calc(-50% - 15px)'
            : 'calc(50% + 15px)';

    const arrowStyle: CSSProperties = {
        left: middlewareData.arrow?.x,
        top: middlewareData.arrow?.y,
    };

    const staticSide = (
        {
            top: 'bottom',
            right: 'left',
            bottom: 'top',
            left: 'right',
        } as const
    )[finalPlacement.split('-')[0]];

    if (staticSide) {
        arrowStyle[staticSide] = '-3px';
    }

    return (
        <>
            <div
                tabIndex={0}
                className="w-fit"
                {...getReferenceProps({ ref: reference })}
            >
                {children}
            </div>
            <FloatingPortal>
                <AnimatePresence>
                    {open ? (
                        <motion.div
                            className="pointer-events-none left-0 top-0 z-50 text-subtitleSmall font-semibold text-white"
                            initial={{
                                opacity: 0,
                                scale: 0,
                                [animateProperty]: animateValue,
                            }}
                            animate={{
                                opacity: 1,
                                scale: 1,
                                [animateProperty]: 0,
                            }}
                            exit={{
                                opacity: 0,
                                scale: 0,
                                [animateProperty]: animateValue,
                            }}
                            transition={{
                                duration: 0.3,
                                ease: 'anticipate',
                            }}
                            style={{
                                position: strategy,
                                top: y ?? 0,
                                left: x ?? 0,
                                width: 'max-content',
                                maxWidth: '200px',
                            }}
                            {...getFloatingProps({ ref: floating })}
                        >
                            <div className="leading-1 flex flex-col flex-nowrap gap-px rounded-md bg-gray-90 p-2 leading-130">
                                {tip}
                            </div>
                            <div
                                ref={arrowRef}
                                className="absolute z-[-1] h-[12px] w-[12px] rotate-45 transform bg-gray-100"
                                style={arrowStyle}
                            />
                        </motion.div>
                    ) : null}
                </AnimatePresence>
            </FloatingPortal>
        </>
    );
}

export type IconTooltipProps = Omit<TooltipProps, 'children'>;

export function IconTooltip(props: IconTooltipProps) {
    return (
        <Tooltip {...props}>
            <InfoSvg />
        </Tooltip>
    );
}
