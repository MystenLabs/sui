script {
#[allow(unused_type_parameter)]
/// This script does really nothing but just aborts.
fun some<T>(_account: signer) {
    abort 1
}
}
