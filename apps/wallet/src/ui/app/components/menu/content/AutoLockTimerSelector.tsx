// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Formik, Form } from 'formik';
import { toast } from 'react-hot-toast';
import * as Yup from 'yup';

import { setKeyringLockTimeout } from '_app/wallet/actions';
import Loading from '_components/loading';
import { useAppDispatch } from '_hooks';
import { AUTO_LOCK_TIMER_MIN_MINUTES, AUTO_LOCK_TIMER_MAX_MINUTES } from '_src/shared/constants';
import { useAutoLockInterval } from '_src/ui/app/hooks/useAutoLockInterval';
import { InputWithAction } from '_src/ui/app/shared/InputWithAction';

const validation = Yup.object({
	timer: Yup.number()
		.integer()
		.required()
		.min(AUTO_LOCK_TIMER_MIN_MINUTES)
		.max(AUTO_LOCK_TIMER_MAX_MINUTES)
		.label('Auto-lock timer'),
});

export default function AutoLockTimerSelector() {
	const dispatch = useAppDispatch();
	const autoLockInterval = useAutoLockInterval();
	return (
		<Loading loading={autoLockInterval === null}>
			<Formik
				initialValues={{ timer: autoLockInterval }}
				validationSchema={validation}
				onSubmit={async ({ timer }) => {
					if (timer !== null) {
						try {
							await dispatch(setKeyringLockTimeout({ timeout: timer })).unwrap();
							toast.success('Auto lock updated');
						} catch (e) {
							// log it?
						}
					}
				}}
				enableReinitialize={true}
			>
				<Form>
					<InputWithAction
						type="number"
						name="timer"
						min={AUTO_LOCK_TIMER_MIN_MINUTES}
						max={AUTO_LOCK_TIMER_MAX_MINUTES}
						step="1"
						actionDisabled="auto"
						actionText="Save"
						placeholder="Auto lock minutes"
					/>
				</Form>
			</Formik>
		</Loading>
	);
}
