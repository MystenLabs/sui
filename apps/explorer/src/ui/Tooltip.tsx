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
} from '@floating-ui/react-dom-interactions';
import { AnimatePresence, motion } from 'framer-motion';
import { cloneElement, isValidElement, useMemo, useRef, useState } from 'react';

import { ReactComponent as InfoSvg } from './icons/info.svg';

import type { Placement } from '@floating-ui/react-dom-interactions';
import type { ReactNode } from 'react';

export type UseTooltipStateProps = {
    initialOpen: boolean;
    placement?: Placement;
    showArrow?: Boolean;
};

export function useTooltipState({
    initialOpen = false,
    placement = 'top',
    showArrow = true,
}: UseTooltipStateProps) {
    const [open, setOpen] = useState(initialOpen);
    const arrowRef = useRef(null);
    const middleware = [offset(5), flip(), shift()];
    if (showArrow) {
        middleware.push(arrow({ element: arrowRef, padding: 6 }));
    }
    const data = useFloating({
        placement,
        open,
        onOpenChange: setOpen,
        whileElementsMounted: autoUpdate,
        middleware,
    });
    const { context } = data;
    const hover = useHover(context, { move: true });
    const focus = useFocus(context);
    const dismiss = useDismiss(context);
    const role = useRole(context, { role: 'tooltip' });
    const interactions = useInteractions([hover, focus, dismiss, role]);
    return useMemo(
        () => ({
            open,
            setOpen,
            showArrow,
            arrowRef,
            ...interactions,
            ...data,
        }),
        [open, setOpen, interactions, data, showArrow]
    );
}

type TooltipAnchorProps = {
    tooltipState: ReturnType<typeof useTooltipState>;
    children: ReactNode;
};

export function TooltipAnchor({ tooltipState, children }: TooltipAnchorProps) {
    if (isValidElement(children)) {
        return cloneElement(children, {
            ...tooltipState.getReferenceProps({ ref: tooltipState.reference }),
        });
    }
    return null;
}

export type TooltipContentProps = {
    tooltipState: ReturnType<typeof useTooltipState>;
    children: ReactNode;
};

export function TooltipContent({
    tooltipState,
    children,
}: TooltipContentProps) {
    const {
        floating: ref,
        open,
        getFloatingProps,
        strategy,
        y,
        x,
        placement,
        showArrow,
        arrowRef,
    } = tooltipState;
    const animateProperty =
        placement.startsWith('top') || placement.startsWith('bottom')
            ? 'y'
            : 'x';
    const animateValue =
        placement.startsWith('bottom') || placement.startsWith('right')
            ? 'calc(-50% - 15px)'
            : 'calc(50% + 15px)';
    const { arrow } = tooltipState.middlewareData;
    const staticSide = {
        top: 'bottom',
        right: 'left',
        bottom: 'top',
        left: 'right',
    }[placement.split('-')[0]];
    const arrowStyle: Record<string, string> = arrow
        ? {
              left: arrow.x !== null ? `${arrow.x}px` : '',
              top: arrow.y !== null ? `${arrow.y}px` : '',
              right: '',
              bottom: '',
          }
        : {};
    if (staticSide && arrow) {
        arrowStyle[staticSide] = '-3px';
    }
    return (
        <FloatingPortal id="tooltips-container-portal">
            <AnimatePresence>
                {open ? (
                    <motion.div
                        className="left-0 top-0 z-50 pointer-events-none text-white text-subtitleSmall font-semibold"
                        initial={{
                            opacity: 0,
                            scale: 0,
                            [animateProperty]: animateValue,
                        }}
                        animate={{ opacity: 1, scale: 1, [animateProperty]: 0 }}
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
                            // Positioning styles
                            position: strategy,
                            top: y ?? 0,
                            left: x ?? 0,
                            width: 'max-content',
                        }}
                        {...getFloatingProps({ ref })}
                    >
                        <div className="bg-sui-grey-100 p-2 flex flex-col flex-nowrap gap-px rounded-md">
                            {children}
                        </div>
                        {showArrow ? (
                            <div
                                ref={arrowRef}
                                className="absolute z-[-1] bg-sui-grey-100 rotate-45 h-[12px] w-[12px]"
                                style={{
                                    transform: 'rotate(45deg)', // ¯\_(ツ)_/¯
                                    ...arrowStyle,
                                }}
                            />
                        ) : null}
                    </motion.div>
                ) : null}
            </AnimatePresence>
        </FloatingPortal>
    );
}

export type IconTooltipProps = Omit<UseTooltipStateProps, 'initialOpen'> & {
    initialOpen?: UseTooltipStateProps['initialOpen'];
    children: ReactNode;
};

export function IconTooltip({
    children,
    initialOpen = false,
    placement = 'top',
}: IconTooltipProps) {
    const state = useTooltipState({ initialOpen, placement });
    return (
        <>
            <TooltipAnchor tooltipState={state}>
                <div className="inline-block">
                    <InfoSvg />
                </div>
            </TooltipAnchor>
            <TooltipContent tooltipState={state}>{children}</TooltipContent>
        </>
    );
}
