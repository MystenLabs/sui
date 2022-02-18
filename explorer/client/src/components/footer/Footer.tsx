import { Link } from 'react-router-dom';

import styles from './Footer.module.css';

function Footer() {
    return (
        <footer className={styles.footer}>
            <nav className={styles.links}>
                <Link to="/" aria-label="home button">
                    Home
                </Link>
                <a
                    href="https://mystenlabs.com/"
                    target="_blank"
                    rel="noreferrer noopener"
                >
                    Mysten Labs
                </a>
                <a
                    href="https://devportal-30dd0.web.app/"
                    target="_blank"
                    rel="noreferrer noopener"
                >
                    Developer Hub
                </a>
            </nav>
        </footer>
    );
}

export default Footer;
