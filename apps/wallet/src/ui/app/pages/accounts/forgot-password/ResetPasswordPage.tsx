// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useAutoLockMinutesMutation } from '_src/ui/app/hooks/useAutoLockMinutesMutation';
import { useResetPasswordMutation } from '_src/ui/app/hooks/useResetPasswordMutation';
import { toast } from 'react-hot-toast';
import { Navigate, useNavigate } from 'react-router-dom';

import { ProtectAccountForm } from '../../../components/accounts/ProtectAccountForm';
import { autoLockDataToMinutes } from '../../../hooks/useAutoLockMinutes';
import { Heading } from '../../../shared/heading';
import { useForgotPasswordContext } from './ForgotPasswordPage';

export function ResetPasswordPage() {
	const { value, clear } = useForgotPasswordContext();
	const autoLockMutation = useAutoLockMinutesMutation();
	const resetPasswordMutation = useResetPasswordMutation();
	const navigate = useNavigate();
	if (!value.length && !resetPasswordMutation.isSuccess) {
		return <Navigate to="/accounts/forgot-password" replace />;
	}
	return (
		<div className="flex flex-col items-center h-full">
			<div className="text-center mt-2.5">
				<Heading variant="heading1" color="gray-90" as="h1" weight="bold">
					Protect Account with a Password Lock
				</Heading>
			</div>
			<div className="mt-6 w-full grow">
				<ProtectAccountForm
					cancelButtonText="Back"
					submitButtonText="Reset Password"
					onSubmit={async ({ password, autoLock }) => {
						try {
							await autoLockMutation.mutateAsync({ minutes: autoLockDataToMinutes(autoLock) });
							await resetPasswordMutation.mutateAsync({
								password: password.input,
								recoveryData: value,
							});
							clear();
							toast.success('Password reset');
							navigate('/');
						} catch (e) {
							toast.error((e as Error)?.message || 'Something went wrong');
						}
					}}
					displayToS
				/>
			</div>
		</div>
	);
}
