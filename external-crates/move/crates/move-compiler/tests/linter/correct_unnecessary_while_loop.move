module 0x42::M {

    public fun finite_loop() {
        let counter = 0;
        loop {
            if(counter == 10) {
                break
            };
            counter = counter + 1;
        }
    }
}