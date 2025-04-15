// options:
// printWidth: 100

module prettier::other {
    fun other() {
        let mut fungible_staked_sui_1 =
            staking_pool.create_fungible_staked_sui_for_testing(
                100_000_000_000,
                scenario.ctx(),
            );
        let fungible_staked_sui_2 =
            staking_pool.create_fungible_staked_sui_for_testing(
                200_000_000_000,
                scenario.ctx(),
            );

        fungible_staked_sui_1.join(fungible_staked_sui_2);

        // expected to break on the argument list
        let new_id =
            validator_cap::new_unverified_validator_operation_cap_and_transfer(address, ctx);

        // expected not to break on the first line
        let mut validator = new_from_metadata(
            new_metadata(
                sui_address,
                protocol_pubkey_bytes,
                network_pubkey_bytes,
                worker_pubkey_bytes,
                proof_of_possession,
                name.to_ascii_string().to_string(),
                description.to_ascii_string().to_string(),
                url::new_unsafe_from_bytes(image_url),
                url::new_unsafe_from_bytes(project_url),
                net_address.to_ascii_string().to_string(),
                p2p_address.to_ascii_string().to_string(),
                primary_address.to_ascii_string().to_string(),
                worker_address.to_ascii_string().to_string(),
                bag::new(ctx),
            ),
            gas_price,
            commission_rate,
            ctx,
        );

        character.add(
            AppKey {},
            BattleApp {
                stats, // this comment is wiped
                wins: 0, losses: 0, current_battle: option::none() },
        );

        // allow lambda to be on the same line
        s.do!(|i| {
            let c = s.as_bytes()[i];
            if (c == 37) {
                // percent "%"
                let a = s.as_bytes()[i + 1];
                let b = s.as_bytes()[i + 2];
                let a = if (a >= 65) a - 55 else a - 48;
                let b = if (b >= 65) b - 55 else b - 48;
                res.push_back(a * 16 + b);
                i = i + 2;
            } else {
                res.push_back(c);
            };
            i = i + 1;
        });

        // some weird stuff is happening with the comment
        expr
            .div(50)
            .add(2)
            .mul(random) // ???
            .div(255)
            // ???
            .calc_u64(0);
    }
}
