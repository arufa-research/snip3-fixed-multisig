#![allow(unused)]

use std::cmp::Ordering;

use cosmwasm_std::{
    log, debug_print, to_binary, Api, Binary, Env, Extern, HandleResponse, InitResponse, Querier,
    StdError, StdResult, Storage, HumanAddr, MessageInfo, CosmosMsg, Empty, BlockInfo
};

use cosmwasm_std::{Order, KV};

use cosmwasm_storage::{ReadonlyBucket};
// use cw_storage_plus::Bound;
// use cw_utils::{Expiration, ThresholdResponse};

use crate::error::ContractError;
use crate::expiration::{Duration, Expiration};
use crate::msg::{HandleMsg, InitMsg, QueryMsg, Vote, Voter};
use crate::query::{
    ProposalListResponse, ProposalResponse, VoteInfo, VoteListResponse, VoteResponse,
    VoterDetail, VoterListResponse, VoterResponse, Status
};
use crate::state::{config, config_read, voters, voters_read, proposal_count, proposal_count_read,
                    ballots, ballots_read, proposals, proposals_read};
use crate::state::{Ballot, Config, Proposal, Votes};
use crate::threshold::ThresholdResponse;

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: InitMsg,
) -> Result<InitResponse, ContractError> {
    if msg.voters.is_empty() {
        return Err(ContractError::NoVoters {});
    }

    let total_weight = msg.voters.iter().map(|v| v.weight).sum();

    msg.threshold.validate(total_weight)?;

    let cfg = Config {
        threshold: msg.threshold,
        total_weight,
        max_voting_period: msg.max_voting_period,
    };

    config(&mut deps.storage).save(&cfg)?;

    // add all voters
    for voter in msg.voters.iter() {
        // I had to remove the addr_validate method because it's not available in the 0.10 API trait
        let key = voter.addr.as_bytes();
        voters(&mut deps.storage).save(key, &voter.weight)?;
    }

    debug_print!("Contract was initialized by {}", env.message.sender);

    Ok(InitResponse::default())
}

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    info: MessageInfo,
    msg: HandleMsg,
) -> Result<HandleResponse<Empty>, ContractError> {
    match msg {
        HandleMsg::Propose {
            title,
            description,
            msgs,
            latest,
        } => execute_propose(deps, env, info, title, description, msgs, latest),
        HandleMsg::Vote { proposal_id, vote } => execute_vote(deps, env, info, proposal_id, vote),
        HandleMsg::Execute { proposal_id } => execute_execute(deps, env, info, proposal_id),
        HandleMsg::Close { proposal_id } => execute_close(deps, env, info, proposal_id),
    }
}

pub fn execute_propose<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    info: MessageInfo,
    title: String,
    description: String,
    msgs: Vec<CosmosMsg>,
    // we ignore earliest
    latest: Option<Expiration>,
) -> Result<HandleResponse<Empty>, ContractError> {
    // only members of the multisig can create a proposal
    let vote_power = voters_read(&deps.storage)
        .may_load(&info.sender.to_string().as_bytes())?
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
    ballots(&mut deps.storage).save(&proposal_id.to_string().as_bytes(),&ballot)?;
    //need to figure out how to do the "double mapping"
    //ie. store both the proposal ID and the voter address...

    // TODO figure out how to do responses on secret
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
    info: MessageInfo,
    proposal_id: u64,
    vote: Vote,
) -> Result<HandleResponse<Empty>, ContractError> {
    // only members of the multisig with weight >= 1 can vote
    let voter_power = voters_read(&deps.storage).may_load(&info.sender.to_string().as_bytes())?;
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

    // cast vote if no vote previously cast
    ballots(&mut deps.storage).update(&proposal_id.to_string().as_bytes(), |bal| match bal{
        Some(_) => Err(StdError::GenericErr { msg: ("Already voted".to_string()), backtrace: (None) }), // TODO: figure out how to return ContractError::AlreadyVoted instead of StdError
        None => Ok(Ballot {
            weight: vote_power,
            vote,
        }),
    })?;

    // update vote tally
    prop.votes.add_vote(vote, vote_power);
    prop.update_status(&env.block);
    proposals(&mut deps.storage).save(&proposal_id.to_le_bytes(), &prop)?;

    Ok(HandleResponse {
        messages: vec![],
        log: vec![
            log("action","vote"),
            log("sender", info.sender),
            log("proposal_id", proposal_id.to_string()),
            log("status", format!("{:?}", prop.status))],
        data: None
    })
}

pub fn execute_execute<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    _env: Env,
    info: MessageInfo,
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
            log("sender", info.sender),
            log("proposal_id", proposal_id.to_string())],
        data: None
    })
}

pub fn execute_close<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    info: MessageInfo,
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
            log("sender", info.sender),
            log("proposal_id", proposal_id.to_string())],
        data: None
    })
}

// Queries and query functions
// TODO: fix up these functions:
// reverse_proposals, list_votes, list_voters

