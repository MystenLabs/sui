// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useCallback } from 'react';
import { useNavigate } from 'react-router-dom';

import { ReactComponent as ContentBackArrowDark } from '../../assets/SVGIcons/back-arrow-dark.svg';

import styles from './GoBack.module.css';

export default function GoBack() {
    const navigate = useNavigate();
    const previousPage = useCallback(() => navigate(-1), [navigate]);

    return (
        <div className={styles.container}>
            <button className={styles.text} onClick={previousPage}>
                <ContentBackArrowDark /> Go Back
            </button>
        </div>
    );
}
