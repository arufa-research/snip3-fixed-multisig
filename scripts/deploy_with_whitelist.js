const { Contract, getAccountByName } = require("secret-polar");
const { validators } = require("./validators_pulsar.json");
const { fromUtf8 } = require("@iov/encoding");
const { addresses } = require("./whitelist.json");

async function run () {
  const runTs = String(new Date());
  const contract_owner = getAccountByName("admin");

  const staking_token = new Contract('staking-token');
  const staking_contract = new Contract('staking-contract');

  await staking_token.parseSchema();
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
      "dev_fee": 3000,   // 3%
      "sscrt_token_contract_hash": "9587d60b8e6b078ace12014ceeee089530b9fabcd76535d93666a6c127ad8813",
      "sscrt_address": "secret18vd8fpwxzck93qlwghaj6arh4p7c5n8978vsyg",
    },
    `SE staking contract ${runTs}`,
    contract_owner
  );
  console.log(staking_contract_info);

  const staking_info = await staking_contract.query.info();
  console.log(JSON.stringify(staking_info, null, 2));
  staking_token.instantiatedWithAddress(staking_info.info.token_address);
}

module.exports = { default: run };
