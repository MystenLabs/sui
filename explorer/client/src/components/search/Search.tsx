import React, { useState, useCallback } from 'react';
import { useNavigate } from 'react-router-dom';

import { navigateWithUnknown } from '../../utils/searchUtil';

import styles from './Search.module.css';

function Search() {
    const [input, setInput] = useState('');
    const navigate = useNavigate();

    const [pleaseWaitMode, setPleaseWaitMode] = useState(false);

    const handleSubmit = useCallback(
        (e: React.FormEvent<HTMLFormElement>) => {
            e.preventDefault();
            setPleaseWaitMode(true);
            navigateWithUnknown(input, navigate).then(() => {
                setInput('');
                setPleaseWaitMode(false);
            });
        },
        [input, navigate, setInput]
    );

    const handleTextChange = useCallback(
        (e: React.ChangeEvent<HTMLInputElement>) =>
            setInput(e.currentTarget.value),
        [setInput]
    );

    return (
        <form
            className={styles.form}
            onSubmit={handleSubmit}
            aria-label="search form"
        >
            <input
                className={styles.searchtext}
                id="search"
                placeholder="Search by ID"
                value={input}
                onChange={handleTextChange}
                type="text"
            />
            <input
                type="submit"
                value={pleaseWaitMode ? 'Please Wait' : 'Search'}
                disabled={pleaseWaitMode}
                className={`${styles.searchbtn} ${
                    pleaseWaitMode && styles.disabled
                }`}
            />
        </form>
    );
}

export default Search;
