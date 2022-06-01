const { Contract, getAccountByName } = require("secret-polar");

async function run() {
  const contract_owner = getAccountByName("account_0");
  const user_a = getAccountByName("account_1");
  const user_b = getAccountByName("account_2");

  const runTs = String(new Date());

  const multisig_contract = new Contract('snip3-fixed-multisig');
  await multisig_contract.parseSchema();

  const multisig_deploy = await multisig_contract.deploy(contract_owner);
  console.log(multisig_deploy);

  const multisig_init_info = await contract.instantiate(
    {
      voters: [
        { addr: user_a.account.address, weight: 1 },
        { addr: user_b.account.address, weight: 1 },
        { addr: contract_owner.account.address, weight: 1 }
      ],
      threshold: { absolute_count: { weight: 2 } },
      max_voting_period: { height: 1000 }
    },
    `Multisig ${runTs}`,
    contract_owner
  );
  console.log(multisig_init_info);

  // await staking_contract.tx.add_to_whitelist({ account: contract_owner }, { "address": "secret16l280j0kxd95q7au09hx0ry7s69mjxaltu20qd" });
  // await staking_contract.tx.set_white({ account: contract_owner }, { "white": true, "track": false });
}

module.exports = { default: run };
