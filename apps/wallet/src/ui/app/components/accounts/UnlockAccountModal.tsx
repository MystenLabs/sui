// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useZodForm } from '@mysten/core';
import { z } from 'zod';
import { Link } from '../../shared/Link';
import { PasswordInput } from '../../shared/forms/controls/PasswordInput';
import { Button } from '_src/ui/app/shared/ButtonUI';
import {
	Dialog,
	DialogContent,
	DialogHeader,
	DialogFooter,
	DialogTitle,
	DialogDescription,
} from '_src/ui/app/shared/Dialog';

const formSchema = z.object({
	password: z.string().nonempty('Required'),
});

type FormValues = z.infer<typeof formSchema>;

type UnlockAccountModalProps = {
	onClose: () => void;
	onConfirm: () => void;
};

export function UnlockAccountModal({ onClose, onConfirm }: UnlockAccountModalProps) {
	const {
		register,
		handleSubmit,
		formState: { isSubmitting, isValid },
	} = useZodForm({
		mode: 'all',
		schema: formSchema,
		defaultValues: {
			password: '',
		},
	});
	const onSubmit = (formValues: FormValues) => {
		// eslint-disable-next-line no-console
		console.log('TODO: Do something when the user submits the form successfully', formValues);
		onConfirm();
	};

	return (
		<Dialog defaultOpen>
			<DialogContent>
				<DialogHeader>
					<DialogTitle>Enter Account Password</DialogTitle>
					<DialogDescription asChild>
						<span className="sr-only">Enter your account password to unlock your account</span>
					</DialogDescription>
				</DialogHeader>
				<form id="unlock-account-modal" onSubmit={handleSubmit(onSubmit)}>
					<label className="sr-only" htmlFor="password">
						Password
					</label>
					<PasswordInput {...register('password')} />
				</form>
				<DialogFooter>
					<div className="flex flex-col gap-3">
						<div className="flex gap-2.5">
							<Button variant="outline" size="tall" text="Cancel" onClick={() => onClose()} />
							<Button
								type="submit"
								form="unlock-account-modal"
								disabled={isSubmitting || !isValid}
								variant="primary"
								size="tall"
								loading={isSubmitting}
								text="Unlock"
							/>
						</div>
						<Link
							color="steelDark"
							weight="medium"
							size="bodySmall"
							text="Forgot Password?"
							to="/account/forgot-password"
						/>
					</div>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}
