module dao::dao {
    use std::option;

    use std::string::String;
    use sui::coin::{Self, Coin};
    use sui::balance::{Self, Balance,Supply};
    use sui::object::{Self, ID, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    const ETaskDistributeEnded:u64 = 0;
    const ENotTaskCapOwner:u64 = 1;
    const EProposalClosed:u64 = 2;
    const EVoteSelf:u64 = 3;
    const EInvailVotes:u64 = 4;
    const EProposalCheck:u64 = 5;
    const EProposalNotClosed:u64 = 6;
    const ERoleCheck:u64 = 7;
    const EProposalNotPassed:u64 = 8;
    const EAlreadyClaimed:u64 = 9;
    const EInsufficientTreasurySupply:u64 = 10;

    const MAX_VOTES_ONE_TIME: u64 = 10;
    const TOTAL_SUPPLY:u64 = 100_000_000_000_000_000;
    const PROPOSAL_FEE:u64 = 5;
    const LEVEL1_REWARD:u64 = 10;
    const LEVEL2_REWARD:u64 = 15;
    const LEVEL3_REWARD:u64 = 30;




    struct DAO has drop{}
    struct Dao<phantom T> has key{
        id: UID,
        total_members: u64, //Total Number of DAO Members
        total_supply: Supply<T>, //Total Supply of DAO Tokens
    }

    //Treasury of the DAO
    struct Treasury<phantom T> has key,store{
        id:UID,
        supply: Balance<T>, //Balance Stored in the Treasury
    }

    struct Proposal has key,store{
        id: UID,
        title: String, //The title of the proposal
        describe: String, //Content of the Proposal
        level: u64,
        proposer: address, //Initiator of the Proposal
        lock_balance: u64, //DAO Tokens Locked by the Proposal
        support: u64, //Number of votes in favor of the proposal
        against: u64, //Number of votes against the proposal
        is_closed: bool, 
        is_passed: bool,
        is_claimed_reward: bool,
    }

    struct VoteCap has key{
        id: UID,
        proposal_id: ID,
        voter: address,
        is_support: bool,
        votes: u64,
    }

    struct CommunityTask has key{
        id:UID,
        describe: String,
        reward_amount: u64,
        distribute_ended: bool,
    }

    struct TaskRewardCap has key{
        id:UID,
        reward_amount: u64,
        owner: address,
    }

    struct InitCoreCap has key{
        id: UID,
        role_address:address,
    }

    struct CoreCap has key{
        id: UID,
        role_address:address,
    }

    struct MemberCap has key{
        id: UID,
        role_address:address,
    }

    fun init(witness: DAO, ctx: &mut TxContext) {
        //1. create dao token and mint supply
        let (treasury_cap,metadata) = coin::create_currency<DAO>(witness,18,b"DAO",b"dao",b"Dao token.",option::none(),ctx);
        transfer::public_freeze_object(metadata);
        let total_balance = coin::mint_balance<DAO>(&mut treasury_cap, TOTAL_SUPPLY);
        
        //2. move supply to treasury and share treasury
        let treasury = Treasury<DAO> {
            id: object::new(ctx),
            supply: total_balance,
        };
        transfer::share_object(treasury);

        //3.create dao metadata and share metadata
        let total_supply = coin::treasury_into_supply<DAO>(treasury_cap);

        let dao = Dao{
            id: object::new(ctx),
            total_members: 1,
            total_supply: total_supply,
        };
        transfer::share_object(dao);

        //4. mint Cap to msg.sender
        let msg_sender = tx_context::sender(ctx);
        
        let init_core_cap = InitCoreCap{
            id: object::new(ctx),
            role_address: msg_sender,
        };
        let core_cap = CoreCap{
            id: object::new(ctx),
            role_address: msg_sender,
        };
        let member_cap = MemberCap{
            id: object::new(ctx),
            role_address: msg_sender,
        };

        transfer::transfer(member_cap,msg_sender);
        transfer::transfer(core_cap, msg_sender);
        transfer::transfer(init_core_cap, msg_sender);
    }

    //======task========
    public fun set_community_task(core_cap:& CoreCap, describe:String, reward_amount:u64, ctx:&mut TxContext){
        check_corecap_role(core_cap,ctx);
        let new_task = CommunityTask{
            id: object::new(ctx),
            describe:describe,
            reward_amount:reward_amount,
            distribute_ended: false,
        };
        transfer::share_object(new_task);
    }


    public fun close_task(core_cap:&mut CoreCap, task:&mut CommunityTask,ctx:&mut TxContext){
        check_corecap_role(core_cap,ctx);
        task.distribute_ended = true;
    }


    public fun delete_task(core_cap:&mut CoreCap, task:CommunityTask,ctx:&mut TxContext){
        check_corecap_role(core_cap,ctx);
        let CommunityTask{ id,describe,reward_amount,distribute_ended} = task;
        object::delete(id);
    }


    public fun distribute_task_rewards(core_cap:& CoreCap, task:& CommunityTask, receiver:address, ctx:&mut TxContext,){
        check_corecap_role(core_cap,ctx);
        assert!(!task.distribute_ended, ETaskDistributeEnded);
        let reward_amount = task.reward_amount;
        let reward_cap = TaskRewardCap{
            id: object::new(ctx),
            reward_amount:reward_amount,
            owner:receiver,
        };
        transfer::transfer(reward_cap,receiver);
    }


    public fun claim_reward(reward_cap:TaskRewardCap, treasury:&mut Treasury<DAO>, ctx:&mut TxContext){
        let TaskRewardCap {id, reward_amount, owner} = reward_cap;
        assert!(owner == tx_context::sender(ctx), ENotTaskCapOwner);
        object::delete(id);
        let reward_coin = take_coin_from_treasury(treasury, reward_amount,ctx);
        transfer::public_transfer(reward_coin,owner);
    }

    //==========proposal==========
    public fun submit_proposal(member_cap:&MemberCap,title: String, describe:String, level:u64, coin:&mut Coin<DAO>,treasury:&mut Treasury<DAO>,ctx:&mut TxContext){
        //1.Verify Membership in the DAO
        check_membercap_role(member_cap, ctx);
        //2.Pay Proposal Fee
        transfer_coin_to_treasury(treasury,coin, PROPOSAL_FEE);
        //3.get duration time

        //4.Create Proposal
        let proposal = Proposal {
            id: object::new(ctx),
            title: title, //The title of the proposal
            describe: describe, //Content of the Proposal
            level: level,
            proposer: member_cap.role_address, //Initiator of the Proposal
            lock_balance: 0, //DAO Tokens Locked by the Proposal
            support: 0, //Number of votes in favor of the proposal
            against: 0, //Number of votes against the proposal
            is_closed: false,
            is_passed: false,
            is_claimed_reward: false,
        };

        transfer::share_object(proposal);
    }

    public fun change_proposal_level(core_cap:& CoreCap, proposal:&mut Proposal, new_level:u64 ,ctx:&mut TxContext){
        check_corecap_role(core_cap,ctx);
        assert!(!proposal.is_closed, EProposalClosed);
        proposal.level = new_level;
    }

    public fun close_proposal(core_cap:& CoreCap,proposal:&mut Proposal,ctx:&mut TxContext){
        check_corecap_role(core_cap,ctx);
        assert!(!proposal.is_closed, EProposalClosed);
        if(proposal.against == 0 || (proposal.support * 2) / 3 >= proposal.against){
            proposal.is_passed = true;
        };
        proposal.is_closed = true;
    }


    public fun vote(member_cap:&MemberCap, proposal:&mut Proposal, votes:u64, is_support:bool, coin:&mut Coin<DAO>, treasury:&mut Treasury<DAO>,ctx:&mut TxContext){
        //1. verify
        check_membercap_role(member_cap,ctx);
        assert!(!proposal.is_closed,EProposalClosed);
        assert!(member_cap.role_address != proposal.proposer, EVoteSelf);
        assert!(votes >= 1  && votes <= MAX_VOTES_ONE_TIME, EInvailVotes);
        check_membercap_role(member_cap, ctx);

        //2. transfer dao
        transfer_coin_to_treasury(treasury,coin,votes);

        //3. change proposal
        update_proposal_vote(proposal,votes,is_support);

        //4. distrubte voteCap
        let vote_cap = VoteCap {
            id: object::new(ctx),
            proposal_id:object::uid_to_inner(&proposal.id),
            voter: member_cap.role_address,
            is_support: is_support,
            votes: votes,
        };

        transfer::transfer(vote_cap, member_cap.role_address);
    }


    public fun claim_proposal_vote(member_cap:&MemberCap, proposal:&mut Proposal, vote_cap: VoteCap,treasury:&mut Treasury<DAO>, ctx:&mut TxContext){
        // 1.veriry the parameters
        check_membercap_role(member_cap,ctx);
        assert!(&vote_cap.proposal_id == object::borrow_id(proposal), EProposalCheck);
        assert!(proposal.is_closed, EProposalNotClosed);
        assert!(member_cap.role_address == vote_cap.voter, ERoleCheck);

        // 2.delete voteCap
        let VoteCap {id, proposal_id, voter, is_support, votes} = vote_cap;
        object::delete(id);

        // 3.take coin from treasury
        let reward_coin = take_coin_from_treasury(treasury,votes,ctx);

        // 4.transfer coin to voter
         transfer::public_transfer(reward_coin,voter);
    }


    public fun claim_proposal_reward(member_cap:&MemberCap, proposal:&mut Proposal, treasury:&mut Treasury<DAO> ,ctx:&mut TxContext){
        // 1.veriry the parameters
        check_membercap_role(member_cap,ctx);
        assert!(proposal.is_passed, EProposalNotPassed);
        assert!(!proposal.is_claimed_reward, EAlreadyClaimed);
        assert!(proposal.proposer == tx_context::sender(ctx), ERoleCheck);
        proposal.is_claimed_reward = true;
        
        //2.reward calculation
        let level = proposal.level;
        let reward_amount:u64;
        if(level == 1){
            reward_amount = LEVEL1_REWARD;
        }
        else if(level == 2){
            reward_amount = LEVEL2_REWARD;
        }
        else{
            reward_amount = LEVEL3_REWARD;
        };

        //3.send reward token;
        let reward_coin = take_coin_from_treasury(treasury,reward_amount,ctx);
        transfer::public_transfer(reward_coin,proposal.proposer);
    }

    //===========Access Contral==========
    public fun distribute_membercap(core_cap:& CoreCap, receiver:address, dao:&mut Dao<DAO>, ctx:&mut TxContext){
        check_corecap_role(core_cap,ctx);
        let total_members =& dao.total_members;
        dao.total_members = *total_members + 1;

        let member_cap = MemberCap{
            id: object::new(ctx),
            role_address: receiver,
        };
        transfer::transfer(member_cap,receiver);

    }
    

    public fun distribute_corecap(init_core_cap:& InitCoreCap, receiver:address, ctx:&mut TxContext){
        check_init_corecap_role(init_core_cap,ctx);
        let core_cap = CoreCap{
            id: object::new(ctx),
            role_address: receiver,
        };
        transfer::transfer(core_cap,receiver);
    }


    public fun distribute_init_corecap(init_core_cap:& InitCoreCap, receiver:address, ctx:&mut TxContext){
        check_init_corecap_role(init_core_cap,ctx);
        let init_core_cap = InitCoreCap{
            id: object::new(ctx),
            role_address: receiver,
        };
        transfer::transfer(init_core_cap,receiver);
    }

    //==========internal==========
    fun check_init_corecap_role(init_core_cap:& InitCoreCap,ctx: &mut TxContext){
        assert!(init_core_cap.role_address== tx_context::sender(ctx), ERoleCheck);
    }

    fun check_corecap_role(core_cap:& CoreCap,ctx: &mut TxContext){
        assert!(core_cap.role_address== tx_context::sender(ctx), ERoleCheck);
    }


    fun check_membercap_role(member_cap:& MemberCap,ctx: &mut TxContext){
        assert!(member_cap.role_address == tx_context::sender(ctx), ERoleCheck);
    }


    fun transfer_coin_to_treasury(treasury:&mut Treasury<DAO>,coin:&mut Coin<DAO>,amount: u64){
        assert!(balance::value<DAO>(&treasury.supply) >= amount, EInsufficientTreasurySupply);
        balance::join<DAO>(&mut treasury.supply, balance::split<DAO>(coin::balance_mut(coin), amount));
    }


    fun take_coin_from_treasury(treasury:&mut Treasury<DAO>,amount: u64,ctx:&mut TxContext): Coin<DAO>{
        let supply = &mut treasury.supply;
        let reward_coin = coin::take<DAO>(supply, amount, ctx);
        reward_coin
    }


    fun update_proposal_vote(proposal:&mut Proposal,votes:u64,is_support:bool){
        let lock_balance = & proposal.lock_balance;
        proposal.lock_balance =*lock_balance + votes;

        if(is_support){
            let support = & proposal.support;
            proposal.support =*support + votes;
        }
        else{
            let against = & proposal.against;
            proposal.against =*against + votes;
        }

        
    }

    //===========View============
    public fun is_closed(proposal: &Proposal):bool{
        proposal.is_closed
    }

    public fun is_passed(proposal: &Proposal):bool{
        proposal.is_passed
    }

    public fun support(proposal: &Proposal):u64{
        proposal.support
    }

    public fun against(proposal: &Proposal):u64{
        proposal.against
    }

    public fun proposer(proposal: &Proposal):address{
        proposal.proposer
    }

    public fun total_members(dao: &Dao<DAO>):u64{
        dao.total_members
    }

    public fun treasury_supply(treasury: &Treasury<DAO>):u64{
        balance::value<DAO>(&treasury.supply)
    }
    //test
    #[test_only]
    /// Wrapper of module initializer for testing
    public fun test_init(ctx: &mut TxContext) {
        init(DAO {}, ctx)
    }

    #[test]
    public fun test() {
        
        use sui::test_scenario;
        use std::string::{Self};

        // Initialize a mock sender address
        let addr1 = @0xA;
        let addr2 = @0xB;
        let addr3 = @0xC;
        // Begins a multi-transaction scenario with addr1 as the sender
        let scenario = test_scenario::begin(addr1);

        //1. dao deploy
        test_init(test_scenario::ctx(&mut scenario));

        //2. set community task
        {
            test_scenario::next_tx(&mut scenario, addr1);
        
            let core_cap = test_scenario::take_from_sender<CoreCap>(& scenario);

            set_community_task(&core_cap, string::utf8(b"The first task"), 5, test_scenario::ctx(&mut scenario));

            test_scenario::return_to_sender(& scenario, core_cap);
        };
        //3. distribute_task_rewards
        {
            test_scenario::next_tx(&mut scenario, addr1);

            let core_cap = test_scenario::take_from_sender<CoreCap>(& scenario);
            let community_task: CommunityTask = test_scenario::take_shared<CommunityTask>(& scenario);

            distribute_task_rewards(&core_cap, &community_task, addr2, test_scenario::ctx(&mut scenario));
            distribute_task_rewards(&core_cap, &community_task, addr3, test_scenario::ctx(&mut scenario));


            test_scenario::return_to_sender(& scenario, core_cap);
            test_scenario::return_shared(community_task);
        };

        //4. claim reward
        {
            test_scenario::next_tx(&mut scenario, addr2);

            let reward_cap = test_scenario::take_from_sender<TaskRewardCap>(& scenario);
            let treasury = test_scenario::take_shared<Treasury<DAO>>(& scenario);

            claim_reward(reward_cap, &mut treasury, test_scenario::ctx(&mut scenario));

            test_scenario::return_shared(treasury);
        };

        {
            test_scenario::next_tx(&mut scenario, addr3);

            let reward_cap = test_scenario::take_from_sender<TaskRewardCap>(& scenario);
            let treasury = test_scenario::take_shared<Treasury<DAO>>(& scenario);

            claim_reward(reward_cap, &mut treasury, test_scenario::ctx(&mut scenario));

            test_scenario::return_shared(treasury);
        };

        //5. distribute membercap
        {
            test_scenario::next_tx(&mut scenario, addr1);
            let core_cap = test_scenario::take_from_sender<CoreCap>(& scenario);
            let dao = test_scenario::take_shared<Dao<DAO>>(& scenario);

            assert!(dao.total_members == 1, 0);
            distribute_membercap(&core_cap, addr2 ,&mut dao, test_scenario::ctx(&mut scenario));
            distribute_membercap(&core_cap, addr3 ,&mut dao, test_scenario::ctx(&mut scenario));
            assert!(dao.total_members == 3, 0);

            test_scenario::return_to_sender(& scenario, core_cap);
            test_scenario::return_shared(dao);
        };


        //6. create proposal
        {

            test_scenario::next_tx(&mut scenario, addr2);

            let member_cap = test_scenario::take_from_sender(& scenario);
            let coin = test_scenario::take_from_sender(& scenario);
            let treasury = test_scenario::take_shared<Treasury<DAO>>(& scenario);

            submit_proposal(&member_cap, string::utf8(b"The first community proposal"), string::utf8(b"....."), 1, &mut coin,&mut treasury, test_scenario::ctx(&mut scenario));

            coin::destroy_zero<DAO>(coin);
            test_scenario::return_to_sender(& scenario, member_cap);
            test_scenario::return_shared(treasury);

        };

        //7. vote
        {
            test_scenario::next_tx(&mut scenario, addr3);

            let member_cap = test_scenario::take_from_sender(& scenario);
            let coin = test_scenario::take_from_sender<Coin<DAO>>(& scenario);
            let treasury = test_scenario::take_shared<Treasury<DAO>>(& scenario);
            let proposal = test_scenario::take_shared<Proposal>(& scenario);

            assert!(balance::value(coin::balance<DAO>(&coin)) == 5, 0);
            assert!(proposal.support == 0, 0);
            assert!(proposal.proposer == addr2, 0);
            assert!(proposal.lock_balance == 0, 0);
            vote(&member_cap,&mut  proposal,5, true,&mut  coin,&mut treasury, test_scenario::ctx(&mut scenario));
            assert!(proposal.support == 5, 0);
            assert!(proposal.lock_balance == 5, 0);

            coin::destroy_zero<DAO>(coin);
            test_scenario::return_to_sender(& scenario, member_cap);
            test_scenario::return_shared(treasury);
            test_scenario::return_shared(proposal);
        };

        ///8. Close proposal
        {
            test_scenario::next_tx(&mut scenario, addr1);

            let proposal = test_scenario::take_shared<Proposal>(& scenario);
            let core_cap = test_scenario::take_from_sender<CoreCap>(& scenario);

            assert!(proposal.is_closed == false, 0);
            assert!(proposal.is_passed == false, 0);
            close_proposal(&core_cap,&mut proposal, test_scenario::ctx(&mut scenario));
            assert!(proposal.is_closed == true, 0);
            assert!(proposal.is_passed == true, 0);

            test_scenario::return_to_sender(& scenario, core_cap);
            test_scenario::return_shared(proposal);
        };

        //9. claim votes
        {
            test_scenario::next_tx(&mut scenario, addr3);
            let proposal = test_scenario::take_shared<Proposal>(& scenario);
            let member_cap = test_scenario::take_from_sender<MemberCap>(& scenario);
            let vote_cap = test_scenario::take_from_sender<VoteCap>(& scenario);
            let treasury = test_scenario::take_shared<Treasury<DAO>>(& scenario);

            claim_proposal_vote(&member_cap,&mut proposal, vote_cap,&mut treasury, test_scenario::ctx(&mut scenario));

            test_scenario::return_to_sender(& scenario, member_cap);
            test_scenario::return_shared(proposal);
            test_scenario::return_shared(treasury);

        };
        // check claim
        {
            test_scenario::next_tx(&mut scenario, addr3);
            let coin = test_scenario::take_from_sender<Coin<DAO>>(& scenario);
            assert!(balance::value(coin::balance<DAO>(&coin)) == 5, 0);
            test_scenario::return_to_sender(& scenario, coin);
        };

        //10. claim proposal rewards
        {
            test_scenario::next_tx(&mut scenario, addr2);
            let proposal = test_scenario::take_shared<Proposal>(& scenario);
            let member_cap = test_scenario::take_from_sender<MemberCap>(& scenario);
            let treasury = test_scenario::take_shared<Treasury<DAO>>(& scenario);

            claim_proposal_reward(&member_cap, &mut proposal, &mut treasury ,test_scenario::ctx(&mut scenario));

            test_scenario::return_to_sender(& scenario, member_cap);
            test_scenario::return_shared(proposal);
            test_scenario::return_shared(treasury);

        };
        // check claim
        {
            test_scenario::next_tx(&mut scenario, addr2);
            let coin = test_scenario::take_from_sender<Coin<DAO>>(&scenario);
            assert!(balance::value(coin::balance<DAO>(&coin)) == 10, 0);
            test_scenario::return_to_sender(& scenario, coin);
        };

        test_scenario::end(scenario);
    }

}