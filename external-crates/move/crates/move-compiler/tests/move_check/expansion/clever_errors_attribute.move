module 0x42::m {
    // should only point to the first constant
    #[error(code = 10)]
    const E: bool = true;

    #[error(code = 20)]
    const F: bool = false;

    #[error(code = 20)]
    const G: bool = false;
}
