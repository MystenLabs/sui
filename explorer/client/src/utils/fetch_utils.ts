let navigateWithUnknown: Function;

if (process.env.REACT_APP_DATA === 'static') {
    import('./static/utility_functions').then(
        (uf) => (navigateWithUnknown = uf.navigateWithUnknown)
    );
} else {
    import('./internetapi/utility_functions').then(
        (uf) => (navigateWithUnknown = uf.navigateWithUnknown)
    );
}

export { navigateWithUnknown };
