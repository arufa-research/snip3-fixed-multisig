
const { fromUtf8 } = require("@iov/encoding");
const { Contract, getAccountByName, polarChai } = require("secret-polar");

let runTs;
const delay = ms => new Promise(res => setTimeout(res, ms));    //Utility function for delaying

async function getViewingKey(account, staking_token) {
    const other_viewing_key_data = await staking_token.tx.create_viewing_key(
        { account: account },
        { entropy: `${runTs}` }
    );
    return JSON.parse(fromUtf8(other_viewing_key_data.data)).create_viewing_key.key;
}

async function run() {
    let contract_owner, other_account;
    let staking_token, staking_contract;
    runTs = String(new Date());
    contract_owner = getAccountByName("admin");
    other_account = getAccountByName("account_1");

    staking_token = new Contract('staking-token');
    staking_contract = new Contract('staking-contract');

    await staking_token.parseSchema();
    await staking_contract.parseSchema();

    await staking_token.deploy(
        contract_owner,
        {
            amount: [{ amount: "1000000", denom: "uscrt" }],
            gas: "4000000",
        }
    );
    await staking_contract.deploy(
        contract_owner,
        {
            amount: [{ amount: "1000000", denom: "uscrt" }],
            gas: "4000000",
        }
    );
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
            // "dev_fee": 3000,   // 3%
            "sscrt_token_contract_hash": "9587d60b8e6b078ace12014ceeee089530b9fabcd76535d93666a6c127ad8813",
            "sscrt_address": "secret18vd8fpwxzck93qlwghaj6arh4p7c5n8978vsyg"
        },
        `SE staking contract ${runTs}`,
        contract_owner
    );

    const transferAmount_2 = [{ "denom": "uscrt", "amount": "4000000" }];
    const staking_info = await staking_contract.query.info();
    staking_token.instantiatedWithAddress(staking_info.info.token_address);
    await staking_contract.tx.add_to_whitelist({ account: contract_owner }, { "address": other_account.account.address });
    await staking_contract.tx.set_white({ account: contract_owner }, { "white": true, "track": false });
    await staking_contract.tx.stake(
        { account: other_account, transferAmount: transferAmount_2 }
    );
    await staking_contract.tx.claim_and_stake({ account: contract_owner });
    const ex_rate_1 = await staking_contract.query.exchange_rate();
    console.log("First", ex_rate_1);
    console.log(await staking_contract.query.info());
    await delay(125000);

    await staking_contract.tx.kill_switch_unbond({ account: contract_owner });
    const ex_rate_2 = await staking_contract.query.exchange_rate();
    console.log("Second", ex_rate_2);
    console.log(await staking_contract.query.info());

    // Try deposit
    try {
        await staking_contract.tx.stake(
            { account: other_account, transferAmount: transferAmount_2 }
        ); s
    } catch (e) {
        console.log(e);
    }

    // Try withdraw
    const balance = await staking_token.query.balance({
        "address": other_account.account.address,
        "key": await getViewingKey(other_account, staking_token),
    });
    const transferAmount_sescrt = balance.balance.amount;
    try {
        await staking_token.tx.send(
            { account: other_account },
            { amount: transferAmount_sescrt, recipient: staking_contract.contractAddress }
        );
    } catch (e) {
        console.log(e);
    }

    await delay(1000000);
    await staking_contract.tx.kill_switch_open_withdraws({ account: contract_owner });

    // deposit should fail
    try {
        await staking_contract.tx.stake(
            { account: other_account, transferAmount: transferAmount_2 }
        ); s
    } catch (e) {
        console.log(e);
    }

    // should be able to withdraw
    await staking_token.tx.send(
        { account: other_account },
        { amount: transferAmount_sescrt, recipient: staking_contract.contractAddress }
    );
}

module.exports = { default: run };