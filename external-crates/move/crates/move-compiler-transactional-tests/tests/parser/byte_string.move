//# run
module 0x42::m {

fun main() {
    assert!(b"" == x"", 0);
    assert!(b"Diem" == x"4469656D", 1);
    assert!(b"\x4c\x69\x62\x72\x61" == x"4c69627261", 2);
    assert!(
        b"Γ ⊢ λ x. x : ∀α. α → α" ==
        x"CE9320E28AA220CEBB20782E2078203A20E28880CEB12E20CEB120E2869220CEB1",
        3
    );
}
}
