// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Field, Formik, Form } from 'formik';
import { toast } from 'react-hot-toast';
import * as Yup from 'yup';

import InputWithAction from '_app/shared/input-with-action';
import { setKeyringLockTimeout } from '_app/wallet/actions';
import Alert from '_components/alert';
import Loading from '_components/loading';
import { useAppDispatch } from '_hooks';
import {
    AUTO_LOCK_TIMER_MIN_MINUTES,
    AUTO_LOCK_TIMER_MAX_MINUTES,
} from '_src/shared/constants';
import { useAutoLockInterval } from '_src/ui/app/hooks/useAutoLockInterval';
import { Pill } from '_src/ui/app/shared/Pill';

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
                            await dispatch(
                                setKeyringLockTimeout({ timeout: timer })
                            ).unwrap();
                            toast.success('Auto lock updated');
                        } catch (e) {
                            // log it?
                        }
                    }
                }}
                enableReinitialize={true}
            >
                {({ dirty, isSubmitting, isValid, touched, errors }) => (
                    <Form>
                        <Field
                            component={InputWithAction}
                            name="timer"
                            min={AUTO_LOCK_TIMER_MIN_MINUTES}
                            max={AUTO_LOCK_TIMER_MAX_MINUTES}
                            step="1"
                            disabled={isSubmitting}
                        >
                            <Pill
                                text="Save"
                                type="submit"
                                disabled={!dirty || !isValid}
                                loading={isSubmitting}
                            />
                        </Field>
                        {touched.timer && errors.timer ? (
                            <div className="mt-1.25">
                                <Alert>{errors.timer}</Alert>
                            </div>
                        ) : null}
                    </Form>
                )}
            </Formik>
        </Loading>
    );
}
