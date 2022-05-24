use std::cmp::Ordering;

use cosmwasm_std::{
    log, to_binary, Api, Binary, Env, Extern, HandleResponse, InitResponse, Querier,
    StdError, StdResult, Storage, CosmosMsg, Empty };

use crate::error::ContractError;
use crate::expiration::Expiration;
use crate::msg::{ HandleMsg, InitMsg, QueryMsg, Vote };
use crate::query::{ ProposalListResponse, ProposalResponse, VoteInfo, VoteListResponse,
                    VoteResponse, VoterListResponse, VoterResponse, Status };
use crate::state::{ config, config_read, voters, voters_read, proposal_count, proposal_count_read,
                    ballots, ballots_read, proposals, proposals_read, voters_list, voters_list_read };
use crate::state::{ Ballot, Config, Proposal, Votes };
use crate::threshold::ThresholdResponse;

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    _env: Env,
    msg: InitMsg,
) -> Result<InitResponse, ContractError> {
    if msg.voters.is_empty() {
        return Err(ContractError::NoVoters {});
    }

    let total_weight = msg.voters.iter().map(|v| v.weight).sum();

    msg.threshold.validate(total_weight)?;
    // TODO Implement address validation

    let cfg = Config {
        threshold: msg.threshold,
        total_weight,
        max_voting_period: msg.max_voting_period,
    };

    // save the configuration settings
    config(&mut deps.storage).save(&cfg)?;

    // save the list of Voters
    voters_list(&mut deps.storage).save(&msg.voters)?;
    
    // save each voter's address and weight in a key-value pair
    for voter in msg.voters.iter() {
        voters(&mut deps.storage).save(voter.addr.as_bytes(), &voter.weight)?;
    }

    // set initial value for proposal count
    proposal_count(&mut deps.storage).save(&0)?;

    Ok(InitResponse::default())
}

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: HandleMsg,
) -> Result<HandleResponse<Empty>, ContractError> {
    match msg {
        HandleMsg::Propose {
            title,
            description,
            msgs,
            latest,
        } => execute_propose(deps, env, title, description, msgs, latest),
        HandleMsg::Vote { proposal_id, vote } => execute_vote(deps, env, proposal_id, vote),
        HandleMsg::Execute { proposal_id } => execute_execute(deps, env, proposal_id),
        HandleMsg::Close { proposal_id } => execute_close(deps, env, proposal_id),
    }
}

pub fn execute_propose<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    title: String,
    description: String,
    msgs: Vec<CosmosMsg>,
    // we ignore earliest
    latest: Option<Expiration>,
) -> Result<HandleResponse<Empty>, ContractError> {
    // only members of the multisig can create a proposal
    let vote_power: u64 = voters_read(&deps.storage)
        .may_load(&env.message.sender.to_string().as_bytes())?
        .ok_or(ContractError::Unauthorized {})?;

    let cfg = config_read(&deps.storage).load()?;

    // max expires also used as default
    let max_expires = cfg.max_voting_period.after(&env.block);
    let mut expires = latest.unwrap_or(max_expires);
    let comp = expires.partial_cmp(&max_expires);
    if let Some(Ordering::Greater) = comp {
        expires = max_expires;
    } else if comp.is_none() {
        return Err(ContractError::WrongExpiration {});
    }

    // create a proposal
    let mut prop = Proposal {
        title,
        description,
        start_height: env.block.height,
        expires,
        msgs,
        status: Status::Open,
        votes: Votes::yes(vote_power),
        threshold: cfg.threshold,
        total_weight: cfg.total_weight,
    };
    prop.update_status(&env.block);
    let proposal_id = proposal_count(&mut deps.storage).update(|mut id| {
        id += 1;
        Ok(id)
    })?;
    proposals(&mut deps.storage).save(&proposal_id.to_le_bytes(), &prop)?;

    // add the first yes vote from voter
    let ballot = Ballot {
        weight: vote_power,
        vote: Vote::Yes,
    };
    ballots(&mut deps.storage, proposal_id).save(&env.message.sender.to_string().as_bytes(),&ballot)?;

    Ok(HandleResponse {
        messages: vec![],
        log: vec![
            log("action","propose"),
            log("sender", env.message.sender),
            log("proposal_id",&proposal_id),
            log("status", format!("{:?}", prop.status))],
        data: None
    })
}

