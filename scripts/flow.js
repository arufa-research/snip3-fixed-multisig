// deploy contract

// make a deposit

// wait some time and do a ClaimAndStake

// wait some time and query exch rate, info, dev fee

// do a user withdraw, this will start the pending claims window

// query current withdraw window (3 days one), pending claims

// claim the rewards after pending claim window is finished (21 + 3 days),
// this will claim from validator to contract and then transfer SCRT from contract to user's wallet

const { Contract, getAccountByName } = require("secret-polar");
const { validators } = require("./validators_pulsar.json");
const { fromUtf8 } = require("@iov/encoding");

async function run () {
  const runTs = String(new Date());
  const contract_owner = getAccountByName("account_0");
  const other_account = getAccountByName("account_1");

  const staking_token = new Contract('staking-token');
  // const gov_token = new Contract('gov-token');
  const staking_contract = new Contract('staking-contract');

  await staking_token.parseSchema();
  // await gov_token.parseSchema();
  await staking_contract.parseSchema();

  // deploy staking token, $seSCRT
  const staking_token_deploy_res = await staking_token.deploy(
    contract_owner, 
    {
      amount: [{ amount: "1000000", denom: "uscrt" }],
      gas: "4000000",
    }
  );
  console.log(staking_token_deploy_res);
  
  // add all the validators from validators_<network>.json file
  // console.log(validators);

  // deploy staking contract
  const staking_contract_deploy_res = await staking_contract.deploy(
    contract_owner, 
    {
      amount: [{ amount: "1000000", denom: "uscrt" }],
      gas: "4000000",
    }
  );
  console.log(staking_contract_deploy_res);
  // init staking contract
  const staking_contract_info = await staking_contract.instantiate(
    {
      "token_code_id": parseInt(staking_token.codeId),
      "token_code_hash": staking_token.contractCodeHash,
      "top_validator_code_hash": "a4491bb1fd86369ff91598a50b26c2c9a40772e977417fcc29e494a258e248df",
      "top_validator_contract_addr": "secret16k6sm2dph2juh9l5seqhjte3qvpar9sh8pzvvm",
      "label": `SE staking token ${runTs}`,  // label for staking token init
      "dev_address": contract_owner.account.address,
      "prng_seed": "GDShgdiu",
      "contract_viewing_key": "eyfy5ftF",
      "threshold": "100000",  // 0.1 SCRT
      // "dev_fee": 10000,   // 10%
    },
    `SE staking contract ${runTs}`,
    contract_owner
  );
  console.log(staking_contract_info);

  const delay = ms => new Promise(res => setTimeout(res, ms));

  const staking_info_1 = await staking_contract.query.info();
  console.log(JSON.stringify(staking_info_1, null, 2));

  // set staking_token addr from 
  staking_token.instantiatedWithAddress(staking_info_1.info.token_address);

  const top_validators = await staking_contract.query.validator_list();
  console.log(JSON.stringify(top_validators, null, 2));

  // const add_validator_res = await staking_contract.tx.add_validator(
  //   {account: contract_owner},
  //   {address: validators[1]["address"]}
  // );
  // console.log(JSON.stringify(add_validator_res, null, 2));

  // const transferAmount_1 = [{"denom": "uscrt", "amount": "2000000"}] // 2 SCRT
  // const stake_res = await staking_contract.tx.stake(
  //   {account: contract_owner, transferAmount: transferAmount_1}
  // );
  // console.log(JSON.stringify(stake_res, null, 2));

  const transferAmount_2 = [{"denom": "uscrt", "amount": "4000000"}] // 4 SCRT
  const other_stake_res = await staking_contract.tx.stake(
    {account: other_account, transferAmount: transferAmount_2}
  );
  // console.log(JSON.stringify(other_stake_res, null, 2));

  const staking_info_2 = await staking_contract.query.info();
  console.log(JSON.stringify(staking_info_2, null, 2));

  const claim_and_stake_res = await staking_contract.tx.claim_and_stake(
    {account: contract_owner}
  );
  console.log(JSON.stringify(claim_and_stake_res, null, 2));
  
  const staking_info_3 = await staking_contract.query.info();
  console.log(JSON.stringify(staking_info_3, null, 2));

  // const viewing_key_data = await staking_token.tx.create_viewing_key(
  //   {account: contract_owner},
  //   {entropy: `${runTs}`}
  // );
  // const viewing_key = JSON.parse(fromUtf8(viewing_key_data.data)).create_viewing_key.key;

  const other_viewing_key_data = await staking_token.tx.create_viewing_key(
    {account: other_account},
    {entropy: `${runTs}`}
  );
  const other_viewing_key = JSON.parse(fromUtf8(other_viewing_key_data.data)).create_viewing_key.key;

  const sescrt_balance_before = await staking_token.query.balance(
    {
      address: other_account.account.address,
      key: other_viewing_key,
    }
  );
  console.log(JSON.stringify(sescrt_balance_before, null, 2));

  const transferAmount_sescrt = "1000000" // 1 seSCRT
  await staking_token.tx.send(
    { account: other_account },
    { amount: transferAmount_sescrt, recipient: staking_contract.contractAddress }
  );
  const staking_window_1 = await staking_contract.query.window();
  console.log(JSON.stringify(staking_window_1, null, 2));
  await delay(240000);
  const adv_window_res = await staking_contract.tx.advance_window(
    {account: contract_owner}
  );
  console.log("adv_window_res", JSON.stringify(adv_window_res, null, 2));
  await delay(1000000);
  await staking_contract.tx.advance_window(
    {account: contract_owner}
  );
  const sescrt_balance_after = await staking_token.query.balance(
    {
      address: other_account.account.address,
      key: other_viewing_key,
    }
  );
  console.log(JSON.stringify(sescrt_balance_after, null, 2));

  const staking_info_4 = await staking_contract.query.info();
  console.log(JSON.stringify(staking_info_4, null, 2));

  const staking_window_2 = await staking_contract.query.window();
  console.log(JSON.stringify(staking_window_2, null, 2));

  const user_claimable = await staking_contract.query.user_claimable({address: other_account.account.address});
  console.log(JSON.stringify(user_claimable, null, 2));
  
  await staking_contract.tx.claim(
    { account: other_account }
  );
  // const account_bal = await contract_owner.getBalance();
  // console.log(JSON.stringify(account_bal, null, 2));
}

module.exports = { default: run };
