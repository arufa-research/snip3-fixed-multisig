const { expect, use, assert } = require("chai");
const { fromUtf8 } = require("@iov/encoding");
const { Contract, getAccountByName, polarChai } = require("secret-polar");

use(polarChai);

describe("Withdraw-claim", () => {
    let contract_owner, other_account, other_account_1;
    let runTs;
    let staking_token, staking_contract;
    before(async () => {
        runTs = String(new Date());
        contract_owner = getAccountByName("admin");
        other_account = getAccountByName("account_1");
        other_account_1 = getAccountByName("account_2");

        staking_token = new Contract('staking-token');
        // mock_validator_contract = new Contract('counter');
        staking_contract = new Contract('staking-contract');
        sscrt_contract = new Contract('secret-secret');

        await staking_token.parseSchema();
        // await mock_validator_contract.parseSchema();
        await staking_contract.parseSchema();
        await sscrt_contract.parseSchema();
        await staking_token.deploy(
            contract_owner,
            {
                amount: [{ amount: "1000000", denom: "uscrt" }],
                gas: "4000000",
            }
        );
        await sscrt_contract.deploy(
            contract_owner,
            {
                amount: [{ amount: "1000000", denom: "uscrt" }],
                gas: "4000000",
            }
        );
        // await mock_validator_contract.deploy(
        //     contract_owner,
        //     {
        //         amount: [{ amount: "1000000", denom: "uscrt" }],
        //         gas: "4000000",
        //     }
        // );
        await staking_contract.deploy(
            contract_owner,
            {
                amount: [{ amount: "1000000", denom: "uscrt" }],
                gas: "4000000",
            }
        );
        // await mock_validator_contract.instantiate({"count": 102}, `validator list 2 ${runTs}`, contract_owner);
        // let v1 =[];
        // v1.push("secretvaloper1rxnt4f04rqtz43mezgajws4ffc2f94fkg5lnq2");
        // await mock_validator_contract.tx.update_list(
        // {account: contract_owner},
        // {list: v1}
        // );
        await sscrt_contract.instantiate(
            {
                "decimals": 6,
                "name": "SampleSnip",
                "prng_seed": "YWE",
                "symbol": "SMPL",
                "config": { "enable_mint": true }
            },
            `SSRT ${runTs}`,
            contract_owner
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
                "sscrt_token_contract_hash": sscrt_contract.contractCodeHash, //"9587d60b8e6b078ace12014ceeee089530b9fabcd76535d93666a6c127ad8813",
                "sscrt_address": sscrt_contract.contractAddress, //"secret18vd8fpwxzck93qlwghaj6arh4p7c5n8978vsyg"
            },
            `SE staking contract ${runTs}`,
            contract_owner
        );
    });
    const delay = ms => new Promise(res => setTimeout(res, ms));

    async function getViewingKey(account) {
        const other_viewing_key_data = await staking_token.tx.create_viewing_key(
            { account: account },
            { entropy: `${runTs}` }
        );
        return JSON.parse(fromUtf8(other_viewing_key_data.data)).create_viewing_key.key;
    }
    async function getViewingKey1(account) {
        const other_viewing_key_data = await sscrt_contract.tx.create_viewing_key(
            { account: account },
            { entropy: ` qwaq ${runTs}` }
        );
        return JSON.parse(fromUtf8(other_viewing_key_data.data)).create_viewing_key.key;
    }
    it("Claim sscrt and deposit sscrt", async () => {
        const staking_info_1 = await staking_contract.query.info();
        staking_token.instantiatedWithAddress(staking_info_1.info.token_address);
        const transferAmount_scrt = [{ "denom": "uscrt", "amount": "4000000" }];
        // await staking_contract.tx.add_to_whitelist({ account: contract_owner }, { "address": other_account.account.address });
        await staking_contract.tx.stake(
            { account: other_account, transferAmount: transferAmount_scrt }
        );
        console.log("First Amount Staked!!!");
        await staking_contract.tx.claim_and_stake(
            { account: contract_owner }
        );
        const balance = await staking_token.query.balance({
            "address": other_account.account.address,
            "key": await getViewingKey(other_account),
        });
        console.log("sescrt balance => ", balance);
        const transferAmount_sescrt = balance.balance.amount;
        await staking_token.tx.send(
            { account: other_account },
            { amount: transferAmount_sescrt, recipient: staking_contract.contractAddress }
        );
        await delay(240000);
        await staking_contract.tx.advance_window(
            { account: contract_owner }
        );
        await delay(1000000);
        await staking_contract.tx.advance_window(
            { account: contract_owner }
        );
        const claimable = await staking_contract.query.user_claimable(
            { "address": other_account.account.address }
        );
        console.log("claimable => ", claimable);
        const claimable_amount = claimable.claimable.claimable_amount;
        console.log("claimable_amount => ", claimable_amount);
        const staking_info_4 = await staking_contract.query.info();
        console.log("staking_info_4 => ", staking_info_4);

        const balance_0 = await sscrt_contract.query.balance({
            "address": other_account.account.address,
            "key": await getViewingKey1(other_account),
        });
        assert.equal(balance_0.balance.amount,0);

        await staking_contract.tx.claim(
            { account: other_account },
            { "secret": true }
        );
        const staking_info_2 = await staking_contract.query.info();
        console.log("staking_info_2 => ", staking_info_2);

        const balance_1 = await staking_token.query.balance({
            "address": other_account.account.address,
            "key": await getViewingKey(other_account),
        });
        console.log("balance_1 => ", balance_1);
        assert.equal(balance_1.balance.amount,0);

        const sscrt_bal_1 = await sscrt_contract.query.balance({
            "address": other_account.account.address,
            "key": await getViewingKey1(other_account),
        });
        console.log("sscrt_bal @ 2 => ", sscrt_bal_1);
        assert.equal(sscrt_bal_1.balance.amount,claimable_amount);

        await sscrt_contract.tx.send(
            { account: other_account },
            { amount: claimable_amount, recipient: staking_contract.contractAddress }
        );
        // const transferAmount_sscrt = [{ "denom": "usscrt", "amount": claimable_amount }];
        // await staking_contract.tx.stake(
        //     { account: other_account, transferAmount: transferAmount_sscrt }
        // );

        await staking_contract.tx.claim_and_stake(
            { account: contract_owner }
        );
        const staking_info_3 = await staking_contract.query.info();
        console.log("staking_info_3 => ", staking_info_3);
    });

});