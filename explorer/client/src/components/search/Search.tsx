import React, { useState, useCallback } from 'react';
import { useNavigate } from 'react-router-dom';

import { navigateWithUnknown } from '../../utils/utility_functions';

import styles from './Search.module.css';

function Search() {
    const [input, setInput] = useState('');
    const navigate = useNavigate();

    const handleSubmit = useCallback(
        (e: React.FormEvent<HTMLFormElement>) => {
            e.preventDefault();
            navigateWithUnknown(input, navigate);
            setInput('');
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
            <input type="submit" value="Search" className={styles.searchbtn} />
        </form>
    );
}

export default Search;
