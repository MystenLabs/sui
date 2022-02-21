import React, { useState } from 'react';
import { useNavigate } from 'react-router-dom';

import styles from './Search.module.css';

function Search() {
    const [input, setInput] = useState('');
    const navigate = useNavigate();

    const handleSubmit = (input : string) => {
      input.length < 60
        ? navigate(`../search/${input}`)
        : navigate(`../transactions/${input}`)
    }


    return (
        <form className={styles.form} onSubmit={() => handleSubmit(input)}>
            <input
                className={styles.searchtext}
                id="search"
                placeholder="Search transactions by ID"
                value={input}
                onChange={(e: React.ChangeEvent<HTMLInputElement>) =>
                    setInput(e.currentTarget.value)
                }
               type="text"
            />
            <input
                type="submit"
                value="Search"
                className={styles.searchbtn}
                aria-label="search button"
            />
        </form>
    );
}

export default Search;
