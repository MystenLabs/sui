
module 0x42::m {
    fun t0() {
        let mut count = 0;
        let (start, stop) = (0, 10);
        let mut i = start;
        while (i < stop) {
            let x = i;
            count = count + x * x;
            i = i + 1;
        };
        assert!(count == 285, 0);
    }

    fun t1() {
        let mut count = 0u64;
            {
            let (start, stop) = (0, 10);
            'macro:  {
                'lambdabreak:  {
                        {
                        let mut i = start;
                        while (i < stop) 'loop: {
                                {
                                let x = i;
                                'lambdareturn:  {
                                    count = count + x * x;
                                }
                            };
                            i = i + 1;
                        }
                    }
                }
            }
        };
        assert!(count == 285, 0);
    }
}
