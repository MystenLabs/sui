// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import cl from 'classnames';

import Icon, { SuiIcons } from '_components/icon';

import st from './ProgressBar.module.scss';

type Props = {
    currentStep: number;
    stepsName: string[];
    changeStep: (step: number) => void;
};

function ProgressBar({ currentStep, stepsName, changeStep }: Props) {
    const activeStep = currentStep - 1;
    return (
        <div className={st.progressBar}>
            {stepsName.map((step, index) => (
                <div
                    onClick={(_) => {
                        if (index !== activeStep) {
                            changeStep(index + 1);
                        }
                    }}
                    className={cl(
                        st.step,
                        index === activeStep && st.currentStep,
                        activeStep > index && st.completedStep
                    )}
                    key={index}
                >
                    <div className={st.stepIndex}>
                        {activeStep > index ? (
                            <Icon
                                icon={SuiIcons.Checkmark}
                                className={st.completedStepIcon}
                            />
                        ) : (
                            index + 1
                        )}
                    </div>
                    <div className={st.stepName}>{step}</div>
                </div>
            ))}
        </div>
    );
}

export default ProgressBar;