pub fn query<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    env: Env,
    msg: QueryMsg
) -> StdResult<Binary> {
    match msg {
        QueryMsg::Threshold {} => to_binary(&query_threshold(deps)?),
        QueryMsg::Proposal { proposal_id } => to_binary(&query_proposal(deps, env, proposal_id)?),
        QueryMsg::Vote { proposal_id, voter } => to_binary(&query_vote(deps, proposal_id, voter)?),
        QueryMsg::ListProposals { start_after, limit } => {
            to_binary(&list_proposals(deps, env, start_after, limit)?)
        },
        QueryMsg::ReverseProposals {
            start_before,
            limit,
        } => to_binary(&reverse_proposals(deps, env, start_before, limit)?),
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

fn query_proposal<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>, env: Env, id: u64) -> StdResult<ProposalResponse> {
    let prop = proposals_read(&deps.storage).load(&id.to_le_bytes())?;
    let status = prop.current_status(&env.block);
    let threshold = prop.threshold.to_response(prop.total_weight);
    Ok(ProposalResponse {
        id,
        title: prop.title,
        description: prop.description,
        msgs: prop.msgs,
        status,
        expires: prop.expires,
        threshold,
    })
}

// settings for pagination
const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;

fn list_proposals<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    env: Env,
    start_after: Option<u64>,
    limit: Option<u32>,
) -> StdResult<ProposalListResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = start_after.unwrap_or_default().to_string(); //not sure about this
    let proposals = proposals_read(&deps.storage)
        .range(Some(start.as_bytes()), None, Order::Ascending)
        .take(limit)
        .map(|p| map_proposal(&deps, &env.block, p))
        .collect::<StdResult<_>>()?;

    Ok(ProposalListResponse { proposals })
}

fn reverse_proposals<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    env: Env,
    start_before: Option<u64>,
    limit: Option<u32>,
) -> StdResult<ProposalListResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let end = start_before.unwrap_or_default().to_string(); //not sure about this
    let props: StdResult<Vec<_>> = proposals_read(&deps.storage)
        .range(None, Some(end.as_bytes()), Order::Descending)
        .take(limit)
        .map(|p| map_proposal(&deps, &env.block, p))
        .collect();

    Ok(ProposalListResponse { proposals: props? })
}

fn map_proposal<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    block: &BlockInfo,
    item: StdResult<(Vec<u8>, Proposal)>,
) -> StdResult<ProposalResponse> {
    item.map(|(prop_key, prop)| {
        let status = prop.current_status(block);
        let threshold = prop.threshold.to_response(prop.total_weight);
        let prop_bytes = &prop_key as &[u8];
        let prop_string = String::from_utf8(prop_key).unwrap(); //might panic!
        ProposalResponse {
            id: prop_string.parse::<u64>().unwrap(), //might panic!
            title: prop.title,
            description: prop.description,
            msgs: prop.msgs,
            status,
            expires: prop.expires,
            threshold,
        }
    })
}

fn query_vote<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    proposal_id: u64,
    voter: String
) -> StdResult<VoteResponse> {
    let voter = &voter; // TODO: figure out a way to validate the address
    // TODO: currently ballots_read only returns a Ballot structs per proposal_id key.
    // There's no way to differentiate the ballots per voter address.
    // Probably need to implement sub-buckets...
    let ballot = ballots_read(&deps.storage).may_load(proposal_id.to_string().as_bytes())?;
    // let ballot = BALLOTS.may_load(deps.storage, (proposal_id, &voter))?;
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
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = start_after.unwrap_or_default().to_string(); //not sure about this

    let votes = ballots_read(&deps.storage)
        // .prefix(proposal_id) //do I need to revamp the storage strategy first?
        .range(Some(start.as_bytes()), None, Order::Ascending)
        .take(limit)
        .map(|item| {
            item.map(|(addr, ballot)| VoteInfo {
                proposal_id,
                voter: String::from_utf8(addr).unwrap().into(), //might panic!,
                vote: ballot.vote,
                weight: ballot.weight,
            })
        })
        .collect::<StdResult<_>>()?;

    Ok(VoteListResponse { votes })
}

fn query_voter<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    voter: String
) -> StdResult<VoterResponse> {
    let voter = &voter; // TODO: figure out a way to validate the address
    let weight = voters_read(&deps.storage).may_load(&voter.to_string().as_bytes())?;
    Ok(VoterResponse { weight })
}

fn list_voters<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<VoterListResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = start_after.unwrap_or_default().to_string(); //not sure about this

    let voters = voters_read(&deps.storage)
        .range(Some(start.as_bytes()), None, Order::Ascending)
        .take(limit)
        .map(|item| {
            item.map(|(addr, weight)| VoterDetail {
                addr: String::from_utf8(addr).unwrap().into(), //might panic!
                weight,
            })
        })
        .collect::<StdResult<_>>()?;

    Ok(VoterListResponse { voters })
}

// #[cfg(test)]
// mod tests {
//     use cosmwasm_beta::testing::{mock_dependencies, mock_env, mock_info};
//     use cosmwasm_beta::{coin, from_binary, BankMsg, Decimal};

//     use cw2::{get_contract_version, ContractVersion};
//     use cw_utils::{Duration, Threshold};

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
//             from_binary(&query(deps, mock_env(), voters).unwrap()).unwrap();
//         // Sum the weights of the Yes votes to get the tally
//         votes
//             .votes
//             .iter()
//             .filter(|&v| v.vote == Vote::Yes)
//             .map(|v| v.weight)
//             .sum()
//     }

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
