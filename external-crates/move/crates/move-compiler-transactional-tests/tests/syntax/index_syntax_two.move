//# init --edition 2024.alpha

//# publish
module 0x42::matrix {

    public struct Matrix has drop { v: vector<vector<u64>> }

    #[syntax(index)]
    public fun borrow_matrix(s: &Matrix, i: u64, j: u64):  &u64 {
        s.v.borrow(i).borrow(j)
    }

    #[syntax(index)]
    public fun borrow_s_mut(s: &mut Matrix, i: u64, j: u64):  &mut u64 {
        s.v.borrow_mut(i).borrow_mut(j)
    }

    public fun make_matrix(v: vector<vector<u64>>):  Matrix {
        Matrix { v }
    }

}

//# run
module 0x42::main {
    use 0x42::matrix;

    fun main() {
        let v0 = vector<u64>[1, 0, 0];
        let v1 = vector<u64>[0, 1, 0];
        let v2 = vector<u64>[0, 0, 1];
        let v = vector<vector<u64>>[v0, v1, v2];
        let mut m = matrix::make_matrix(v);

        let mut i = 0;
        while (i < 3) {
            let mut j = 0;
            while (j < 3) {
                if (i == j) {
                    assert!(m[i, j] == 1, i);
                } else {
                    assert!(m[i, j] == 0, i + 10);
                };
                *(&mut m[i,j]) = 2;
                j = j + 1;
            };
            i = i + 1;
        };

        let mut i = 0;
        while (i < 3) {
            let mut j = 0;
            while (j < 3) {
                assert!(m[i, j] == 2, i * j + 20);
                j = j + 1;
            };
            i = i + 1;
        };
    }
}

