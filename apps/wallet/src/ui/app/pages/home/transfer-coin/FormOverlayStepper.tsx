// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ArrowRight16, ArrowLeft16 } from '@mysten/icons';
import { Formik, type FormikConfig, type FormikValues } from 'formik';
import { useCallback, useState, type ReactElement } from 'react';
import { useNavigate } from 'react-router-dom';

import BottomMenuLayout, {
    Content,
    Menu,
} from '_app/shared/bottom-menu-layout';
import Button from '_app/shared/button';
import { SuiIcons } from '_components/icon';
import Loading from '_components/loading';
import Overlay from '_components/overlay';

export interface FormStepProps
    extends Pick<FormikConfig<FormikValues>, 'children' | 'validationSchema'> {
    label: string;
    loading?: boolean;
}

export function FormStep({ children }: FormStepProps) {
    return <>{children}</>;
}

export function FormOverlayStepper({
    children,
    ...props
}: FormikConfig<FormikValues>) {
    const [showModal, setShowModal] = useState(true);
    const navigate = useNavigate();
    const closeSendToken = useCallback(() => {
        navigate('/');
    }, [navigate]);
    const childrenArray = children as ReactElement<FormStepProps>[];
    const [step, setStep] = useState(0);
    const currentChild = childrenArray[step];

    function isLastStep() {
        return step === childrenArray.length - 1;
    }

    return (
        <Overlay
            showModal={showModal}
            setShowModal={setShowModal}
            title={currentChild.props.label}
            closeOverlay={closeSendToken}
            closeIcon={SuiIcons.Close}
        >
            <Loading loading={currentChild.props.loading || false}>
                <Formik
                    {...props}
                    validationSchema={currentChild.props.validationSchema}
                    onSubmit={async (values, helpers) => {
                        if (isLastStep()) {
                            await props.onSubmit(values, helpers);
                        } else {
                            setStep((s) => s + 1);

                            helpers.setTouched({});
                        }
                    }}
                >
                    {({ isSubmitting, isValid, submitForm }) => (
                        <BottomMenuLayout>
                            <Content>
                                <div className="flex flex-col gap-7.5 mt-3.75 w-full">
                                    {currentChild}
                                </div>
                            </Content>
                            <Menu
                                stuckClass="sendCoin-cta"
                                className="w-full px-0 pb-0 mx-0 gap-2.5"
                            >
                                {step > 0 ? (
                                    <Button
                                        type="button"
                                        mode="neutral"
                                        onClick={() => setStep(0)}
                                        className="w-full text-steel-darker"
                                    >
                                        <ArrowLeft16 /> Back
                                    </Button>
                                ) : null}
                                <Button
                                    type="submit"
                                    mode="primary"
                                    onClick={submitForm}
                                    disabled={!isValid || isSubmitting}
                                    className="w-full"
                                >
                                    {isLastStep() ? 'Send Now' : 'Review'}{' '}
                                    <ArrowRight16 />
                                </Button>
                            </Menu>
                        </BottomMenuLayout>
                    )}
                </Formik>
            </Loading>
        </Overlay>
    );
}
