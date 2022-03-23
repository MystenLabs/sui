import Footer from '../components/footer/Footer';
import Search from '../components/search/Search';
import AppRoutes from '../pages/config/AppRoutes';

import styles from './App.module.css';

function App() {
    return (
        <div className={styles.app}>
            <div className={styles.search}>
                <h2 className={styles.suititle}>SuiExplorer</h2>
                <Search />
            </div>
            <main>
                <AppRoutes />
            </main>
            <Footer />
        </div>
    );
}

export default App;
