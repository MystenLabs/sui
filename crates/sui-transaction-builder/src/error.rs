use sui_types::base_types::{ObjectID, ObjectType, SuiAddress};
use sui_types::error::{ExecutionError, SuiError, SuiObjectResponseError, UserInputError};
use thiserror::Error;

pub type SuiTransactionBuilderResult<T = ()> = Result<T, SuiTransactionBuilderError>;

#[derive(Debug, Error)]
pub enum SuiTransactionBuilderError {
    #[error("Gas budget {0} is less than the reference gas price {1}. The gas budget must be at least the current reference gas price of {1}.")]
    InsufficientGasBudget(u64, u64),

    #[error("bcs field is unexpectedly empty")]
    BcsFieldEmpty,

    #[error("Cannot parse move object to gas object")]
    ParseMoveObjectError,

    #[error("Cannot find gas coin for signer address [{0}] with amount sufficient for the required gas amount [{1}].")]
    InsufficientGasCoin(SuiAddress, u64),

    #[error("Gas coin is in input coins of Pay transaction, use PaySui transaction instead!")]
    InvalidPayTransaction,

    #[error("Bcs field in object [{0}] is missing or not a package.")]
    MissingBcsField(ObjectID), // check

    #[error("Unable to determine ownership of upgrade capability")]
    UnknownUpgradeCapability,

    #[error("Invalid Batch Transaction: Batch Transaction cannot be empty")]
    InvalidBatchTransaction,

    #[error("Coins input should contain at least one coin object.")]
    EmptyInputCoins,

    #[error("Provided object [{0}] is not a move object.")]
    NotAMoveObject(ObjectID),

    #[error("Expecting either Coin<T> input coin objects. Received [{0}]")]
    InvalidCoinObjectType(String),

    #[error("All coins should be the same type, expecting {0}, got {1}.")]
    CoinTypeMismatch(ObjectType, ObjectType),

    #[error(transparent)]
    SuiObjectResponseError(#[from] SuiObjectResponseError),

    #[error(transparent)]
    ExecutionError(#[from] ExecutionError),

    #[error(transparent)]
    Bcs(#[from] bcs::Error),

    #[error(transparent)]
    UserInputError(#[from] UserInputError),

    #[error(transparent)]
    SuiError(#[from] SuiError),

    #[error(transparent)]
    DataReaderError(anyhow::Error),

    #[error(transparent)]
    ProgrammableTransactionBuilderError(anyhow::Error),

    #[error(transparent)]
    ObjectTypeError(anyhow::Error),

    #[error(transparent)]
    SuiObjectDataError(anyhow::Error),

    #[error(transparent)]
    TransactionDataError(anyhow::Error),

    #[error(transparent)]
    SuiJsonError(anyhow::Error),

    #[error(transparent)]
    IdentifierError(anyhow::Error),

    #[error(transparent)]
    TypeTagError(anyhow::Error),
}
