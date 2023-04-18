// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Dialog, Transition } from '@headlessui/react';
import { X12 } from '@mysten/icons';
import { Fragment, type ReactNode } from 'react';

import { Heading } from '../Heading';

export interface ModalProps {
    open: boolean;
    onClose: () => void;
    children: ReactNode;
}

export function CloseButton({ onClick }: { onClick: () => void }) {
    return (
        <button
            onClick={onClick}
            type="button"
            className="absolute right-0 top-0 p-4 text-steel hover:text-steel-darker"
        >
            <X12 />
        </button>
    );
}

export function ModalBody({ children }: { children: ReactNode }) {
    return <div className="py-5">{children}</div>;
}

export function ModalContent({ children }: { children: ReactNode }) {
    return (
        <div className="flex flex-col rounded-lg bg-gray-40 p-5">
            {children}
        </div>
    );
}

export function ModalHeading({ children }: { children: ReactNode }) {
    return (
        <Heading variant="heading3/semibold" color="gray-90">
            {children}
        </Heading>
    );
}

export function Modal({ open, onClose, children }: ModalProps) {
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
                                {children}
                            </div>
                        </Transition.Child>
                    </div>
                </div>
            </Dialog>
        </Transition>
    );
}
