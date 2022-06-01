const { Contract, getAccountByName } = require("secret-polar");
const { validators } = require("./validators_pulsar.json");

async function run () {
  const contract_owner = getAccountByName("account_0");

  const staking_token = new Contract('staking-token');
  const gov_token = new Contract('gov-token');
  const staking_contract = new Contract('staking-contract');

  await staking_token.parseSchema();
  await gov_token.parseSchema();
  await staking_contract.parseSchema();

  // deploy staking token, $seSCRT
  const staking_token_deploy_res = await staking_token.deploy(contract_owner);
  console.log(staking_token_deploy_res);
  // init
  const staking_token_info = await contract.instantiate(
    {"count": 102},
    "SE staking token 1",
    contract_owner
  );
  console.log(staking_token_info);

  // deploy gov token, $SEASY
  // const gov_token_deploy_res = await gov_token.deploy(contract_owner);
  // console.log(gov_token_deploy_res);
  // init
  // const gov_token_info = await contract.instantiate(
  //   {"count": 102},
  //   "SE gov token 1",
  //   contract_owner
  // );
  // console.log(gov_token_info);

  // deploy staking contract
  const staking_contract_deploy_res = await staking_contract.deploy(contract_owner);
  console.log(staking_contract_deploy_res);
  // init
  // const staking_contract_info = await contract.instantiate(
  //   {"count": 102},
  //   "SE staking contract 1",
  //   contract_owner
  // );
  // console.log(staking_contract_info);

  // add all the validators from validators_<network>.json file
  console.log(validators);

  // const ex_response = await contract.tx.increment(contract_owner);
  // console.log(ex_response);

  // const response = await contract.query.get_count();
  // console.log(response);
}

module.exports = { default: run };
