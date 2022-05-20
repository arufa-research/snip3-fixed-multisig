use std::cmp::Ordering;

use cosmwasm_std::{
    log, debug_print, to_binary, Api, Binary, Env, Extern, HandleResponse, InitResponse, Querier,
    StdError, StdResult, Storage, CosmosMsg, Empty };

// use crate::error::ContractError;
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
    env: Env,
    msg: InitMsg,
) -> Result<InitResponse, StdError> {
    if msg.voters.is_empty() {
        return Err(StdError::generic_err("No voters"));
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
    proposal_count(&mut deps.storage).save(&0);

    Ok(InitResponse::default())
}

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: HandleMsg,
) -> Result<HandleResponse<Empty>, StdError> {
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
) -> Result<HandleResponse<Empty>, StdError> {
    // only members of the multisig can create a proposal
    let vote_power: u64 = voters_read(&deps.storage)
        .may_load(&env.message.sender.to_string().as_bytes())?
        .ok_or(StdError::generic_err("Unauthorized"))?;

    let cfg = config_read(&deps.storage).load()?;

    // max expires also used as default
    let max_expires = cfg.max_voting_period.after(&env.block);
    let mut expires = latest.unwrap_or(max_expires);
    let comp = expires.partial_cmp(&max_expires);
    if let Some(Ordering::Greater) = comp {
        expires = max_expires;
    } else if comp.is_none() {
        return Err(StdError::generic_err("Wrong expiration option"));
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
            log("sender","info"),
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
) -> Result<HandleResponse<Empty>, StdError> {
    // only members of the multisig with weight >= 1 can vote
    let voter_power = voters_read(&deps.storage).may_load(&env.message.sender.to_string().as_bytes())?;
    let vote_power = match voter_power {
        Some(power) if power >= 1 => power,
        _ => return Err(StdError::unauthorized()),
    };

    // ensure proposal exists and can be voted on
    let mut prop = proposals_read(&deps.storage).load(&proposal_id.to_le_bytes())?;
    if prop.status != Status::Open {
        return Err(StdError::generic_err("Proposal is not open"));
    }
    if prop.expires.is_expired(&env.block) {
        return Err(StdError::generic_err("Proposal voting period has expired"));
    }

    // TODO check if the person has already voted before trying to save
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
) -> Result<HandleResponse, StdError> {
    // anyone can trigger this if the vote passed

    let mut prop = proposals_read(&deps.storage).load(&proposal_id.to_le_bytes())?;
    // we allow execution even after the proposal "expiration" as long as all vote come in before
    // that point. If it was approved on time, it can be executed any time.
    if prop.status != Status::Passed {
        return Err(StdError::generic_err("Proposal must have passed and not yet been executed"));
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
) -> Result<HandleResponse<Empty>, StdError> {
    // anyone can trigger this if the vote passed

    let mut prop = proposals_read(&deps.storage).load(&proposal_id.to_le_bytes())?;
    if [Status::Executed, Status::Rejected, Status::Passed]
        .iter()
        .any(|x| *x == prop.status)
    {
        return Err(StdError::generic_err("Cannot close completed or passed proposals"));
    }
    if !prop.expires.is_expired(&env.block) {
        return Err(StdError::generic_err("Proposal must expire before you can close it"));
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
// const MAX_LIMIT: u32 = 30;
// const DEFAULT_LIMIT: u32 = 10;
// let's figure out the limit stuff later
// for now, return the full list every time

fn list_proposals<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    start_after: Option<u64>,
    limit: Option<u32>,
) -> StdResult<ProposalListResponse> {
    let latest_prop = proposal_count_read(&deps.storage).load()?;

    // let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT).into();
    // let start = start_after.unwrap_or(1);
    let start = 1;
    let limit = latest_prop;
    
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

    // let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT).into();
    // let start = start_before.unwrap_or(latest_prop);
    let start = latest_prop;
    let limit = 1;
    
    let mut proposals: Vec<ProposalResponse> = vec![];
    let mut i = start;
    while i >= limit {
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
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<VoteListResponse> {
    // let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    // let start = start_after.unwrap_or_default().to_string(); //not sure about this

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
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<VoterListResponse> {
    // let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    // let start = start_after.unwrap_or_default();
    let voters = voters_list_read(&deps.storage).load()?;
    Ok(VoterListResponse { voters })
}

// #[cfg(test)]
// mod tests {
//     use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
//     use cosmwasm_std::{coin, from_binary, BankMsg, Decimal};

//     use crate::expiration::Duration;
//     use crate::threshold::Threshold;

//     use crate::msg::Voter;

//     use super::*;

//     fn mock_env_height(height_delta: u64) -> Env {
//         let mut env = mock_env();
//         env.block.height += height_delta;
//         env
//     }

//     fn mock_env_time(time_delta: u64) -> Env {
//         let mut env = mock_env();
//         env.block.time = env.block.time.plus_seconds(time_delta);
//         env
//     }

//     const OWNER: &str = "admin0001";
//     const VOTER1: &str = "voter0001";
//     const VOTER2: &str = "voter0002";
//     const VOTER3: &str = "voter0003";
//     const VOTER4: &str = "voter0004";
//     const VOTER5: &str = "voter0005";
//     const NOWEIGHT_VOTER: &str = "voterxxxx";
//     const SOMEBODY: &str = "somebody";

//     fn voter<T: Into<String>>(addr: T, weight: u64) -> Voter {
//         Voter {
//             addr: addr.into(),
//             weight,
//         }
//     }

//     // this will set up the instantiation for other tests
//     #[track_caller]
//     fn setup_test_case(
//         deps: DepsMut,
//         info: MessageInfo,
//         threshold: Threshold,
//         max_voting_period: Duration,
//     ) -> Result<Response<Empty>, ContractError> {
//         // Instantiate a contract with voters
//         let voters = vec![
//             voter(&info.sender, 1),
//             voter(VOTER1, 1),
//             voter(VOTER2, 2),
//             voter(VOTER3, 3),
//             voter(VOTER4, 4),
//             voter(VOTER5, 5),
//             voter(NOWEIGHT_VOTER, 0),
//         ];

//         let instantiate_msg = InstantiateMsg {
//             voters,
//             threshold,
//             max_voting_period,
//         };
//         instantiate(deps, mock_env(), info, instantiate_msg)
//     }

//     fn get_tally(deps: Deps, proposal_id: u64) -> u64 {
//         // Get all the voters on the proposal
//         let voters = QueryMsg::ListVotes {
//             proposal_id,
//             start_after: None,
//             limit: None,
//         };
//         let votes: VoteListResponse =
//             from_binary(&query(deps, voters).unwrap()).unwrap();
//         // Sum the weights of the Yes votes to get the tally
//         votes
//             .votes
//             .iter()
//             .filter(|&v| v.vote == Vote::Yes)
//             .map(|v| v.weight)
//             .sum()
//     }}

//     #[test]
//     fn test_instantiate_works() {
//         let mut deps = mock_dependencies();
//         let info = mock_info(OWNER, &[]);

//         let max_voting_period = Duration::Time(1234567);

//         // No voters fails
//         let instantiate_msg = InstantiateMsg {
//             voters: vec![],
//             threshold: Threshold::ThresholdQuorum {
//                 threshold: Decimal::zero(),
//                 quorum: Decimal::percent(1),
//             },
//             max_voting_period,
//         };
//         let err = instantiate(
//             deps.as_mut(),
//             mock_env(),
//             info.clone(),
//             instantiate_msg.clone(),
//         )
//         .unwrap_err();
//         assert_eq!(err, ContractError::NoVoters {});

//         // Zero required weight fails
//         let instantiate_msg = InstantiateMsg {
//             voters: vec![voter(OWNER, 1)],
//             ..instantiate_msg
//         };
//         let err =
//             instantiate(deps.as_mut(), mock_env(), info.clone(), instantiate_msg).unwrap_err();
//         assert_eq!(
//             err,
//             ContractError::Threshold(cw_utils::ThresholdError::InvalidThreshold {})
//         );

//         // Total weight less than required weight not allowed
//         let threshold = Threshold::AbsoluteCount { weight: 100 };
//         let err =
//             setup_test_case(deps.as_mut(), info.clone(), threshold, max_voting_period).unwrap_err();
//         assert_eq!(
//             err,
//             ContractError::Threshold(cw_utils::ThresholdError::UnreachableWeight {})
//         );

//         // All valid
//         let threshold = Threshold::AbsoluteCount { weight: 1 };
//         setup_test_case(deps.as_mut(), info, threshold, max_voting_period).unwrap();

//         // Verify
//         assert_eq!(
//             ContractVersion {
//                 contract: CONTRACT_NAME.to_string(),
//                 version: CONTRACT_VERSION.to_string(),
//             },
//             get_contract_version(&deps.storage).unwrap()
//         )
//     }

//     // TODO: query() tests

//     #[test]
//     fn zero_weight_member_cant_vote() {
//         let mut deps = mock_dependencies();

//         let threshold = Threshold::AbsoluteCount { weight: 4 };
//         let voting_period = Duration::Time(2000000);

//         let info = mock_info(OWNER, &[]);
//         setup_test_case(deps.as_mut(), info, threshold, voting_period).unwrap();

//         let bank_msg = BankMsg::Send {
//             to_address: SOMEBODY.into(),
//             amount: vec![coin(1, "BTC")],
//         };
//         let msgs = vec![CosmosMsg::Bank(bank_msg)];

//         // Voter without voting power still can create proposal
//         let info = mock_info(NOWEIGHT_VOTER, &[]);
//         let proposal = ExecuteMsg::Propose {
//             title: "Rewarding somebody".to_string(),
//             description: "Do we reward her?".to_string(),
//             msgs,
//             latest: None,
//         };
//         let res = execute(deps.as_mut(), mock_env(), info, proposal).unwrap();

//         // Get the proposal id from the logs
//         let proposal_id: u64 = res.attributes[2].value.parse().unwrap();

//         // Cast a No vote
//         let no_vote = ExecuteMsg::Vote {
//             proposal_id,
//             vote: Vote::No,
//         };
//         // Only voters with weight can vote
//         let info = mock_info(NOWEIGHT_VOTER, &[]);
//         let err = execute(deps.as_mut(), mock_env(), info, no_vote).unwrap_err();
//         assert_eq!(err, ContractError::Unauthorized {});
//     }

//     #[test]
//     fn test_propose_works() {
//         let mut deps = mock_dependencies();

//         let threshold = Threshold::AbsoluteCount { weight: 4 };
//         let voting_period = Duration::Time(2000000);

//         let info = mock_info(OWNER, &[]);
//         setup_test_case(deps.as_mut(), info, threshold, voting_period).unwrap();

//         let bank_msg = BankMsg::Send {
//             to_address: SOMEBODY.into(),
//             amount: vec![coin(1, "BTC")],
//         };
//         let msgs = vec![CosmosMsg::Bank(bank_msg)];

//         // Only voters can propose
//         let info = mock_info(SOMEBODY, &[]);
//         let proposal = ExecuteMsg::Propose {
//             title: "Rewarding somebody".to_string(),
//             description: "Do we reward her?".to_string(),
//             msgs: msgs.clone(),
//             latest: None,
//         };
//         let err = execute(deps.as_mut(), mock_env(), info, proposal.clone()).unwrap_err();
//         assert_eq!(err, ContractError::Unauthorized {});

//         // Wrong expiration option fails
//         let info = mock_info(OWNER, &[]);
//         let proposal_wrong_exp = ExecuteMsg::Propose {
//             title: "Rewarding somebody".to_string(),
//             description: "Do we reward her?".to_string(),
//             msgs,
//             latest: Some(Expiration::AtHeight(123456)),
//         };
//         let err = execute(deps.as_mut(), mock_env(), info, proposal_wrong_exp).unwrap_err();
//         assert_eq!(err, ContractError::WrongExpiration {});

//         // Proposal from voter works
//         let info = mock_info(VOTER3, &[]);
//         let res = execute(deps.as_mut(), mock_env(), info, proposal.clone()).unwrap();

//         // Verify
//         assert_eq!(
//             res,
//             Response::new()
//                 .add_attribute("action", "propose")
//                 .add_attribute("sender", VOTER3)
//                 .add_attribute("proposal_id", 1.to_string())
//                 .add_attribute("status", "Open")
//         );

//         // Proposal from voter with enough vote power directly passes
//         let info = mock_info(VOTER4, &[]);
//         let res = execute(deps.as_mut(), mock_env(), info, proposal).unwrap();

//         // Verify
//         assert_eq!(
//             res,
//             Response::new()
//                 .add_attribute("action", "propose")
//                 .add_attribute("sender", VOTER4)
//                 .add_attribute("proposal_id", 2.to_string())
//                 .add_attribute("status", "Passed")
//         );
//     }

//     #[test]
//     fn test_vote_works() {
//         let mut deps = mock_dependencies();

//         let threshold = Threshold::AbsoluteCount { weight: 3 };
//         let voting_period = Duration::Time(2000000);

//         let info = mock_info(OWNER, &[]);
//         setup_test_case(deps.as_mut(), info.clone(), threshold, voting_period).unwrap();

//         // Propose
//         let bank_msg = BankMsg::Send {
//             to_address: SOMEBODY.into(),
//             amount: vec![coin(1, "BTC")],
//         };
//         let msgs = vec![CosmosMsg::Bank(bank_msg)];
//         let proposal = ExecuteMsg::Propose {
//             title: "Pay somebody".to_string(),
//             description: "Do I pay her?".to_string(),
//             msgs,
//             latest: None,
//         };
//         let res = execute(deps.as_mut(), mock_env(), info.clone(), proposal).unwrap();

//         // Get the proposal id from the logs
//         let proposal_id: u64 = res.attributes[2].value.parse().unwrap();

//         // Owner cannot vote (again)
//         let yes_vote = ExecuteMsg::Vote {
//             proposal_id,
//             vote: Vote::Yes,
//         };
//         let err = execute(deps.as_mut(), mock_env(), info, yes_vote.clone()).unwrap_err();
//         assert_eq!(err, ContractError::AlreadyVoted {});

//         // Only voters can vote
//         let info = mock_info(SOMEBODY, &[]);
//         let err = execute(deps.as_mut(), mock_env(), info, yes_vote.clone()).unwrap_err();
//         assert_eq!(err, ContractError::Unauthorized {});

//         // But voter1 can
//         let info = mock_info(VOTER1, &[]);
//         let res = execute(deps.as_mut(), mock_env(), info, yes_vote.clone()).unwrap();

//         // Verify
//         assert_eq!(
//             res,
//             Response::new()
//                 .add_attribute("action", "vote")
//                 .add_attribute("sender", VOTER1)
//                 .add_attribute("proposal_id", proposal_id.to_string())
//                 .add_attribute("status", "Open")
//         );

//         // No/Veto votes have no effect on the tally
//         // Get the proposal id from the logs
//         let proposal_id: u64 = res.attributes[2].value.parse().unwrap();

//         // Compute the current tally
//         let tally = get_tally(deps.as_ref(), proposal_id);

//         // Cast a No vote
//         let no_vote = ExecuteMsg::Vote {
//             proposal_id,
//             vote: Vote::No,
//         };
//         let info = mock_info(VOTER2, &[]);
//         execute(deps.as_mut(), mock_env(), info, no_vote.clone()).unwrap();

//         // Cast a Veto vote
//         let veto_vote = ExecuteMsg::Vote {
//             proposal_id,
//             vote: Vote::Veto,
//         };
//         let info = mock_info(VOTER3, &[]);
//         execute(deps.as_mut(), mock_env(), info.clone(), veto_vote).unwrap();

//         // Verify
//         assert_eq!(tally, get_tally(deps.as_ref(), proposal_id));

//         // Once voted, votes cannot be changed
//         let err = execute(deps.as_mut(), mock_env(), info.clone(), yes_vote.clone()).unwrap_err();
//         assert_eq!(err, ContractError::AlreadyVoted {});
//         assert_eq!(tally, get_tally(deps.as_ref(), proposal_id));

//         // Expired proposals cannot be voted
//         let env = match voting_period {
//             Duration::Time(duration) => mock_env_time(duration + 1),
//             Duration::Height(duration) => mock_env_height(duration + 1),
//         };
//         let err = execute(deps.as_mut(), env, info, no_vote).unwrap_err();
//         assert_eq!(err, ContractError::Expired {});

//         // Vote it again, so it passes
//         let info = mock_info(VOTER4, &[]);
//         let res = execute(deps.as_mut(), mock_env(), info, yes_vote.clone()).unwrap();

//         // Verify
//         assert_eq!(
//             res,
//             Response::new()
//                 .add_attribute("action", "vote")
//                 .add_attribute("sender", VOTER4)
//                 .add_attribute("proposal_id", proposal_id.to_string())
//                 .add_attribute("status", "Passed")
//         );

//         // non-Open proposals cannot be voted
//         let info = mock_info(VOTER5, &[]);
//         let err = execute(deps.as_mut(), mock_env(), info, yes_vote).unwrap_err();
//         assert_eq!(err, ContractError::NotOpen {});

//         // Propose
//         let info = mock_info(OWNER, &[]);
//         let bank_msg = BankMsg::Send {
//             to_address: SOMEBODY.into(),
//             amount: vec![coin(1, "BTC")],
//         };
//         let msgs = vec![CosmosMsg::Bank(bank_msg)];
//         let proposal = ExecuteMsg::Propose {
//             title: "Pay somebody".to_string(),
//             description: "Do I pay her?".to_string(),
//             msgs,
//             latest: None,
//         };
//         let res = execute(deps.as_mut(), mock_env(), info, proposal).unwrap();

//         // Get the proposal id from the logs
//         let proposal_id: u64 = res.attributes[2].value.parse().unwrap();

//         // Cast a No vote
//         let no_vote = ExecuteMsg::Vote {
//             proposal_id,
//             vote: Vote::No,
//         };
//         // Voter1 vote no, weight 1
//         let info = mock_info(VOTER1, &[]);
//         let res = execute(deps.as_mut(), mock_env(), info, no_vote.clone()).unwrap();

//         // Verify it is not enough to reject yet
//         assert_eq!(
//             res,
//             Response::new()
//                 .add_attribute("action", "vote")
//                 .add_attribute("sender", VOTER1)
//                 .add_attribute("proposal_id", proposal_id.to_string())
//                 .add_attribute("status", "Open")
//         );

//         // Voter 4 votes no, weight 4, total weight for no so far 5, need 14 to reject
//         let info = mock_info(VOTER4, &[]);
//         let res = execute(deps.as_mut(), mock_env(), info, no_vote.clone()).unwrap();

//         // Verify it is still open as we actually need no votes > 16 - 3
//         assert_eq!(
//             res,
//             Response::new()
//                 .add_attribute("action", "vote")
//                 .add_attribute("sender", VOTER4)
//                 .add_attribute("proposal_id", proposal_id.to_string())
//                 .add_attribute("status", "Open")
//         );

//         // Voter 3 votes no, weight 3, total weight for no far 8, need 14
//         let info = mock_info(VOTER3, &[]);
//         let _res = execute(deps.as_mut(), mock_env(), info, no_vote.clone()).unwrap();

//         // Voter 5 votes no, weight 5, total weight for no far 13, need 14
//         let info = mock_info(VOTER5, &[]);
//         let res = execute(deps.as_mut(), mock_env(), info, no_vote.clone()).unwrap();

//         // Verify it is still open as we actually need no votes > 16 - 3
//         assert_eq!(
//             res,
//             Response::new()
//                 .add_attribute("action", "vote")
//                 .add_attribute("sender", VOTER5)
//                 .add_attribute("proposal_id", proposal_id.to_string())
//                 .add_attribute("status", "Open")
//         );

//         // Voter 2 votes no, weight 2, total weight for no so far 15, need 14.
//         // Can now reject
//         let info = mock_info(VOTER2, &[]);
//         let res = execute(deps.as_mut(), mock_env(), info, no_vote).unwrap();

//         // Verify it is rejected as, 15 no votes > 16 - 3
//         assert_eq!(
//             res,
//             Response::new()
//                 .add_attribute("action", "vote")
//                 .add_attribute("sender", VOTER2)
//                 .add_attribute("proposal_id", proposal_id.to_string())
//                 .add_attribute("status", "Rejected")
//         );
//     }

//     #[test]
//     fn test_execute_works() {
//         let mut deps = mock_dependencies();

//         let threshold = Threshold::AbsoluteCount { weight: 3 };
//         let voting_period = Duration::Time(2000000);

//         let info = mock_info(OWNER, &[]);
//         setup_test_case(deps.as_mut(), info.clone(), threshold, voting_period).unwrap();

//         // Propose
//         let bank_msg = BankMsg::Send {
//             to_address: SOMEBODY.into(),
//             amount: vec![coin(1, "BTC")],
//         };
//         let msgs = vec![CosmosMsg::Bank(bank_msg)];
//         let proposal = ExecuteMsg::Propose {
//             title: "Pay somebody".to_string(),
//             description: "Do I pay her?".to_string(),
//             msgs: msgs.clone(),
//             latest: None,
//         };
//         let res = execute(deps.as_mut(), mock_env(), info.clone(), proposal).unwrap();

//         // Get the proposal id from the logs
//         let proposal_id: u64 = res.attributes[2].value.parse().unwrap();

//         // Only Passed can be executed
//         let execution = ExecuteMsg::Execute { proposal_id };
//         let err = execute(deps.as_mut(), mock_env(), info, execution.clone()).unwrap_err();
//         assert_eq!(err, ContractError::WrongExecuteStatus {});

//         // Vote it, so it passes
//         let vote = ExecuteMsg::Vote {
//             proposal_id,
//             vote: Vote::Yes,
//         };
//         let info = mock_info(VOTER3, &[]);
//         let res = execute(deps.as_mut(), mock_env(), info.clone(), vote).unwrap();

//         // Verify
//         assert_eq!(
//             res,
//             Response::new()
//                 .add_attribute("action", "vote")
//                 .add_attribute("sender", VOTER3)
//                 .add_attribute("proposal_id", proposal_id.to_string())
//                 .add_attribute("status", "Passed")
//         );

//         // In passing: Try to close Passed fails
//         let closing = ExecuteMsg::Close { proposal_id };
//         let err = execute(deps.as_mut(), mock_env(), info, closing).unwrap_err();
//         assert_eq!(err, ContractError::WrongCloseStatus {});

//         // Execute works. Anybody can execute Passed proposals
//         let info = mock_info(SOMEBODY, &[]);
//         let res = execute(deps.as_mut(), mock_env(), info.clone(), execution).unwrap();

//         // Verify
//         assert_eq!(
//             res,
//             Response::new()
//                 .add_messages(msgs)
//                 .add_attribute("action", "execute")
//                 .add_attribute("sender", SOMEBODY)
//                 .add_attribute("proposal_id", proposal_id.to_string())
//         );

//         // In passing: Try to close Executed fails
//         let closing = ExecuteMsg::Close { proposal_id };
//         let err = execute(deps.as_mut(), mock_env(), info, closing).unwrap_err();
//         assert_eq!(err, ContractError::WrongCloseStatus {});
//     }

//     #[test]
//     fn test_close_works() {
//         let mut deps = mock_dependencies();

//         let threshold = Threshold::AbsoluteCount { weight: 3 };
//         let voting_period = Duration::Height(2000000);

//         let info = mock_info(OWNER, &[]);
//         setup_test_case(deps.as_mut(), info.clone(), threshold, voting_period).unwrap();

//         // Propose
//         let bank_msg = BankMsg::Send {
//             to_address: SOMEBODY.into(),
//             amount: vec![coin(1, "BTC")],
//         };
//         let msgs = vec![CosmosMsg::Bank(bank_msg)];
//         let proposal = ExecuteMsg::Propose {
//             title: "Pay somebody".to_string(),
//             description: "Do I pay her?".to_string(),
//             msgs: msgs.clone(),
//             latest: None,
//         };
//         let res = execute(deps.as_mut(), mock_env(), info, proposal).unwrap();

//         // Get the proposal id from the logs
//         let proposal_id: u64 = res.attributes[2].value.parse().unwrap();

//         let closing = ExecuteMsg::Close { proposal_id };

//         // Anybody can close
//         let info = mock_info(SOMEBODY, &[]);

//         // Non-expired proposals cannot be closed
//         let err = execute(deps.as_mut(), mock_env(), info, closing).unwrap_err();
//         assert_eq!(err, ContractError::NotExpired {});

//         // Expired proposals can be closed
//         let info = mock_info(OWNER, &[]);

//         let proposal = ExecuteMsg::Propose {
//             title: "(Try to) pay somebody".to_string(),
//             description: "Pay somebody after time?".to_string(),
//             msgs,
//             latest: Some(Expiration::AtHeight(123456)),
//         };
//         let res = execute(deps.as_mut(), mock_env(), info.clone(), proposal).unwrap();

//         // Get the proposal id from the logs
//         let proposal_id: u64 = res.attributes[2].value.parse().unwrap();

//         let closing = ExecuteMsg::Close { proposal_id };

//         // Close expired works
//         let env = mock_env_height(1234567);
//         let res = execute(
//             deps.as_mut(),
//             env,
//             mock_info(SOMEBODY, &[]),
//             closing.clone(),
//         )
//         .unwrap();

//         // Verify
//         assert_eq!(
//             res,
//             Response::new()
//                 .add_attribute("action", "close")
//                 .add_attribute("sender", SOMEBODY)
//                 .add_attribute("proposal_id", proposal_id.to_string())
//         );

//         // Trying to close it again fails
//         let err = execute(deps.as_mut(), mock_env(), info, closing).unwrap_err();
//         assert_eq!(err, ContractError::WrongCloseStatus {});
//     }
// }
