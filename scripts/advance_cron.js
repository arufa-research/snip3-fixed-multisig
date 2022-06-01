const { Contract, getAccountByName } = require("secret-polar");

function sleep(seconds) {
    console.log("Sleeping for " + seconds + " seconds");
    return new Promise(resolve => setTimeout(resolve, seconds * 1000));
}

async function run() {
    const runTs = String(new Date());
    const contract_owner = getAccountByName("deployer");

    const staking_contract = new Contract('staking-contract');
    await staking_contract.parseSchema();

    var count = 0;
    while (true) {
        try {
            const customFees = { // custom fees
                amount: [{ amount: "50000", denom: "uscrt" }],
                gas: "200000",
            }
            const advance_res = await staking_contract.tx.advance_window(
                { account: contract_owner, customFees: customFees }
            );
            // console.log(JSON.stringify(claim_and_stake_res, null, 2));
        } catch (e) {
            console.log(e);
            console.log("Advance window failing, skipping");
        }

        await sleep(60 * 60 * 24 * 3); // 3 days
        count += 1;
    }
}

module.exports = { default: run };
