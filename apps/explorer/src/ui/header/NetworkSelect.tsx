// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * This is an App UI Component, which is responsible for network selection.
 * It's as context un-aware as it reasonably can be, being a controlled component.
 * In the future, this should move outside of the base `~/ui/` directory, but for
 * now we are including App UI Components in the base UI component directory.
 */

import {
    autoUpdate,
    flip,
    FloatingPortal,
    offset,
    shift,
    useFloating,
} from '@floating-ui/react-dom-interactions';
import { Popover } from '@headlessui/react';
import { zodResolver } from '@hookform/resolvers/zod';
import clsx from 'clsx';
import { AnimatePresence, motion } from 'framer-motion';
import { useEffect, useState } from 'react';
import { useForm } from 'react-hook-form';
import { z } from 'zod';

import { ReactComponent as CheckIcon } from '../icons/check_16x16.svg';
import { ReactComponent as ChevronDownIcon } from '../icons/chevron_down.svg';
import { ReactComponent as MenuIcon } from '../icons/menu.svg';
import { NavItem } from './NavItem';

import type { ComponentProps, ReactNode } from 'react';

export interface NetworkOption {
    id: string;
    label: string;
}

export interface NetworkSelectProps {
    networks: NetworkOption[];
    value: string;
    onChange(networkId: string): void;
}

enum NetworkState {
    UNSELECTED = 'UNSELECTED',
    PENDING = 'PENDING',
    SELECTED = 'SELECTED',
}

interface SelectableNetworkProps extends ComponentProps<'div'> {
    state: NetworkState;
    children: ReactNode;
    onClick(): void;
}

function SelectableNetwork({
    state,
    children,
    onClick,
    ...props
}: SelectableNetworkProps) {
    return (
        <div
            role="button"
            onClick={onClick}
            className={clsx(
                // CSS Reset:
                'cursor-pointer border-0 bg-transparent text-left',
                'flex items-start gap-4 px-2 py-3 text-body font-semibold rounded-md hover:text-gray-90 ui-active:text-gray-90 hover:bg-gray-40 ui-active:bg-gray-40',
                state !== NetworkState.UNSELECTED
                    ? 'text-gray-90'
                    : 'text-gray-75'
            )}
            {...props}
        >
            <CheckIcon
                className={clsx('flex-shrink-0', {
                    'text-success': state === NetworkState.SELECTED,
                    'text-gray-60': state === NetworkState.PENDING,
                    'text-gray-45': state === NetworkState.UNSELECTED,
                })}
            />
            <div className="mt-px">{children}</div>
        </div>
    );
}

const CustomRPCSchema = z.object({
    url: z.string().url(),
});

function CustomRPCInput({
    value,
    onChange,
}: {
    value: string;
    onChange(networkUrl: string): void;
}) {
    // TODO: Generalize into `useZodForm`.
    const { register, handleSubmit, formState } = useForm<
        z.infer<typeof CustomRPCSchema>
    >({
        mode: 'all',
        resolver: zodResolver(CustomRPCSchema),
        defaultValues: {
            url: value,
        },
    });

    return (
        <form
            onSubmit={handleSubmit((values) => {
                onChange(values.url);
            })}
            className="relative flex items-center rounded-md"
        >
            <input
                {...register('url')}
                type="text"
                className={clsx(
                    'block w-full rounded-md border border-solid shadow-sm outline-none p-3 pr-16',
                    formState.errors.url
                        ? 'border-issue-dark text-issue-dark'
                        : 'border-gray-65 text-gray-90'
                )}
            />

            <div className="absolute inset-y-0 right-0 flex flex-col justify-center px-3">
                <button
                    disabled={!formState.isDirty || !formState.isValid}
                    type="submit"
                    className="text-white uppercase text-captionSmall font-semibold rounded-full px-2 py-1 bg-gray-90 flex items-center justify-center border-0 transition disabled:bg-gray-45 disabled:text-gray-65"
                >
                    Save
                </button>
            </div>
        </form>
    );
}

function NetworkSelectPanel({ networks, onChange, value }: NetworkSelectProps) {
    const isCustomNetwork = !networks.find(({ id }) => id === value);
    const [customOpen, setCustomOpen] = useState(isCustomNetwork);

    useEffect(() => {
        setCustomOpen(isCustomNetwork);
    }, [isCustomNetwork]);

    return (
        <>
            {networks.map((network) => (
                <SelectableNetwork
                    key={network.id}
                    state={
                        !customOpen && value === network.id
                            ? NetworkState.SELECTED
                            : NetworkState.UNSELECTED
                    }
                    onClick={() => {
                        onChange(network.id);
                    }}
                >
                    {network.label}
                </SelectableNetwork>
            ))}

            <SelectableNetwork
                state={
                    isCustomNetwork
                        ? NetworkState.SELECTED
                        : customOpen
                        ? NetworkState.PENDING
                        : NetworkState.UNSELECTED
                }
                onClick={() => setCustomOpen(true)}
            >
                Custom RPC URL
                {customOpen && (
                    <div className="mt-3">
                        <CustomRPCInput
                            value={isCustomNetwork ? value : ''}
                            onChange={onChange}
                        />
                    </div>
                )}
            </SelectableNetwork>
        </>
    );
}

function ResponsiveIcon() {
    return (
        <div>
            <ChevronDownIcon className="hidden md:block" />
            <MenuIcon className="block md:hidden" />
        </div>
    );
}

export function NetworkSelect({
    networks,
    value,
    onChange,
}: NetworkSelectProps) {
    const { x, y, reference, floating, strategy } = useFloating({
        placement: 'bottom-end',
        middleware: [offset(5), flip(), shift()],
        whileElementsMounted: autoUpdate,
    });

    const selected = networks.find(({ id }) => id === value);

    return (
        <Popover>
            {({ open, close }) => (
                <>
                    <Popover.Button
                        ref={reference}
                        as={NavItem}
                        afterIcon={<ResponsiveIcon />}
                    >
                        <span className="hidden md:block">
                            {selected?.label || 'Custom'}
                        </span>
                    </Popover.Button>
                    <FloatingPortal>
                        <AnimatePresence>
                            {open && (
                                <Popover.Panel
                                    static
                                    ref={floating}
                                    as={motion.div}
                                    initial={{
                                        opacity: 0,
                                        scale: 0.95,
                                    }}
                                    animate={{
                                        opacity: 1,
                                        scale: 1,
                                    }}
                                    exit={{
                                        opacity: 0,
                                        scale: 0.95,
                                    }}
                                    transition={{ duration: 0.15 }}
                                    className="z-10 gap-3 flex flex-col w-64 rounded-lg bg-white shadow-lg focus:outline-none p-4"
                                    style={{
                                        position: strategy,
                                        top: y ?? 0,
                                        left: x ?? 0,
                                    }}
                                >
                                    <NetworkSelectPanel
                                        networks={networks}
                                        value={value}
                                        onChange={(network) => {
                                            onChange(network);
                                            close();
                                        }}
                                    />
                                </Popover.Panel>
                            )}
                        </AnimatePresence>
                    </FloatingPortal>
                </>
            )}
        </Popover>
    );
}
