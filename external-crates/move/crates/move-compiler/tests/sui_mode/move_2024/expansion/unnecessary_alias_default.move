module sui::object {
    public struct ID()
    public struct UID()
}
module sui::transfer {

}
module sui::tx_context {
    public struct TxContext()
}

module a::m {
    use sui::{object::{Self, ID, UID}, transfer, tx_context::{Self, TxContext}};
}