pub fn execute_vote<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    proposal_id: u64,
    vote: Vote,
) -> Result<HandleResponse<Empty>, ContractError> {
    // only members of the multisig with weight >= 1 can vote
    let voter_power = voters_read(&deps.storage).may_load(&env.message.sender.to_string().as_bytes())?;
    let vote_power = match voter_power {
        Some(power) if power >= 1 => power,
        _ => return Err(ContractError::Unauthorized {}),
    };

    // ensure proposal exists and can be voted on
    let mut prop = proposals_read(&deps.storage).load(&proposal_id.to_le_bytes())?;
    if prop.status != Status::Open {
        return Err(ContractError::NotOpen {});
    }
    if prop.expires.is_expired(&env.block) {
        return Err(ContractError::Expired {});
    }

    // a voter can only vote once
    if let Some(_ballot) = ballots_read(&deps.storage, proposal_id).may_load(&env.message.sender.to_string().as_bytes())? {
        return Err(ContractError::AlreadyVoted {})
    }

    let ballot = Ballot {
        weight: vote_power,
        vote,
    };

    ballots(&mut deps.storage, proposal_id).save(&env.message.sender.to_string().as_bytes(),&ballot)?;

    // update vote tally
    prop.votes.add_vote(vote, vote_power);
    prop.update_status(&env.block);
    proposals(&mut deps.storage).save(&proposal_id.to_le_bytes(), &prop)?;

    Ok(HandleResponse {
        messages: vec![],
        log: vec![
            log("action","vote"),
            log("sender", env.message.sender),
            log("proposal_id", proposal_id.to_string()),
            log("status", format!("{:?}", prop.status))],
        data: None
    })
}

pub fn execute_execute<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    proposal_id: u64,
) -> Result<HandleResponse, ContractError> {
    // anyone can trigger this if the vote passed

    let mut prop = proposals_read(&deps.storage).load(&proposal_id.to_le_bytes())?;
    // we allow execution even after the proposal "expiration" as long as all vote come in before
    // that point. If it was approved on time, it can be executed any time.
    if prop.status != Status::Passed {
        return Err(ContractError::WrongExecuteStatus {});
    }

    // set it to executed
    prop.status = Status::Executed;
    proposals(&mut deps.storage).save(&proposal_id.to_le_bytes(), &prop)?;

    // dispatch all proposed messages
    Ok(HandleResponse {
        messages: prop.msgs,
        log: vec![
            log("action","execute"),
            log("sender", env.message.sender),
            log("proposal_id", proposal_id.to_string())],
        data: None
    })
}

pub fn execute_close<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    proposal_id: u64,
) -> Result<HandleResponse<Empty>, ContractError> {
    // anyone can trigger this if the vote passed

    let mut prop = proposals_read(&deps.storage).load(&proposal_id.to_le_bytes())?;
    if [Status::Executed, Status::Rejected, Status::Passed]
        .iter()
        .any(|x| *x == prop.status)
    {
        return Err(ContractError::WrongCloseStatus {});
    }
    if !prop.expires.is_expired(&env.block) {
        return Err(ContractError::NotExpired {});
    }

    // set it to failed
    prop.status = Status::Rejected;

    proposals(&mut deps.storage).save(&proposal_id.to_le_bytes(), &prop)?;

    Ok(HandleResponse {
        messages: vec![],
        log: vec![
            log("action","close"),
            log("sender", env.message.sender),
            log("proposal_id", proposal_id.to_string())],
        data: None
    })
}

// Queries and query functions

pub fn query<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    msg: QueryMsg
) -> StdResult<Binary> {
    match msg {
        QueryMsg::Threshold {} => to_binary(&query_threshold(deps)?),
        QueryMsg::Proposal { proposal_id } => to_binary(&query_proposal(deps, proposal_id)?),
        QueryMsg::Vote { proposal_id, voter } => to_binary(&query_vote(deps, proposal_id, voter)?),
        QueryMsg::ListProposals { start_after, limit } => {
            to_binary(&list_proposals(deps, start_after, limit)?)
        },
        QueryMsg::ReverseProposals {
            start_before,
            limit,
        } => to_binary(&reverse_proposals(deps, start_before, limit)?),
        QueryMsg::ListVotes {
            proposal_id,
            start_after,
            limit,
        } => to_binary(&list_votes(deps, proposal_id, start_after, limit)?),
        QueryMsg::Voter { address } => to_binary(&query_voter(deps, address)?),
        QueryMsg::ListVoters { start_after, limit } => {
            to_binary(&list_voters(deps, start_after, limit)?)
        }
    }
}

