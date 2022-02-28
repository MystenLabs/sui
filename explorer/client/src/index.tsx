import React from 'react';
import ReactDOM from 'react-dom';
import { BrowserRouter as Router } from 'react-router-dom';

import App from './app/App';
import reportWebVitals from './utils/reportWebVitals';

import './index.scss';

let init = Promise.resolve();

if (
    process.env.NODE_ENV === 'development' &&
    process.env.REACT_APP_MOCK_API === 'true'
) {
    init = (async () => {
        (await import('./mocks/api/browser-mock')).worker.start({
            onUnhandledRequest: 'bypass',
        });
    })();
}

init.then(() => {
    ReactDOM.render(
        <React.StrictMode>
            <Router>
                <App />
            </Router>
        </React.StrictMode>,
        document.getElementById('root')
    );

    reportWebVitals();
});
