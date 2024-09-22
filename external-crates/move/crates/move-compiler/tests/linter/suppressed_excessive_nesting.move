module 0x42::excessive_nesting {

    #[allow(lint(excessive_nesting))]
    fun func1() {
        let x = 10;
        let y = 20;

        if (x > 5) {
            if (y < 30) {
                if (x + y > 30) {
                    if (x - y < 0) {
                        // Further nesting beyond recommended levels
                        if (y % 2 == 0) {
                            abort 1
                        } else {
                            abort 2
                        }
                    };
                };
            };
        }
    }
}
