// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Dialog, Transition } from '@headlessui/react';
import { Fragment, ReactNode } from 'react';

export function ModalBase({
	isOpen,
	closeModal,
	children,
	title = 'A modal',
}: {
	isOpen: boolean;
	closeModal: () => void;
	children: ReactNode;
	title: string;
}) {
	return (
		<Transition appear show={isOpen} as={Fragment}>
			<Dialog as="div" className="fixed inset-0 z-10 overflow-y-auto" onClose={closeModal}>
				<div className="min-h-screen px-4 text-center bg-primary bg-opacity-60">
					<Transition.Child
						as={Fragment}
						enter="ease-out duration-300"
						enterFrom="opacity-0"
						enterTo="opacity-100"
						leave="ease-in duration-200"
						leaveFrom="opacity-100"
						leaveTo="opacity-0"
					>
						<Dialog.Overlay className="fixed inset-0" />
					</Transition.Child>

					<span className="inline-block h-screen align-middle" aria-hidden="true">
						&#8203;
					</span>
					<Transition.Child
						as={Fragment}
						enter="ease-out duration-300"
						enterFrom="opacity-0 scale-95"
						enterTo="opacity-100 scale-100"
						leave="ease-in duration-200"
						leaveFrom="opacity-100 scale-100"
						leaveTo="opacity-0 scale-95"
					>
						<div className="inline-block w-full max-w-md p-6 my-8 overflow-hidden text-left align-middle transition-all transform bg-white shadow-xl rounded-lg">
							<Dialog.Title as="h3" className="text-lg font-medium leading-6 text-gray-900">
								{title}
							</Dialog.Title>
							<div className="mt-2">{children}</div>
						</div>
					</Transition.Child>
				</div>
			</Dialog>
		</Transition>
	);
}