fn query_threshold<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>) -> StdResult<ThresholdResponse> {
    let cfg = config_read(&deps.storage).load()?;
    Ok(cfg.threshold.to_response(cfg.total_weight))
}

fn query_proposal<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>, id: u64) -> StdResult<ProposalResponse> {
    let prop = proposals_read(&deps.storage).load(&id.to_le_bytes())?;

    // TODO Uncomment this line once block info is available to queries
    // let status = prop.current_status(&env.block);

    let threshold = prop.threshold.to_response(prop.total_weight);
    Ok(ProposalResponse {
        id,
        title: prop.title,
        description: prop.description,
        msgs: prop.msgs,
        status: prop.status, //using status from last save (it may have expired since then)
        expires: prop.expires,
        threshold,
    })
}

// settings for pagination
const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;

fn list_proposals<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    start_after: Option<u64>,
    limit: Option<u32>,
) -> StdResult<ProposalListResponse> {
    let latest_prop = proposal_count_read(&deps.storage).load()?;

    let limit: u64 = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT).into();
    let limit = limit.min(latest_prop);
    let start = start_after.unwrap_or(1);
    
    let mut proposals: Vec<ProposalResponse> = vec![];
    let mut i = start;
    while i <= limit {
        let prop = proposals_read(&deps.storage).load(&i.to_le_bytes())?;
        let threshold = prop.threshold.to_response(prop.total_weight);
        let prop_response = ProposalResponse {
            id: i,
            title: prop.title,
            description: prop.description,
            msgs: prop.msgs,
            status: prop.status, //using status from last save (it may have expired since then)
            expires: prop.expires,
            threshold,
        };
        proposals.push(prop_response);
        i = i+1;
    }

    Ok(ProposalListResponse { proposals })
}

fn reverse_proposals<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    start_before: Option<u64>,
    limit: Option<u32>,
) -> StdResult<ProposalListResponse> {
    let latest_prop = proposal_count_read(&deps.storage).load()?;

    let limit: u64 = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT).into();
    let limit = limit.min(latest_prop) + 1;
    let start = start_before.unwrap_or(latest_prop).min(latest_prop);

    let mut proposals: Vec<ProposalResponse> = vec![];
    let mut i = start;
    for _n in 1..limit {
        let prop = proposals_read(&deps.storage).load(&i.to_le_bytes())?;
        let threshold = prop.threshold.to_response(prop.total_weight);
        let prop_response = ProposalResponse {
            id: i,
            title: prop.title,
            description: prop.description,
            msgs: prop.msgs,
            status: prop.status, //using status from last save (it may have expired since then)
            expires: prop.expires,
            threshold,
        };
        proposals.push(prop_response);
        i = i-1;
    }

    Ok(ProposalListResponse { proposals })
}

fn query_vote<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    proposal_id: u64,
    voter: String
) -> StdResult<VoteResponse> {
    // TODO: Implement address validation

    let ballot = ballots_read(&deps.storage, proposal_id).may_load(voter.as_bytes())?;
    let vote = ballot.map(|b| VoteInfo {
        proposal_id,
        voter: voter.into(),
        vote: b.vote,
        weight: b.weight,
    });
    Ok(VoteResponse { vote })
}

fn list_votes<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    proposal_id: u64,
    _start_after: Option<String>,
    _limit: Option<u32>,
) -> StdResult<VoteListResponse> {
    // Currently no use for start_after or limit 
    // Returns every vote 
    let voters = voters_list_read(&deps.storage).load()?;
    let mut votes: Vec<VoteInfo> = Vec::new();
    for voter in voters {
        let ballot = ballots_read(&deps.storage, proposal_id).may_load(&voter.addr.as_bytes()).unwrap();
        if ballot.is_some() {
            let vote_info = VoteInfo {
                proposal_id,
                voter: voter.addr,
                vote: ballot.unwrap().vote,
                weight: voter.weight,
            };
            votes.push(vote_info);
        } 
    }
    Ok(VoteListResponse { votes })
}

