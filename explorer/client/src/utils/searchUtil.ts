let navigateWithUnknown: Function;

if (process.env.REACT_APP_DATA === 'static') {
    import('./static/searchUtil').then(
        (uf) => (navigateWithUnknown = uf.navigateWithUnknown)
    );
} else {
    import('./internetapi/searchUtil').then(
        (uf) => (navigateWithUnknown = uf.navigateWithUnknown)
    );
}

export { navigateWithUnknown };
