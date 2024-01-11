// Not a license :)

#[derive(thiserror::Error, Debug)]
pub enum SuiSnifferError {
    #[error(transparent)]
    SnifferError(#[from] mamoru_sniffer::SnifferError),

    #[error(transparent)]
    DataError(#[from] mamoru_sniffer::core::DataError),
}
