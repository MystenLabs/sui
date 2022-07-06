// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import classnames from 'classnames/bind';

import st from './Logo.module.scss';

const cl = classnames.bind(st);

type LogoProps = {
    size?: 'normal' | 'big' | 'bigger' | 'huge';
    txt?: boolean;
    className?: string;
};

const Logo = ({ size = 'normal', txt = false, className }: LogoProps) => {
    return (
        <div className={cl('container', className)}>
            <span className={cl('image', size)} />
            {txt ? <span className={cl('txt', size)}>sui</span> : null}
        </div>
    );
};

export default Logo;
