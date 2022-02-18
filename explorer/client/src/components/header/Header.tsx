import { Link } from 'react-router-dom';

import styles from './Header.module.css';

const Header = () => {
    return (
        <header>
            <nav className={styles.nav}>
                <Link to="/" aria-label="logo" className={styles.logo}>
                    Mysten Labs
                </Link>
            </nav>
        </header>
    );
};

export default Header;
