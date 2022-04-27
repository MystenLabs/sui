// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import logo from '../assets/images/sui-icon.png';

import st from './App.module.scss';

const App = () => {
    return (
        <div className={st.container}>
            <img className={st.logo} src={logo} alt="logo" />
            <h2>Under Construction</h2>
        </div>
    );
};

export default App;
