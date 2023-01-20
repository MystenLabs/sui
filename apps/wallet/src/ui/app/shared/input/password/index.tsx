// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react';

import Icon, { SuiIcons } from '_components/icon';

import type { FieldProps } from 'formik';

export type PasswordInputProps = FieldProps;

function PasswordInput({ field, meta, form, ...props }: PasswordInputProps) {
    const [passwordShown, setPasswordShown] = useState(false);
    return (
        <div className="flex w-full relative items-center">
            <input
                type={passwordShown ? 'text' : 'password'}
                {...field}
                {...props}
                className={
                    'peer h-11 w-full text-body text-steel-dark font-medium flex items-center gap-5 bg-white py-2.5 pr-0 pl-3 border border-solid  border-gray-45 rounded-2lg shadow-button focus:border-steel focus:shadow-none placeholder-gray-65'
                }
                placeholder="Password"
            />
            <Icon
                icon={SuiIcons[passwordShown ? 'ShowPassword' : 'HidePassword']}
                className="absolute text-heading6 font-normal text-gray-60 cursor-pointer right-3 peer-focus:text-steel"
                onClick={() => setPasswordShown(!passwordShown)}
            />
        </div>
    );
}

export default PasswordInput;
