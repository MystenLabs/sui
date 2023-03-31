// dep: bcs
// dep: Runtime
// actor: 0x3 Basic State init start count_down
// instance: 0x3 Basic 0x4
#[actor]
module Test::Basic {

    #[state]
    struct State {
        value: u64,
    }

    #[init]
    fun init(): State {
        State{value: 0}
    }

    #[message]
    fun start(s: &State) {
        send_count_down(@4, 5);
    }

    #[message]
    fun count_down(s: &mut State, counter: u64) {
        if (counter == 0) {
            assert!(s.value == 5, 1)
        } else {
            s.value = s.value + 1;
            send_count_down(@4, counter - 1);
        }
    }
}
