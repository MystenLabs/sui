// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Dialog, Transition } from '@headlessui/react';
import { X12 } from '@mysten/icons';
import { Fragment, type ReactNode } from 'react';

import { IconButton } from './IconButton';

interface LightBoxProps {
    open: boolean;
    onClose: () => void;
    children: ReactNode;
}

export function LightBox({ open, onClose, children }: LightBoxProps) {
    return (
        <Transition show={open} as={Fragment}>
            <Dialog className="relative z-50" open={open} onClose={onClose}>
                <Transition.Child
                    as={Fragment}
                    enter="ease-out duration-200"
                    enterFrom="opacity-0"
                    enterTo="opacity-100"
                    leave="ease-in duration-200"
                    leaveFrom="opacity-100"
                    leaveTo="opacity-0"
                >
                    <div
                        className="fixed inset-0 z-10 bg-gray-100/80"
                        aria-hidden="true"
                    />
                </Transition.Child>
                <div className="fixed inset-0 z-10 overflow-y-auto">
                    <div className="flex min-h-full items-center justify-center">
                        <Transition.Child
                            as={Fragment}
                            enter="ease-out duration-300"
                            enterFrom="opacity-0 scale-95"
                            enterTo="opacity-100 scale-100"
                            leave="ease-in duration-200"
                            leaveFrom="opacity-100 scale-100"
                            leaveTo="opacity-0 scale-95"
                        >
                            <div className="w-full max-w-xl transform align-middle transition-all">
                                <div className="absolute -right-12">
                                    <IconButton
                                        onClick={onClose}
                                        className="inline-flex h-8 w-8 cursor-pointer items-center justify-center rounded-full border-0 bg-gray-90 p-0 text-sui-light outline-none hover:scale-105 active:scale-100"
                                        aria-label="Close"
                                        icon={X12}
                                    />
                                </div>
                                {children}
                            </div>
                        </Transition.Child>
                    </div>
                </div>
            </Dialog>
        </Transition>
    );
}
