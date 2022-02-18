import NavigationHeader from '../nav-header/NavigationHeader';
import AppRoutes from '../pages/config/AppRoutes';

function App() {
    return (
        <>
            <NavigationHeader />
            <main>
                <AppRoutes />
            </main>
        </>
    );
}

export default App;
