const { Contract, getAccountByName } = require("secret-polar");
const { validators } = require("./validators_pulsar.json");

async function run() {
  const contract_owner = getAccountByName("account_2");

  const staking_token = new Contract('staking-token');
  //const gov_token = new Contract('gov-token');
  //const staking_contract = new Contract('staking-contract');

  await staking_token.parseSchema();
  //await gov_token.parseSchema();
  //await staking_contract.parseSchema();
  // const viewing_key_data0 = await staking_token.tx.create_viewing_key(
  //   { account: contract_owner },
  //   { entropy: "sdasadhhahhs" }
  // );
  // let str0 = new TextDecoder().decode(viewing_key_data0.data);
  // let viewing_key0 = JSON.parse(str0).create_viewing_key.key;

  // console.log(await staking_token.query.balance({"address": contract_owner.account.address, "key": viewing_key0}));

  const transferAmount_sescrt = "300000" // 1 Sienna
  const x = await staking_token.tx.send(
    { account: contract_owner },
    { amount: transferAmount_sescrt, recipient: "secret1qmzjf40mfud7xwz5efqugzrp6g23yl05v89vgp" }
  );
  console.log(x);
  // secret1wp68sr79x0th8zq0fjvd2exj3923l4pr6jamly acc
  // mock_rewards secret1ueu3397tk6nx3q3t4yddw2mcw4el5qwrg5pppe
  // vault secret1flkurty77acf4pa93ur7em9mlhydgn0qg69a9r
  // const staking_info_1 = await staking_contract.query.info();
  // console.log(JSON.stringify(staking_info_1, null, 2));

  // //await staking_contract.tx.claim_and_stake({account: contract_owner});
  // const info = await staking_contract.tx.advance_window({account: contract_owner});
  // console.log(info);

  // const user_claimable = await staking_contract.query.user_claimable({address: "secret1tvmxk7z3udvv5md7a9t6jmtngjc0jw9a5309ph"});
  // console.log(JSON.stringify(user_claimable, null, 2));
  // const ex_response = await contract.tx.increment(contract_owner);
  // console.log(ex_response);

  // const response = await contract.query.get_count();
  // console.log(response);
}

module.exports = { default: run };
