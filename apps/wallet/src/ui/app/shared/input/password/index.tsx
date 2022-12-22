// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { useState } from 'react';

import Icon, { SuiIcons } from '_components/icon';

import type { FieldProps } from 'formik';

import st from './PasswordInput.module.scss';

export type PasswordInputProps = FieldProps;

function PasswordInput({ field, meta, form, ...props }: PasswordInputProps) {
    const [passwordShown, setPasswordShown] = useState(false);
    return (
        <div className="flex w-full relative items-center">
            <input
                type={passwordShown ? 'text' : 'password'}
                {...field}
                {...props}
                className={cl(
                    'h-11 w-full text-body text-steel-dark font-medium',
                    st.input
                )}
                placeholder="Password"
            />
            <Icon
                icon={SuiIcons[passwordShown ? 'ShowPassword' : 'HidePassword']}
                className="absolute text-heading6 font-normal text-steel-dark cursor-pointer right-3"
                onClick={() => setPasswordShown(!passwordShown)}
            />
        </div>
    );
}

export default PasswordInput;