fn query_voter<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    voter: String
) -> StdResult<VoterResponse> {
    // TODO: Implement address validation
    let weight = voters_read(&deps.storage).may_load(&voter.as_bytes())?;
    Ok(VoterResponse { weight })
}

fn list_voters<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    _start_after: Option<String>,
    _limit: Option<u32>,
) -> StdResult<VoterListResponse> {
    // Currently no use for start_after or limit 
    // Returns the full list of voters
    let voters = voters_list_read(&deps.storage).load()?;
    Ok(VoterListResponse { voters })
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::testing::{mock_dependencies, mock_env};
    use cosmwasm_std::{coin, coins, from_binary, BankMsg, Coin, MessageInfo, HumanAddr};

    use crate::expiration::Duration;
    use crate::threshold::{Threshold, ThresholdError};
    use crate::math::{Decimal, Uint128};
    use crate::msg::Voter;

    use super::*;

    fn mock_env_height(height_delta: u64) -> Env {
        let mut env = mock_env(OWNER, &[]);
        env.block.height += height_delta;
        env
    }

    fn mock_env_time(time_delta: u64) -> Env {
        let mut env = mock_env(OWNER, &[]);
        env.block.time = env.block.time + time_delta;
        env
    }

    const OWNER: &str = "admin0001";
    const VOTER1: &str = "voter0001";
    const VOTER2: &str = "voter0002";
    const VOTER3: &str = "voter0003";
    const VOTER4: &str = "voter0004";
    const VOTER5: &str = "voter0005";
    const NOWEIGHT_VOTER: &str = "voterxxxx";
    const SOMEBODY: &str = "somebody";

    fn voter<T: Into<String>>(addr: T, weight: u64) -> Voter {
        Voter {
            addr: addr.into(),
            weight,
        }
    }

    // this will set up the instantiation for other tests
    #[track_caller]
    fn setup_test_case<S: Storage, A: Api, Q: Querier>(
        deps: &mut Extern<S, A, Q>,
        info: MessageInfo,
        threshold: Threshold,
        max_voting_period: Duration,
    ) -> Result<InitResponse<Empty>, ContractError> {
        // Instantiate a contract with voters
        let voters = vec![
            voter(&info.sender.to_string(), 1),
            voter(VOTER1, 1),
            voter(VOTER2, 2),
            voter(VOTER3, 3),
            voter(VOTER4, 4),
            voter(VOTER5, 5),
            voter(NOWEIGHT_VOTER, 0),
        ];

        let init_msg = InitMsg {
            voters,
            threshold,
            max_voting_period,
        };
        init(deps, mock_env(OWNER, &[]), init_msg)
    }

    fn get_tally<S: Storage, A: Api, Q: Querier>(deps: &Extern<S,A,Q>, proposal_id: u64) -> u64 {
        // Get all the voters on the proposal
        let voters = QueryMsg::ListVotes {
            proposal_id,
            start_after: None,
            limit: None,
        };
        let votes: VoteListResponse =
            from_binary(&query(deps, voters).unwrap()).unwrap();
        // Sum the weights of the Yes votes to get the tally
        votes
            .votes
            .iter()
            .filter(|&v| v.vote == Vote::Yes)
            .map(|v| v.weight)
            .sum()
    }

    #[test]
    fn test_init_works() {
        let mut deps = mock_dependencies(6,&[]);
        let info = MessageInfo {sender: HumanAddr::from(OWNER), sent_funds: vec![]};

        let max_voting_period = Duration::Time(1234567);

        // No voters fails
        let init_msg = InitMsg {
            voters: vec![],
            threshold: Threshold::ThresholdQuorum {
                threshold: Decimal::zero(),
                quorum: Decimal::percent(1),
            },
            max_voting_period,
        };
        let err = init(
            &mut deps,
            mock_env(OWNER, &[]),
            init_msg.clone(),
        )
        .unwrap_err();
        assert_eq!(err, ContractError::NoVoters {});

        // Zero required weight fails
        let init_msg = InitMsg {
            voters: vec![voter(OWNER, 1)],
            ..init_msg
        };
        let err =
            init(&mut deps, mock_env(OWNER, &[]), init_msg).unwrap_err();
        assert_eq!(
            err,
            ContractError::Threshold(ThresholdError::InvalidThreshold {})
        );

        // Total weight less than required weight not allowed
        let threshold = Threshold::AbsoluteCount { weight: 100 };
        let err =
            setup_test_case(&mut deps, info.clone(), threshold, max_voting_period).unwrap_err();
        assert_eq!(
            err,
            ContractError::Threshold(ThresholdError::UnreachableWeight {})
        );

        // All valid
        let threshold = Threshold::AbsoluteCount { weight: 1 };
        setup_test_case(&mut deps, info.clone(), threshold, max_voting_period).unwrap();

    }

    #[test]
    fn zero_weight_member_cant_vote() {
        let mut deps = mock_dependencies(6,&[]);

        let threshold = Threshold::AbsoluteCount { weight: 4 };
        let voting_period = Duration::Time(2000000);

        let info = MessageInfo {sender: HumanAddr::from(OWNER), sent_funds: vec![]};
        setup_test_case(&mut deps, info, threshold, voting_period).unwrap();

        let bank_msg = BankMsg::Send {
            from_address: OWNER.into(),
            to_address: SOMEBODY.into(),
            amount: vec![coin(1, "BTC")],
        };
        let msgs = vec![CosmosMsg::Bank(bank_msg)];

        // Voter without voting power still can create proposal
        let proposal = HandleMsg::Propose {
            title: "Rewarding somebody".to_string(),
            description: "Do we reward her?".to_string(),
            msgs,
            latest: None,
        };
        let res = handle( &mut deps, mock_env(NOWEIGHT_VOTER, &[]), proposal).unwrap();

        // Get the proposal id from the logs
        let proposal_id: u64 = res.log[2].value.parse().unwrap();

        // Cast a No vote
        let no_vote = HandleMsg::Vote {
            proposal_id,
            vote: Vote::No,
        };
        // Only voters with weight can vote
        let err = handle(&mut deps, mock_env(NOWEIGHT_VOTER, &[]), no_vote).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});
    }

    #[test]
    fn test_propose_works() {
        let mut deps = mock_dependencies(6,&[]);

        let threshold = Threshold::AbsoluteCount { weight: 4 };
        let voting_period = Duration::Time(2000000);

        let info = MessageInfo {sender: HumanAddr::from(OWNER), sent_funds: vec![]};
        setup_test_case(&mut deps, info.clone(), threshold, voting_period).unwrap();

        let bank_msg = BankMsg::Send {
            from_address: OWNER.into(),
            to_address: SOMEBODY.into(),
            amount: vec![coin(1, "BTC")],
        };
        let msgs = vec![CosmosMsg::Bank(bank_msg)];

        // Only voters can propose
        let proposal = HandleMsg::Propose {
            title: "Rewarding somebody".to_string(),
            description: "Do we reward her?".to_string(),
            msgs: msgs.clone(),
            latest: None,
        };
        let err = handle(&mut deps, mock_env(SOMEBODY, &[]), proposal.clone()).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});

        // Wrong expiration option fails
        let proposal_wrong_exp = HandleMsg::Propose {
            title: "Rewarding somebody".to_string(),
            description: "Do we reward her?".to_string(),
            msgs,
            latest: Some(Expiration::AtHeight(123456)),
        };
        let err = handle(&mut deps, mock_env(OWNER, &[]), proposal_wrong_exp).unwrap_err();
        assert_eq!(err, ContractError::WrongExpiration {});

        // Proposal from voter works
        let res = handle( &mut deps, mock_env(VOTER3, &[]), proposal.clone()).unwrap();

        // Verify
        assert_eq!(
            res,
            HandleResponse {
                messages: vec![],
                log: vec![
                    log("action","propose"),
                    log("sender", VOTER3),
                    log("proposal_id", 1u8.to_string()),
                    log("status", "Open")],
                data: None
            }
        );

        // Proposal from voter with enough vote power directly passes
        let res = handle(&mut deps, mock_env(VOTER4, &[]), proposal).unwrap();

        // Verify
        assert_eq!(
            res,
            HandleResponse {
                messages: vec![],
                log: vec![
                    log("action","propose"),
                    log("sender", VOTER4),
                    log("proposal_id", 2u8.to_string()),
                    log("status", "Passed")],
                data: None
            }
        );
    }

    #[test]
    fn test_vote_works() {
        let mut deps = mock_dependencies(6,&[]);

        let threshold = Threshold::AbsoluteCount { weight: 3 };
        let voting_period = Duration::Time(2000000);

        let info = MessageInfo {sender: HumanAddr::from(OWNER), sent_funds: vec![]};
        setup_test_case(&mut deps, info.clone(), threshold, voting_period).unwrap();

        // Propose
        let bank_msg = BankMsg::Send {
            from_address: OWNER.into(),
            to_address: SOMEBODY.into(),
            amount: vec![coin(1, "BTC")],
        };
        let msgs = vec![CosmosMsg::Bank(bank_msg)];
        let proposal = HandleMsg::Propose {
            title: "Pay somebody".to_string(),
            description: "Do I pay her?".to_string(),
            msgs,
            latest: None,
        };
        let res = handle(&mut deps, mock_env(OWNER, &[]), proposal).unwrap();

        // Get the proposal id from the logs
        let proposal_id: u64 = res.log[2].value.parse().unwrap();

        // Owner cannot vote (again)
        let yes_vote = HandleMsg::Vote {
            proposal_id,
            vote: Vote::Yes,
        };
        let err = handle(&mut deps, mock_env(OWNER, &[]), yes_vote.clone()).unwrap_err();
        assert_eq!(err, ContractError::AlreadyVoted {});

        // Only voters can vote
        let err = handle(&mut deps, mock_env(SOMEBODY, &[]), yes_vote.clone()).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});

        // But voter1 can
        let res = handle(&mut deps, mock_env(VOTER1, &[]), yes_vote.clone()).unwrap();

        // Verify
        assert_eq!(
            res,
            HandleResponse {
                messages: vec![],
                log: vec![
                    log("action","vote"),
                    log("sender", VOTER1),
                    log("proposal_id", proposal_id.to_string()),
                    log("status", "Open")],
                data: None
            }
        );

        // No/Veto votes have no effect on the tally
        // Get the proposal id from the logs
        let proposal_id: u64 = res.log[2].value.parse().unwrap();

        // Compute the current tally
        let tally = get_tally(&mut deps, proposal_id);

        // Cast a No vote
        let no_vote = HandleMsg::Vote {
            proposal_id,
            vote: Vote::No,
        };
        handle(&mut deps, mock_env(VOTER2, &[]), no_vote.clone()).unwrap();

        // Cast a Veto vote
        let veto_vote = HandleMsg::Vote {
            proposal_id,
            vote: Vote::Veto,
        };
        handle(&mut deps, mock_env(VOTER3, &[]), veto_vote).unwrap();

        // Verify
        assert_eq!(tally, get_tally(&deps, proposal_id));

        // Once voted, votes cannot be changed
        let err = handle(&mut deps, mock_env(VOTER3, &[]), yes_vote.clone()).unwrap_err();
        assert_eq!(err, ContractError::AlreadyVoted {});
        assert_eq!(tally, get_tally(&deps, proposal_id));

        // Expired proposals cannot be voted
        let env = match voting_period {
            Duration::Time(duration) => mock_env_time(duration + 1),
            Duration::Height(duration) => mock_env_height(duration + 1),
        };
        let err = handle(&mut deps, env, no_vote).unwrap_err();
        assert_eq!(err, ContractError::Expired {});

        // Vote it again, so it passes
        let res = handle(&mut deps, mock_env(VOTER4, &[]), yes_vote.clone()).unwrap();

        // Verify
        assert_eq!(
            res,
            HandleResponse {
                messages: vec![],
                log: vec![
                    log("action","vote"),
                    log("sender", VOTER4),
                    log("proposal_id", proposal_id.to_string()),
                    log("status", "Passed")],
                data: None
            }
        );

        // non-Open proposals cannot be voted
        let err = handle(&mut deps, mock_env(VOTER5, &[]), yes_vote).unwrap_err();
        assert_eq!(err, ContractError::NotOpen {});

        // Propose
        let bank_msg = BankMsg::Send {
            from_address: OWNER.into(),
            to_address: SOMEBODY.into(),
            amount: vec![coin(1, "BTC")],
        };
        let msgs = vec![CosmosMsg::Bank(bank_msg)];
        let proposal = HandleMsg::Propose {
            title: "Pay somebody".to_string(),
            description: "Do I pay her?".to_string(),
            msgs,
            latest: None,
        };
        let res = handle(&mut deps, mock_env(OWNER, &[]), proposal).unwrap();

        // Get the proposal id from the logs
        let proposal_id: u64 = res.log[2].value.parse().unwrap();

        // Cast a No vote
        let no_vote = HandleMsg::Vote {
            proposal_id,
            vote: Vote::No,
        };
        // Voter1 vote no, weight 1
        let res = handle(&mut deps, mock_env(VOTER1, &[]), no_vote.clone()).unwrap();

        // Verify it is not enough to reject yet
        assert_eq!(
            res,
            HandleResponse {
                messages: vec![],
                log: vec![
                    log("action","vote"),
                    log("sender", VOTER1),
                    log("proposal_id", proposal_id.to_string()),
                    log("status", "Open")],
                data: None
            }
        );

        // Voter 4 votes no, weight 4, total weight for no so far 5, need 14 to reject
        let res = handle(&mut deps, mock_env(VOTER4, &[]), no_vote.clone()).unwrap();

        // Verify it is still open as we actually need no votes > 16 - 3
        assert_eq!(
            res,
            HandleResponse {
                messages: vec![],
                log: vec![
                    log("action","vote"),
                    log("sender", VOTER4),
                    log("proposal_id", proposal_id.to_string()),
                    log("status", "Open")],
                data: None
            }
        );

        // Voter 3 votes no, weight 3, total weight for no far 8, need 14
        let _res = handle(&mut deps, mock_env(VOTER3, &[]), no_vote.clone()).unwrap();

        // Voter 5 votes no, weight 5, total weight for no far 13, need 14
        let res = handle(&mut deps, mock_env(VOTER5, &[]), no_vote.clone()).unwrap();

        // Verify it is still open as we actually need no votes > 16 - 3
        assert_eq!(
            res,
            HandleResponse {
                messages: vec![],
                log: vec![
                    log("action","vote"),
                    log("sender", VOTER5),
                    log("proposal_id", proposal_id.to_string()),
                    log("status", "Open")],
                data: None
            }
        );

        // Voter 2 votes no, weight 2, total weight for no so far 15, need 14.
        // Can now reject
        let res = handle(&mut deps, mock_env(VOTER2, &[]), no_vote).unwrap();

        // Verify it is rejected as, 15 no votes > 16 - 3
        assert_eq!(
            res,
            HandleResponse {
                messages: vec![],
                log: vec![
                    log("action","vote"),
                    log("sender", VOTER2),
                    log("proposal_id", proposal_id.to_string()),
                    log("status", "Rejected")],
                data: None
            }
        );
    }

    #[test]
    fn test_execute_works() {
        let mut deps = mock_dependencies(6,&[]);

        let threshold = Threshold::AbsoluteCount { weight: 3 };
        let voting_period = Duration::Time(2000000);

        let info = MessageInfo {sender: HumanAddr::from(OWNER), sent_funds: vec![]};
        setup_test_case(&mut deps, info.clone(), threshold, voting_period).unwrap();

        // Propose
        let bank_msg = BankMsg::Send {
            from_address: OWNER.into(),
            to_address: SOMEBODY.into(),
            amount: vec![coin(1, "BTC")],
        };
        let msgs = vec![CosmosMsg::Bank(bank_msg)];
        let proposal = HandleMsg::Propose {
            title: "Pay somebody".to_string(),
            description: "Do I pay her?".to_string(),
            msgs: msgs.clone(),
            latest: None,
        };
        let res = handle(&mut deps, mock_env(OWNER, &[]), proposal).unwrap();

        // Get the proposal id from the logs
        let proposal_id: u64 = res.log[2].value.parse().unwrap();

        // Only Passed can be executed
        let execution = HandleMsg::Execute { proposal_id };
        let err = handle(&mut deps, mock_env(OWNER, &[]), execution.clone()).unwrap_err();
        assert_eq!(err, ContractError::WrongExecuteStatus {});

        // Vote it, so it passes
        let vote = HandleMsg::Vote {
            proposal_id,
            vote: Vote::Yes,
        };
        let res = handle(&mut deps, mock_env(VOTER3, &[]), vote).unwrap();

        // Verify
        assert_eq!(
            res,
            HandleResponse {
                messages: vec![],
                log: vec![
                    log("action","vote"),
                    log("sender", VOTER3),
                    log("proposal_id", proposal_id.to_string()),
                    log("status", "Passed")],
                data: None
            }
        );

        // In passing: Try to close Passed fails
        let closing = HandleMsg::Close { proposal_id };
        let err = handle(&mut deps, mock_env(VOTER3, &[]), closing).unwrap_err();
        assert_eq!(err, ContractError::WrongCloseStatus {});

        // Execute works. Anybody can execute Passed proposals
        let res = handle(&mut deps, mock_env(SOMEBODY, &[]), execution).unwrap();

        // Verify
        assert_eq!(
            res,
            HandleResponse {
                messages: msgs,
                log: vec![
                    log("action","execute"),
                    log("sender", SOMEBODY),
                    log("proposal_id", proposal_id.to_string())],
                data: None
            }
        );

        // In passing: Try to close Executed fails
        let closing = HandleMsg::Close { proposal_id };
        let err = handle(&mut deps, mock_env(SOMEBODY, &[]), closing).unwrap_err();
        assert_eq!(err, ContractError::WrongCloseStatus {});
    }

    #[test]
    fn test_close_works() {
        let mut deps = mock_dependencies(6,&[]);

        let threshold = Threshold::AbsoluteCount { weight: 3 };
        let voting_period = Duration::Height(2000000);

        let info = MessageInfo {sender: HumanAddr::from(OWNER), sent_funds: vec![]};
        setup_test_case(&mut deps, info.clone(), threshold, voting_period).unwrap();

        // Propose
        let bank_msg = BankMsg::Send {
            from_address: OWNER.into(),
            to_address: SOMEBODY.into(),
            amount: vec![coin(1, "BTC")],
        };
        let msgs = vec![CosmosMsg::Bank(bank_msg)];
        let proposal = HandleMsg::Propose {
            title: "Pay somebody".to_string(),
            description: "Do I pay her?".to_string(),
            msgs: msgs.clone(),
            latest: None,
        };
        let res = handle(&mut deps, mock_env(OWNER, &[]), proposal).unwrap();

        // Get the proposal id from the logs
        let proposal_id: u64 = res.log[2].value.parse().unwrap();

        let closing = HandleMsg::Close { proposal_id };

        // Non-expired proposals cannot be closed
        let err = handle(&mut deps, mock_env(SOMEBODY, &[]), closing).unwrap_err();
        assert_eq!(err, ContractError::NotExpired {});

        // Expired proposals can be closed
        let proposal = HandleMsg::Propose {
            title: "(Try to) pay somebody".to_string(),
            description: "Pay somebody after time?".to_string(),
            msgs,
            latest: Some(Expiration::AtHeight(123456)),
        };
        let res = handle(&mut deps, mock_env(OWNER, &[]), proposal).unwrap();

        // Get the proposal id from the logs
        let proposal_id: u64 = res.log[2].value.parse().unwrap();

        let closing = HandleMsg::Close { proposal_id };

        // Close expired works
        let env = mock_env_height(1234567);
        let res = handle(
            &mut deps,
            env,
            closing.clone(),
        )
        .unwrap();

        // Verify
        assert_eq!(
            res,
            HandleResponse {
                messages: vec![],
                log: vec![
                    log("action","close"),
                    log("sender", OWNER),
                    log("proposal_id", proposal_id.to_string())],
                data: None
            }
        );

        // Trying to close it again fails
        let err = handle(&mut deps, mock_env(SOMEBODY, &[]), closing).unwrap_err();
        assert_eq!(err, ContractError::WrongCloseStatus {});
    }
}
