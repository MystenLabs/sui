address 0x2 {
module X {
    public entry fun foo() {}
}
}

script {
fun main() {
    0x2::X::foo()
}
}
