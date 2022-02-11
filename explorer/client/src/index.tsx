import React from 'react';
import ReactDOM from 'react-dom';

import App from './app/App';
import reportWebVitals from './utils/reportWebVitals';

import './index.scss';

ReactDOM.render(
    <React.StrictMode>
        <App />
    </React.StrictMode>,
    document.getElementById('root')
);

reportWebVitals();
