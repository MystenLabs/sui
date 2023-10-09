// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react';

import { Button, type ButtonProps } from './ButtonUI';
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from './Dialog';
import { Text } from './text';

export type ConfirmationModalProps = {
	isOpen: boolean;
	title?: string;
	hint?: string;
	confirmText?: string;
	confirmStyle?: ButtonProps['variant'];
	cancelText?: string;
	cancelStyle?: ButtonProps['variant'];
	onResponse: (confirmed: boolean) => void;
};

export function ConfirmationModal({
	isOpen,
	title = 'Are you sure?',
	hint,
	confirmText = 'Confirm',
	confirmStyle = 'primary',
	cancelText = 'Cancel',
	cancelStyle = 'outline',
	onResponse,
}: ConfirmationModalProps) {
	const [isConfirmLoading, setIsConfirmLoading] = useState(false);
	const [isCancelLoading, setIsCancelLoading] = useState(false);
	return (
		<Dialog
			open={isOpen}
			onOpenChange={async (open) => {
				if (open || isCancelLoading || isConfirmLoading) {
					return;
				}
				setIsCancelLoading(true);
				await onResponse(false);
				setIsCancelLoading(false);
			}}
		>
			<DialogContent>
				<DialogHeader>
					<DialogTitle>{title}</DialogTitle>
				</DialogHeader>
				{hint ? (
					<div className="break-words text-center">
						<Text variant="pBodySmall" color="steel-dark" weight="normal">
							{hint}
						</Text>
					</div>
				) : null}
				<DialogFooter>
					<div className="flex flex-row self-stretch gap-3">
						<Button
							variant={cancelStyle}
							size="tall"
							text={cancelText}
							loading={isCancelLoading}
							disabled={isConfirmLoading}
							onClick={async () => {
								setIsCancelLoading(true);
								await onResponse(false);
								setIsCancelLoading(false);
							}}
						/>
						<Button
							variant={confirmStyle}
							size="tall"
							text={confirmText}
							loading={isConfirmLoading}
							disabled={isCancelLoading}
							onClick={async () => {
								setIsConfirmLoading(true);
								await onResponse(true);
								setIsConfirmLoading(false);
							}}
						/>
					</div>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}
