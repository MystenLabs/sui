//# init --edition 2024.beta

//# publish
module 0x42::m {

    public enum Action has drop {
        Stop,
        MoveTo { x: u64, y: u64 },
        ChangeSpeed(u64),
    }

    public fun test() {
        // Define a list of actions
        let actions: vector<Action> = vector[
            Action::MoveTo { x: 10, y: 20 },
            Action::ChangeSpeed(20),
            Action::MoveTo { x: 10, y: 20 },
            Action::Stop,
            Action::ChangeSpeed(40),
        ];

        let mut total_moves = 0;

        'loop_label: loop {
            let mut i = 0;
            while (i < actions.length()) {
                let action = &actions[i];

                match (action) {
                    Action::MoveTo { x: speed, y: _ } | Action::ChangeSpeed(speed) => {
                        'loop_label: loop {
                            total_moves = total_moves + *speed;
                            break 'loop_label
                        };
                    },
                    Action::Stop => {
                        break 'loop_label
                    },
                };
                i = i + 1;
            };
        };

        actions.destroy!(|_| {});

        assert!(total_moves == 40);
    }
}

//# run 0x42::m::test
