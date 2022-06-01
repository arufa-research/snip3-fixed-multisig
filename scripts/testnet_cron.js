const { Contract, getAccountByName } = require("secret-polar");

function sleep(seconds) {
  console.log("Sleeping for " + seconds + " seconds");
  return new Promise(resolve => setTimeout(resolve, seconds*1000));
}

async function run () {
  const runTs = String(new Date());
  const contract_owner = getAccountByName("admin");

  const staking_contract = new Contract('staking-contract');
  await staking_contract.parseSchema();

  var count = 0;
  while(true) {
    // compounding txn
    try {
      const claim_and_stake_res = await staking_contract.tx.claim_and_stake(
        {account: contract_owner}
      );
      // console.log(JSON.stringify(claim_and_stake_res, null, 2));
    } catch {
      console.log("ClaimAndStake failing, skipping");
    }

    // advance window txn
    try {
      const adv_window_res = await staking_contract.tx.advance_window(
        {account: contract_owner}
      );
      // console.log(JSON.stringify(adv_window_res, null, 2));
    } catch(e) {
      console.log(e);
      await sleep(10);
      try {
        const adv_window_res = await staking_contract.tx.advance_window(
          {account: contract_owner}
        );
        // console.log(JSON.stringify(adv_window_res, null, 2));
      } catch(e) {
        console.log(e);
        await sleep(10);
        try {
          const adv_window_res = await staking_contract.tx.advance_window(
            {account: contract_owner}
          );
          // console.log(JSON.stringify(adv_window_res, null, 2));
        } catch {
          console.log("Advance window failing, skipping");
        }
      }
    }

    await sleep(2*60);
    count += 1;
  }
}

module.exports = { default: run };
