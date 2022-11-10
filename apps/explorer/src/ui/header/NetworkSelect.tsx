// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    autoUpdate,
    flip,
    offset,
    shift,
    useFloating,
} from '@floating-ui/react-dom-interactions';
import { Menu, Transition } from '@headlessui/react';
import clsx from 'clsx';
import { AnimatePresence, motion } from 'framer-motion';
import { ComponentProps, forwardRef, Fragment, ReactNode, useState } from 'react';

import { ReactComponent as CheckIcon } from '../icons/check_16x16.svg';
import { ReactComponent as ChevronDownIcon } from '../icons/chevron_down.svg';
import { NavItem } from './NavItem';

export interface NetworkSelectProps {
    networks: string[];
    value: string;
    onChange(network: string): void;
}

enum NetworkState {
    UNSELECTED = 'UNSELECTED',
    PENDING = 'PENDING',
    SELECTED = 'SELECTED',
}

interface SelectableNetworkProps extends ComponentProps<'button'> {
    state?: NetworkState;
    children: ReactNode;
    onClick(): void;
}

const SelectableNetwork = forwardRef<HTMLButtonElement, SelectableNetworkProps>(
    ({ state = NetworkState, children, onClick, ...props }, ref) => {
        return (
            <button
                ref={ref}
                type="button"
                onClick={onClick}
                className={clsx(
                    // CSS Reset:
                    'cursor-pointer border-0 bg-transparent text-left',
                    'flex items-start gap-4 px-2 py-3 text-body font-semibold rounded-md transition hover:text-sui-grey-90 ui-active:text-sui-grey-90 hover:bg-sui-grey-40 ui-active:bg-sui-grey-40',
                    state !== NetworkState.UNSELECTED
                        ? 'text-sui-grey-90'
                        : 'text-sui-grey-75'
                )}
                {...props}
            >
                <CheckIcon
                    className={clsx('flex-shrink-0 transition', {
                        'text-success': state === NetworkState.SELECTED,
                        'text-sui-grey-60': state === NetworkState.PENDING,
                        'text-sui-grey-45': state === NetworkState.UNSELECTED,
                    })}
                />
                <div className="mt-px">{children}</div>
            </button>
        );
    }
);

function CustomRPCInput() {
    const [value, setValue] = useState('');

    return (
        <form
            onSubmit={(e) => {
                e.preventDefault();
                e.stopPropagation();
                console.log('hi');
            }}
            className="relative flex items-center rounded-md"
        >
            <input
                type="text"
                name="search"
                className="block w-full rounded-md border-sui-grey-65 border border-solid shadow-sm outline-none text-sui-grey-90 p-3 pr-16"
                onInput={(e) => e.preventDefault()}
            />

            <div className="absolute inset-y-0 right-0 flex flex-col justify-center px-3">
                <button
                    type="submit"
                    className="text-white uppercase text-captionSmall font-semibold rounded-full px-2 py-1 bg-sui-grey-90 flex items-center justify-center border-0"
                >
                    Save
                </button>
            </div>
        </form>
    );
}

function NetworkSelectPanel({ networks, onChange, value }: NetworkSelectProps) {
    const [customOpen, setCustomOpen] = useState(false);

    return (
        <>
            {networks.map((network) => (
                <Menu.Item key={network}>
                    <SelectableNetwork
                        state={
                            !customOpen && value === network
                                ? NetworkState.SELECTED
                                : NetworkState.UNSELECTED
                        }
                        onClick={() => onChange(network)}
                    >
                        {network}
                    </SelectableNetwork>
                </Menu.Item>
            ))}

            <SelectableNetwork
                state={
                    customOpen ? NetworkState.PENDING : NetworkState.UNSELECTED
                }
                onClick={() => setCustomOpen(true)}
            >
                Custom RPC URL
                {customOpen && (
                    <div className="mt-3">
                        <CustomRPCInput />
                    </div>
                )}
            </SelectableNetwork>
        </>
    );
}

// TODO: Handle incoming custom RPC.
// TODO: Handle invalid custom RPC.
// TODO: Handle save custom RPC.
export function NetworkSelect(props: NetworkSelectProps) {
    const { x, y, reference, floating, strategy } = useFloating({
        placement: 'bottom-end',
        middleware: [offset(5), flip(), shift()],
        whileElementsMounted: autoUpdate,
    });

    return (
        <Menu>
            {({ open }) => (
                <>
                    <Menu.Button
                        ref={reference}
                        as={NavItem}
                        afterIcon={<ChevronDownIcon />}
                    >
                        {props.value}
                    </Menu.Button>
                    <AnimatePresence>
                        {open && (
                            <Menu.Items
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
                                className="gap-3 flex flex-col w-56 rounded-lg bg-white shadow-lg focus:outline-none p-4"
                                style={{
                                    position: strategy,
                                    top: y ?? 0,
                                    left: x ?? 0,
                                }}
                            >
                                <NetworkSelectPanel {...props} />
                            </Menu.Items>
                        )}
                    </AnimatePresence>
                </>
            )}
        </Menu>
    );
}
