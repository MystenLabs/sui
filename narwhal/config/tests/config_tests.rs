use std::collections::BTreeMap;

use config::{PrimaryAddresses, Stake};
use crypto::ed25519::Ed25519PublicKey;
use rand::seq::SliceRandom;

#[test]
fn update_primary_network_info_test() {
    let committee = test_utils::committee(None);
    let res = committee
        .update_primary_network_info(BTreeMap::new())
        .unwrap_err();
    for err in res {
        assert!(matches!(
            err,
            config::ComitteeUpdateError::MissingFromUpdate(_)
        ))
    }

    let committee2 = test_utils::committee(42);
    let invalid_new_info = committee2
        .authorities
        .load()
        .iter()
        .map(|(pk, a)| (pk.clone(), (a.stake, a.primary.clone())))
        .collect::<BTreeMap<_, (Stake, PrimaryAddresses)>>();
    let res2 = committee
        .update_primary_network_info(invalid_new_info)
        .unwrap_err();
    for err in res2 {
        // we'll get the two collections reporting missing from each other
        assert!(matches!(
            err,
            config::ComitteeUpdateError::NotInCommittee(_)
                | config::ComitteeUpdateError::MissingFromUpdate(_)
        ))
    }

    let committee3 = test_utils::committee(None);
    let invalid_new_info = committee3
        .authorities
        .load()
        .iter()
        // change the stake
        .map(|(pk, a)| (pk.clone(), (a.stake + 1, a.primary.clone())))
        .collect::<BTreeMap<_, (Stake, PrimaryAddresses)>>();
    let res2 = committee
        .update_primary_network_info(invalid_new_info)
        .unwrap_err();
    for err in res2 {
        assert!(matches!(
            err,
            config::ComitteeUpdateError::DifferentStake(_)
        ))
    }

    let committee4 = test_utils::committee(None);
    let mut pk_n_stake = Vec::new();
    let mut addresses = Vec::new();

    committee4.authorities.load().iter().for_each(|(pk, a)| {
        pk_n_stake.push((pk.clone(), a.stake));
        addresses.push(a.primary.clone())
    });

    let mut rng = rand::thread_rng();
    addresses.shuffle(&mut rng);

    let new_info = pk_n_stake
        .into_iter()
        .zip(addresses)
        .map(|((pk, stk), addr)| (pk, (stk, addr)))
        .collect::<BTreeMap<Ed25519PublicKey, (Stake, PrimaryAddresses)>>();
    let res = committee.update_primary_network_info(new_info.clone());
    assert!(res.is_ok());
    for (pk, a) in committee.authorities.load().iter() {
        assert_eq!(a.primary, new_info.get(pk).unwrap().1);
    }
}
