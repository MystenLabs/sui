module 0x42::TestLoopLabelShadowing;

public enum Action has drop {
    Stop,
    MoveTo { x: u64, y: u64 },
    ChangeSpeed(u64),
}

public fun test() {
    // Define a list of actions
    let actions: vector<Action> = vector[
        Action::MoveTo { x: 10, y: 20 },
        Action::ChangeSpeed(40),
        Action::MoveTo { x: 10, y: 20 },
        Action::Stop
    ];

    let mut total_moves = 0;

    'loop_label: loop {
        let mut i = 0;
        while (i < actions.length()) {
            let action = &actions[i];

            match (action) {
                Action::MoveTo { x, y } => {
                    'loop_label: loop {
                        total_moves = total_moves + *x + *y;
                        break 'loop_label
                    };
                },
                Action::ChangeSpeed(_) => {
                    'loop_label: loop {
                        break 'loop_label
                    };
                },
                Action::Stop => {
                    break 'loop_label
                },
                _ => {},
            };
            i = i + 1;
        };
    };

    actions.destroy_empty();

    assert!(total_moves == 60);
}
