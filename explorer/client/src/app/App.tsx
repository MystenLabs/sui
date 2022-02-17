import Footer from '../footer/Footer';
import Header from '../header/Header';
import AppRoutes from '../pages/config/AppRoutes';
import Search from '../pages/search/Search';
import styles from './App.module.scss';

function App() {
    return (
        <div className={styles.app}>
            <Header />
            <div className={styles.search}>
                <h2>The Sui Explorer</h2>
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
