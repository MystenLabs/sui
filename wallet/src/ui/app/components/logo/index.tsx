// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import classnames from 'classnames/bind';

import st from './Logo.module.scss';

const cl = classnames.bind(st);

type LogoProps = {
    size?: 'normal' | 'big' | 'bigger' | 'huge';
};

const Logo = ({ size = 'normal' }: LogoProps) => {
    return (
        <div className={cl('container')}>
            <span className={cl('image', size)} />
        </div>
    );
};

export default Logo;
