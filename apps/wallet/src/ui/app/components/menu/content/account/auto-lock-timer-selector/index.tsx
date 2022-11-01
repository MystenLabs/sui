// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Field, Formik, Form } from 'formik';
import { useEffect, useState } from 'react';
import Browser from 'webextension-polyfill';
import * as Yup from 'yup';

import Button from '_app/shared/button';
import InputWithAction from '_app/shared/input-with-action';
import { setKeyringLockTimeout } from '_app/wallet/actions';
import Alert from '_components/alert';
import Loading from '_components/loading';
import { useAppDispatch } from '_hooks';
import {
    AUTO_LOCK_TIMER_DEFAULT_INTERVAL_MINUTES,
    AUTO_LOCK_TIMER_STORAGE_KEY,
    AUTO_LOCK_TIMER_MIN_MINUTES,
    AUTO_LOCK_TIMER_MAX_MINUTES,
} from '_src/shared/constants';

import st from './AutoLockTimerSelector.module.scss';

const validation = Yup.object({
    timer: Yup.number()
        .integer()
        .required()
        .min(AUTO_LOCK_TIMER_MIN_MINUTES)
        .max(AUTO_LOCK_TIMER_MAX_MINUTES)
        .label('Auto-lock timer'),
});

export default function AutoLockTimerSelector() {
    const [timerMinutes, setTimerMinutes] = useState<number | null>(null);
    useEffect(() => {
        Browser.storage.local
            .get({
                [AUTO_LOCK_TIMER_STORAGE_KEY]:
                    AUTO_LOCK_TIMER_DEFAULT_INTERVAL_MINUTES,
            })
            .then(({ [AUTO_LOCK_TIMER_STORAGE_KEY]: storedTimer }) =>
                setTimerMinutes(storedTimer)
            );
    }, []);
    const dispatch = useAppDispatch();
    return (
        <Loading loading={timerMinutes === null}>
            <Formik
                initialValues={{ timer: timerMinutes }}
                validationSchema={validation}
                onSubmit={async ({ timer }) => {
                    if (timer !== null) {
                        try {
                            await dispatch(
                                setKeyringLockTimeout({ timeout: timer })
                            ).unwrap();
                        } catch (e) {
                            // log it?
                        }
                    }
                    setTimerMinutes(timer);
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
                            <Button
                                type="submit"
                                disabled={!dirty || isSubmitting || !isValid}
                                size="mini"
                                className={st.action}
                            >
                                Save
                            </Button>
                        </Field>
                        {touched.timer && errors.timer ? (
                            <Alert className={st.error}>{errors.timer}</Alert>
                        ) : null}
                    </Form>
                )}
            </Formik>
        </Loading>
    );
}
