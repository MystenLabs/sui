module a::m;

public enum Proposal has store, copy, drop {
        ConfigProposalQuorum {approved: vector<u16>, rejected: vector<u16>},
        MpcTxProposalQuorum {approved: vector<u16>, rejected: vector<u16>},
        MpcTxProposalSpecific { require_approval_users: vector<u16>, threshold: u16, approved: vector<u16>, rejected: vector<u16> },
}

public struct Users {}

public fun quorum_approves(_users: &Users, _approved: &vector<u16>): bool { false }

public(package) fun is_proposal_approved(proposal: &Proposal, users: &Users): bool {
    match (proposal) {
        Proposal::ConfigProposalQuorum { approved: approved, rejected: _ } => {
            users.quorum_approves(approved)
        },
        Proposal::MpcTxProposalQuorum { approved:approved, rejected: _ } => {
            users.quorum_approves(approved)
        },
        Proposal::MpcTxProposalSpecific {
            require_approval_users : _,
            threshold: threshold,
            approved: approved,
            rejected: _
        } => {
            ((approved.length() as u16) >= *threshold)
        }
    }
}
