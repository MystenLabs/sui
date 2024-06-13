//# init --edition 2024.beta

//# run
module 0x42::m {

fun main() {
    assert!(b"" == x"");
    assert!(b"Diem" == x"4469656D");
    assert!(b"\x4c\x69\x62\x72\x61" == x"4c69627261");
    assert!(
        b"Γ ⊢ λ x. x : ∀α. α → α" ==
        x"CE9320E28AA220CEBB20782E2078203A20E28880CEB12E20CEB120E2869220CEB1",
    );
    assert!(
        b"😏\n👉🕶️\n😎" ==
        vector[
            240,
            159,
            152,
            143,
            10,
            240,
            159,
            145,
            137,
            240,
            159,
            149,
            182,
            239,
            184,
            143,
            10,
            240,
            159,
            152,
            142,
        ],
    );
}
}
