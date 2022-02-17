import React, { useState } from 'react';
import { useNavigate } from 'react-router-dom';

import styles from './Search.module.scss';

function Search() {
    const [input, setInput] = useState('');
    const navigate = useNavigate();
    return (
        <div className={styles.form}>
            <input
                className={styles.searchtext}
                type="text"
                id="search"
                placeholder="Search transactions by ID"
                value={input}
                onChange={(e: React.ChangeEvent<HTMLInputElement>) =>
                    setInput(e.currentTarget.value)
                }
            />
            <button
                className={styles.searchbtn}
                onClick={() =>
                    input.length < 60
                        ? navigate(`../search/${input}`)
                        : navigate(`../transactions/${input}`)
                }
                aria-label="search button"
            >
                Search
            </button>
        </div>
    );
}

export default Search;
