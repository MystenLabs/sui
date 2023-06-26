// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Dialog, Transition } from '@headlessui/react';
import { Fragment, type ReactNode, useEffect } from 'react';

import { Heading } from './heading';

export type ModalDialogProps = {
	isOpen: boolean;
	title?: ReactNode;
	body?: ReactNode;
	footer?: ReactNode;
	preventClose?: boolean;
	onClose: () => void;
};

export function ModalDialog({
	isOpen,
	title,
	body,
	footer,
	preventClose,
	onClose,
}: ModalDialogProps) {
	useEffect(() => {
		// we use escape for closing menu as well and closing Dialog with escape closes menu as well
		// fix this by capturing escape as early as possible
		const handler = (e: KeyboardEvent) => {
			if (e.key === 'Escape' && isOpen) {
				e.stopImmediatePropagation();
				e.preventDefault();
				if (!preventClose) {
					onClose();
				}
			}
		};
		if (isOpen) {
			window.addEventListener('keydown', handler, true);
		}
		return () => window.removeEventListener('keydown', handler, true);
	}, [onClose, isOpen, preventClose]);
	return (
		<Transition show={isOpen} as={Fragment}>
			<Dialog
				onClose={() => {
					if (!preventClose) {
						onClose();
					}
				}}
			>
				<Transition.Child
					as={Fragment}
					enter="ease-out duration-300"
					enterFrom="opacity-0"
					enterTo="opacity-100"
					leave="ease-in duration-200"
					leaveFrom="opacity-100"
					leaveTo="opacity-0"
				>
					<div className="fixed inset-0 bg-gray-95/10 backdrop-blur-lg z-[99998]" />
				</Transition.Child>
				<Transition.Child
					as={Fragment}
					enter="ease-out duration-300"
					enterFrom="opacity-0 scale-95"
					enterTo="opacity-100 scale-100"
					leave="ease-in duration-200"
					leaveFrom="opacity-100 scale-100"
					leaveTo="opacity-0 scale-95"
				>
					<div className="fixed inset-0 flex flex-col items-center justify-center z-[99999]">
						<Dialog.Panel className="shadow-wallet-modal bg-white py-6 rounded-xl w-80 max-w-[85vw] max-h-[60vh] overflow-hidden flex flex-col flex-nowrap items-stretch gap-1.5">
							{title ? (
								<div className="px-6 text-center">
									<Dialog.Title as={Heading} variant="heading6" weight="semibold" color="gray-90">
										{title}
									</Dialog.Title>
								</div>
							) : null}
							{body ? (
								<div className="flex flex-col flex-nowrap flex-1 items-stretch overflow-hidden overflow-y-auto px-6">
									{body}
								</div>
							) : null}
							{footer ? (
								<div className="flex flex-col flex-nowrap mt-4.5 px-6">{footer}</div>
							) : null}
						</Dialog.Panel>
					</div>
				</Transition.Child>
			</Dialog>
		</Transition>
	);
}
