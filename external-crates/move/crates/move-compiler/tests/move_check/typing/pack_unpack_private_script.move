module 0x42::M {
    struct T {}

}


script {

    use 0x42::M::T;

    fun main() {
        let t = T {  };
        let T {} = t;
    }
}
