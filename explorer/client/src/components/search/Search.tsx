import React, { useState, useCallback } from 'react';
import { useNavigate } from 'react-router-dom';

import styles from './Search.module.css';

function Search() {
    const [input, setInput] = useState('');
    const navigate = useNavigate();

    const handleSubmit = useCallback( () => {
      input.length < 60
        ? navigate(`../search/${input}`)
        : navigate(`../transactions/${input}`)
    }, [input, navigate]);

    const handleTextChange = useCallback( () => (
      e: React.ChangeEvent<HTMLInputElement>
    ) => setInput(
        e.currentTarget.value
      )
    , [])

    return (
        <form 
          className={styles.form} 
          onSubmit={ handleSubmit }
          aria-label="search form"
        >
            <input
                className={styles.searchtext}
                id="search"
                placeholder="Search transactions by ID"
                value={input}
                onChange={ handleTextChange() }
               type="text"
            />
            <input
                type="submit"
                value="Search"
                className={styles.searchbtn}
            />
        </form>
    );
}

export default Search;
