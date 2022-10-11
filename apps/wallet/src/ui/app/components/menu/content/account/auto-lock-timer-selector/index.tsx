// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Field, Formik, Form } from 'formik';
import { useEffect, useState } from 'react';
import Browser from 'webextension-polyfill';
import * as Yup from 'yup';

import Button from '_app/shared/button';
import InputWithAction from '_app/shared/input-with-action';
import Alert from '_components/alert';
import Loading from '_components/loading';
import {
    AUTO_LOCK_TIMER_DEFAULT_INTERVAL_MINUTES,
    AUTO_LOCK_TIMER_STORAGE_KEY,
} from '_src/shared/constants';

import st from './AutoLockTimerSelector.module.scss';

const MIN_TIMER_MINUTES = 1;
const MAX_TIMER_MINUTES = 30;

const validation = Yup.object({
    timer: Yup.number()
        .integer()
        .required()
        .min(MIN_TIMER_MINUTES)
        .max(MAX_TIMER_MINUTES)
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
    return (
        <Loading loading={timerMinutes === null}>
            <Formik
                initialValues={{ timer: timerMinutes }}
                validationSchema={validation}
                onSubmit={async ({ timer }) => {
                    await Browser.storage.local.set({
                        [AUTO_LOCK_TIMER_STORAGE_KEY]: timer,
                    });
                    setTimerMinutes(timer);
                }}
                enableReinitialize={true}
            >
                {({ dirty, isSubmitting, isValid, touched, errors }) => (
                    <Form>
                        <Field
                            component={InputWithAction}
                            name="timer"
                            min={MIN_TIMER_MINUTES}
                            max={MAX_TIMER_MINUTES}
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
