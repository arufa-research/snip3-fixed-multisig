// TODO : Check on Exchange rate
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
        contract_owner = getAccountByName("account_2");
        other_account = getAccountByName("account_1");
        other_account_1 = getAccountByName("admin");

        staking_token = new Contract('staking-token');
        // mock_validator_contract = new Contract('counter');
        staking_contract = new Contract('staking-contract');

        await staking_token.parseSchema();
        // await mock_validator_contract.parseSchema();
        await staking_contract.parseSchema();
        await staking_token.deploy(
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

    });
    const delay = ms => new Promise(res => setTimeout(res, ms));

    async function getViewingKey(account) {
        const other_viewing_key_data = await staking_token.tx.create_viewing_key(
            { account: account },
            { entropy: `${runTs}` }
        );
        return JSON.parse(fromUtf8(other_viewing_key_data.data)).create_viewing_key.key;
    }

    it("Claim should be 0 with 0 seSCRT unstaking", async () => {
        const staking_info_1 = await staking_contract.query.info();
        staking_token.instantiatedWithAddress(staking_info_1.info.token_address);
        await staking_contract.tx.add_to_whitelist(
            { account: contract_owner },
            { "address": other_account.account.address });
        await delay(240000);
        await staking_contract.tx.advance_window(
            { account: contract_owner }
        );
        await delay(1000000);
        await staking_contract.tx.advance_window(
            { account: contract_owner }
        );
        await expect(staking_contract.query.user_claimable(
            { "address": other_account.account.address }
        )).to.respondWith({ claimable: { claimable_amount: '0' } });
        await staking_contract.tx.claim(
            { account: other_account },
            {secret: false}
        );
        const staking_info_3 = await staking_contract.query.info();
        assert.equal(staking_info_3.info.admin, contract_owner.account.address);
        assert.equal(staking_info_3.info.scrt_in_contract, '0');
        assert.equal(staking_info_3.info.sescrt_in_contract, '0');
        assert.equal(staking_info_3.info.scrt_under_withdraw, '0');
    });

    it("Should be able to Stake during unbonding period", async () => {
        const staking_info_1 = await staking_contract.query.info();
        staking_token.instantiatedWithAddress(staking_info_1.info.token_address);
        const transferAmount_2 = [{ "denom": "uscrt", "amount": "4000000" }];
        await staking_contract.tx.add_to_whitelist(
            { account: contract_owner },
            { "address": other_account.account.address });
        await staking_contract.tx.stake(
            { account: other_account, transferAmount: transferAmount_2 }
        );
        await staking_contract.tx.claim_and_stake(
            { account: contract_owner }
        );
        const xc = await staking_contract.query.info();
        console.log("Validator: ", xc.info.validators);
        console.log("Moved to validator!!");
        const balance = await staking_token.query.balance({
            "address": other_account.account.address,
            "key": await getViewingKey(other_account),
        });
        const transferAmount_sescrt = balance.balance.amount;
        await staking_token.tx.send(
            { account: other_account },
            { amount: transferAmount_sescrt, recipient: staking_contract.contractAddress }
        );
        await expect(staking_token.query.balance({
            "address": other_account.account.address,
            "key": await getViewingKey(other_account),
        })).to.respondWith({ 'balance': { 'amount': '0' } });

        await delay(240000);
        await staking_contract.tx.advance_window(
            { account: contract_owner }
        );
        await delay(100000);
        await staking_contract.tx.stake(
            { account: other_account, transferAmount: transferAmount_2 }
        );
        await expect(staking_token.query.balance({
            "address": other_account.account.address,
            "key": await getViewingKey(other_account),
        })).to.respondWith({ 'balance': { 'amount': '4000000' } });
        await delay(1000000);
        await staking_contract.tx.advance_window(
            { account: contract_owner }
        );
        const claimable = await staking_contract.query.user_claimable(
            { "address": other_account.account.address }
        );
        const claimable_amount = claimable.claimable.claimable_amount;
        assert.isAtLeast(parseInt(claimable_amount), 4000000);

        await staking_contract.tx.claim(
            { account: other_account },
            {secret: false}
        );
        const staking_info_3 = await staking_contract.query.info();
        assert.equal(staking_info_3.info.admin, contract_owner.account.address);
        assert.equal(staking_info_3.info.scrt_in_contract, '4000000');
        assert.equal(staking_info_3.info.sescrt_in_contract, '0');
        assert.equal(staking_info_3.info.scrt_under_withdraw, '0');
    });

    it("Claim without advance_window Call", async () => {
        const staking_info_1 = await staking_contract.query.info();
        staking_token.instantiatedWithAddress(staking_info_1.info.token_address);
        const transferAmount_2 = [{ "denom": "uscrt", "amount": "4000000" }];
        await staking_contract.tx.add_to_whitelist(
            { account: contract_owner },
            { "address": other_account.account.address });
        await staking_contract.tx.stake(
            { account: other_account, transferAmount: transferAmount_2 }
        );
        await staking_contract.tx.claim_and_stake(
            { account: contract_owner }
        );
        const balance = await staking_token.query.balance({
            "address": other_account.account.address,
            "key": await getViewingKey(other_account),
        });
        const transferAmount_sescrt = balance.balance.amount;
        await staking_token.tx.send(
            { account: other_account },
            { amount: transferAmount_sescrt, recipient: staking_contract.contractAddress }
        );
        const claimable = await staking_contract.query.user_claimable(
            { "address": other_account.account.address }
        );
        const claimable_amount = claimable.claimable.claimable_amount;
        assert.equal(parseInt(claimable_amount), 0);
        const claim_res = await staking_contract.tx.claim(
            { account: other_account },
            {secret: false}
        );
    });

    it("Without depositing making request for unstaking", async () => {
        const staking_info_1 = await staking_contract.query.info();
        staking_token.instantiatedWithAddress(staking_info_1.info.token_address);
        const transferAmount_sescrt = "4000000";
        await expect(staking_token.tx.send(
            { account: other_account },
            { amount: transferAmount_sescrt, recipient: staking_contract.contractAddress }
        )).to.be.revertedWith('insufficient funds');
    });

    it("Check Withdrawal with a Happy flow", async () => {
        const staking_info_1 = await staking_contract.query.info();
        staking_token.instantiatedWithAddress(staking_info_1.info.token_address);
        const transferAmount_2 = [{ "denom": "uscrt", "amount": "4000000" }];
        await staking_contract.tx.add_to_whitelist({ account: contract_owner }, { "address": other_account.account.address });
        await staking_contract.tx.stake(
            { account: other_account, transferAmount: transferAmount_2 }
        );
        console.log("First Amount Staked!!!");
        await staking_contract.tx.claim_and_stake(
            { account: contract_owner }
        );
        const xc = await staking_contract.query.info();
        console.log("Validator: ", xc.info.validators);
        console.log("Moved to validator!!");
        const exchange_rate_1 = await staking_contract.query.exchange_rate();
        let rate_1 = parseFloat(exchange_rate_1.exchange_rate.rate);
        console.log("exchange_rate_1", rate_1);
        const balance = await staking_token.query.balance({
            "address": other_account.account.address,
            "key": await getViewingKey(other_account),
        });
        const transferAmount_sescrt = balance.balance.amount;
        await staking_token.tx.send(
            { account: other_account },
            { amount: transferAmount_sescrt, recipient: staking_contract.contractAddress }
        );
        console.log("Withdraw request submitted!!");
        await expect(staking_token.query.balance({
            "address": other_account.account.address,
            "key": await getViewingKey(other_account),
        })).to.respondWith({ 'balance': { 'amount': '0' } });

        const staking_info_2 = await staking_contract.query.info();
        console.log("staking_info_2 ", staking_info_2);
        console.log("Validator: ", staking_info_2.info.validators);
        await delay(240000);
        await staking_contract.tx.advance_window(
            { account: contract_owner }
        );
        const staking_in = await staking_contract.query.info();
        console.log("staking_in ", staking_in);
        console.log("Validator: ", staking_in.info.validators);
        await expect(staking_contract.query.user_claimable(
            { "address": other_account.account.address })).to.respondWith({ claimable: { claimable_amount: '0' } });

        const xf = await staking_contract.query.undelegations({
            "address": other_account.account.address
        });
        console.log("Pending:  ", xf.pending_claims.pending);
        await delay(1000000);
        await staking_contract.tx.advance_window(
            { account: contract_owner }
        );
        const claimable = await staking_contract.query.user_claimable(
            { "address": other_account.account.address }
        );
        const claimable_amount = claimable.claimable.claimable_amount;
        assert.isAtLeast(parseInt(claimable_amount), 4000000);

        await staking_contract.tx.claim(
            { account: other_account },
            {secret: false}
        );
        const staking_info_3 = await staking_contract.query.info();
        assert.equal(staking_info_3.info.admin, contract_owner.account.address);
        assert.equal(staking_info_3.info.scrt_in_contract, '0');
        assert.equal(staking_info_3.info.sescrt_in_contract, '0');
        assert.equal(staking_info_3.info.scrt_under_withdraw, '0');
    });

    it("Double deposit and claim_stake not called after second deposit", async () => {
        const staking_info_1 = await staking_contract.query.info();
        staking_token.instantiatedWithAddress(staking_info_1.info.token_address);
        const transferAmount_2 = [{ "denom": "uscrt", "amount": "4000000" }];
        await staking_contract.tx.add_to_whitelist({ account: contract_owner }, { "address": other_account.account.address });
        await staking_contract.tx.stake(
            { account: other_account, transferAmount: transferAmount_2 }
        );
        console.log("First Amount Staked!!!");
        await staking_contract.tx.claim_and_stake(
            { account: contract_owner }
        );
        const xc = await staking_contract.query.info();
        console.log("Validator: ", xc.info.validators);
        console.log("Moved to validator!!");
        const exchange_rate_1 = await staking_contract.query.exchange_rate();
        let rate_1 = parseFloat(exchange_rate_1.exchange_rate.rate);
        console.log("exchange_rate_1", rate_1 );
        const transferAmount_3 = [{ "denom": "uscrt", "amount": "2000000" }];
        await staking_contract.tx.stake(
            { account: other_account, transferAmount: transferAmount_3 }
        );
        console.log("Second Amount Staked!!!");
        const exchange_rate_2 = await staking_contract.query.exchange_rate();
        let rate_2 = parseFloat(exchange_rate_2.exchange_rate.rate);
        console.log("exchange_rate_1", rate_2 );
        const balance_after_two_deposit = await staking_token.query.balance({
            "address": other_account.account.address,
            "key": await getViewingKey(other_account),
        });
        const transferAmount_sescrt = balance_after_two_deposit.balance.amount;
        const exchange_rate_3 = await staking_contract.query.exchange_rate();
        let rate_3 = parseFloat(exchange_rate_3.exchange_rate.rate);
        console.log("exchange_rate_3", rate_3 );

        await staking_token.tx.send(
            { account: other_account },
            { amount: transferAmount_sescrt, recipient: staking_contract.contractAddress }
        );;
        console.log("Withdraw request submitted!!");

        const query_window_1 = await staking_contract.query.window();
        console.log("query window 1 => ", query_window_1);

        await expect(staking_token.query.balance({
            "address": other_account.account.address,
            "key": await getViewingKey(other_account),
        })).to.respondWith({ 'balance': { 'amount': '0' } });

        const staking_info_2 = await staking_contract.query.info();
        console.log("Validator: ", staking_info_2.info.validators);
        assert.equal(staking_info_2.info.admin, contract_owner.account.address);
        assert.isAtLeast(parseInt(staking_info_2.info.total_staked), 6000000);
        assert.equal(staking_info_2.info.scrt_in_contract, '0');
        assert.equal(staking_info_2.info.sescrt_in_contract, transferAmount_sescrt);
        assert.equal(staking_info_2.info.scrt_under_withdraw, '0');

        await delay(240000);
        await staking_contract.tx.advance_window(
            { account: contract_owner }
        );
        const staking_in = await staking_contract.query.info();
        console.log("Validator: ", staking_in.info.validators);
        await expect(staking_contract.query.user_claimable(
            { "address": other_account.account.address })).to.respondWith({ claimable: { claimable_amount: '0' } });

        const xf = await staking_contract.query.undelegations({
            "address": other_account.account.address
        });
        console.log("Pending:  ", xf.pending_claims.pending);
        await delay(1000000);
        await staking_contract.tx.advance_window(
            { account: contract_owner }
        );
        const claimable = await staking_contract.query.user_claimable(
            { "address": other_account.account.address }
        );
        const claimable_amount = claimable.claimable.claimable_amount;
        assert.isAtLeast(parseInt(claimable_amount), 6000000);

        await staking_contract.tx.claim(
            { account: other_account },
            {secret: false}
        );
        const staking_info_3 = await staking_contract.query.info();
        assert.equal(staking_info_3.info.admin, contract_owner.account.address);
        assert.equal(staking_info_3.info.scrt_in_contract, '0');
        assert.equal(staking_info_3.info.sescrt_in_contract, '0');
        assert.equal(staking_info_3.info.scrt_under_withdraw, '0');
    });

    it("Sending more seSCRT than in balance", async () => {
        const staking_info_1 = await staking_contract.query.info();
        staking_token.instantiatedWithAddress(staking_info_1.info.token_address);
        await staking_contract.tx.add_to_whitelist(
            { account: contract_owner },
            { "address": other_account.account.address });
        const transferAmount_2 = [{ "denom": "uscrt", "amount": "4000000" }];
        await staking_contract.tx.stake(
            { account: other_account, transferAmount: transferAmount_2 }
        );
        await staking_contract.tx.claim_and_stake(
            { account: contract_owner }
        );
        const transferAmount_sescrt = "20000000"
        await expect(staking_token.tx.send(
            { account: other_account },
            { amount: transferAmount_sescrt, recipient: staking_contract.contractAddress }
        )).to.be.revertedWith('insufficient funds');
    });

    it("unstaking less seSCRT than limit", async () => {
        const staking_info_1 = await staking_contract.query.info();
        staking_token.instantiatedWithAddress(staking_info_1.info.token_address);
        await staking_contract.tx.add_to_whitelist(
            { account: contract_owner },
            { "address": other_account.account.address });
        const transferAmount_2 = [{ "denom": "uscrt", "amount": "4000000" }];
        await staking_contract.tx.stake(
            { account: other_account, transferAmount: transferAmount_2 }
        );
        await staking_contract.tx.claim_and_stake(
            { account: contract_owner }
        );
        const transferAmount_sescrt = "8000"
        await expect(staking_token.tx.send(
            { account: other_account },
            { amount: transferAmount_sescrt, recipient: staking_contract.contractAddress }
        )).to.be.revertedWith('Amount withdrawn below minimum of 10000 usescrt');
    });

    it("Two User partial withdraw-Test", async () => {
        const staking_info_1 = await staking_contract.query.info();
        staking_token.instantiatedWithAddress(staking_info_1.info.token_address);
        const transferAmount_1 = [{ "denom": "uscrt", "amount": "4000000" }];
        await staking_contract.tx.add_to_whitelist(
            { account: contract_owner },
            { "address": other_account.account.address });
        await staking_contract.tx.add_to_whitelist(
            { account: contract_owner },
            { "address": other_account_1.account.address });
        await staking_contract.tx.stake(
            { account: other_account, transferAmount: transferAmount_1 }
        );
        await staking_contract.tx.claim_and_stake(
            { account: contract_owner }
        );
        const transferAmount_2 = [{ "denom": "uscrt", "amount": "2000000" }];
        // Second User deposit
        await staking_contract.tx.stake(
            { account: other_account_1, transferAmount: transferAmount_2 }
        );
        await staking_contract.tx.claim_and_stake(
            { account: contract_owner }
        );
        await staking_token.tx.send(
            { account: other_account },
            { amount: "2000000", recipient: staking_contract.contractAddress }
        );
        await delay(240000);
        const exchange_rate_1 = await staking_contract.query.exchange_rate();
        let rate_1 = parseFloat(exchange_rate_1.exchange_rate.rate);
        console.log("Before advance window exchange_rate_1", rate_1 );
        const user_1_scrt_amount = Math.floor(rate_1 * 2000000);
        await staking_contract.tx.claim_and_stake(
            { account: contract_owner }
        );
        await staking_contract.tx.advance_window(
            { account: contract_owner }
        );
        await delay(1000000);
        await staking_contract.tx.advance_window(
            { account: contract_owner }
        );
        console.log("user_1_scrt_amount should be => ", user_1_scrt_amount);
        const user_1_claimable = await staking_contract.query.user_claimable(
            { "address": other_account.account.address }
        );
        await staking_contract.tx.claim(
            { account: other_account },
            {secret: false}
        )
        const user_1_claimable_amount = user_1_claimable.claimable.claimable_amount;
        console.log("(user_1_claimable_amount - user_1_scrt_amount) => ", (user_1_claimable_amount - user_1_scrt_amount));
        assert.isAtMost((user_1_claimable_amount - user_1_scrt_amount), 600);
        // User_2 unstake his amount
        await staking_token.tx.send(
            { account: other_account_1 },
            { amount: "1999900", recipient: staking_contract.contractAddress }
        );
        await delay(240000);
        const exchange_rate_2 = await staking_contract.query.exchange_rate();
        let rate_2 = parseFloat(exchange_rate_2.exchange_rate.rate);
        console.log("x_rate_2 after u1 withdrawl before u2 advance call", rate_2 );
        const user_2_scrt_amount = Math.floor(rate_2 * 1000000);
        assert.isAtLeast(rate_1,rate_2);
        await staking_contract.tx.claim_and_stake(
            { account: contract_owner }
        );
        await staking_contract.tx.advance_window(
            { account: contract_owner }
        );
        await delay(1000000);
        await staking_contract.tx.advance_window(
            { account: contract_owner }
        );
        console.log("user_2_scrt_amount should be=> ", user_2_scrt_amount);
        const user_2_claimable = await staking_contract.query.user_claimable(
            { "address": other_account_1.account.address }
        );
        const user_2_claimable_amount = user_2_claimable.claimable.claimable_amount;
        console.log("(user_2_claimable_amount - user_2_scrt_amount) => ", (user_2_claimable_amount - user_2_scrt_amount));
        await staking_contract.tx.claim(
            { account: other_account_1 },
            {secret: false}
        )
        assert.isAtMost((user_2_claimable_amount - user_2_scrt_amount), 600);
        // xrate after both partial withdraw
        const XR_BPW = await staking_contract.query.exchange_rate();
        let XR_after_both_partial_withdraw = parseFloat(XR_BPW.exchange_rate.rate);
        console.log("XR after both partial withdraw", XR_after_both_partial_withdraw );
        assert.isAtLeast(rate_2 , XR_after_both_partial_withdraw);
    });

    it("Two User FULL withdraw-Test", async () => {
        const staking_info_1 = await staking_contract.query.info();
        staking_token.instantiatedWithAddress(staking_info_1.info.token_address);
        const transferAmount_1 = [{ "denom": "uscrt", "amount": "4000000" }];
        await staking_contract.tx.add_to_whitelist(
            { account: contract_owner },
            { "address": other_account.account.address });
        await staking_contract.tx.add_to_whitelist(
            { account: contract_owner },
            { "address": other_account_1.account.address });
        await staking_contract.tx.stake(
            { account: other_account, transferAmount: transferAmount_1 }
        );
        await staking_contract.tx.claim_and_stake(
            { account: contract_owner }
        );
        const staking_info_2 = await staking_contract.query.info();
        console.log("Info after first deposit => ", staking_info_2);
        const transferAmount_2 = [{ "denom": "uscrt", "amount": "2000000" }];
        // Second User deposit
        await staking_contract.tx.stake(
            { account: other_account_1, transferAmount: transferAmount_2 }
        );
        await staking_contract.tx.claim_and_stake(
            { account: contract_owner }
        );
        const staking_info_3 = await staking_contract.query.info();
        console.log("Info after second deposit => ", staking_info_3);

        await staking_token.tx.send(
            { account: other_account },
            { amount: "4000000", recipient: staking_contract.contractAddress }
        );
        await delay(240000);
        const staking_info_4 = await staking_contract.query.info();
        console.log("Info after first receive => ", staking_info_4);
        const exchange_rate_1 = await staking_contract.query.exchange_rate();
        let rate_1 = parseFloat(exchange_rate_1.exchange_rate.rate);
        console.log("Before advance window exchange_rate_1", rate_1 );
        const user_1_scrt_amount = Math.floor(rate_1 * 4000000);
        await staking_contract.tx.claim_and_stake(
            { account: contract_owner }
        );
        const staking_info_5 = await staking_contract.query.info();
        console.log("Info before firstU advance call => ", staking_info_5);
        await staking_contract.tx.advance_window(
            { account: contract_owner }
        );
        const deleg = await staking_contract.query.undelegations({
            "address": other_account.account.address
        });
        console.log(deleg.pending_claims.pending);
        await delay(1200000);
        const staking_info_6 = await staking_contract.query.info();
        console.log("Info before firstU Second advance call => ", staking_info_6);
        await staking_contract.tx.advance_window(
            { account: contract_owner }
        );
        console.log("user_1_scrt_amount should be => ", user_1_scrt_amount);
        const user_1_claimable = await staking_contract.query.user_claimable(
            { "address": other_account.account.address }
        );
        console.log(user_1_claimable);
        const user_1_claimable_amount = user_1_claimable.claimable.claimable_amount;
        console.log("(user_1_claimable_amount - user_1_scrt_amount) => ", (user_1_claimable_amount - user_1_scrt_amount));
        assert.isAtLeast(user_1_claimable_amount,4000000);
        assert.isAtMost((user_1_claimable_amount - user_1_scrt_amount), 600);
        await staking_contract.tx.claim(
            { account: other_account },
            {secret: false}
        )
        // User_2 unstake his amount
        const staking_info_7 = await staking_contract.query.info();
        console.log("Info before secondU receive call => ", staking_info_7);
        await staking_token.tx.send(
            { account: other_account_1 },
            { amount: "1999900", recipient: staking_contract.contractAddress }
        );
        await delay(240000);
        const exchange_rate_2 = await staking_contract.query.exchange_rate();
        let rate_2 = parseFloat(exchange_rate_2.exchange_rate.rate);
        console.log("x_rate_2 after u1 withdrawl before u2 advance call", rate_2 );
        const user_2_scrt_amount = Math.floor(rate_2 * 2000000);
        assert.isAtLeast(rate_1,rate_2);
        await staking_contract.tx.claim_and_stake(
            { account: contract_owner }
        );
        const staking_info_8 = await staking_contract.query.info();
        console.log("Info before secondU advance call => ", staking_info_8);
        await staking_contract.tx.advance_window(
            { account: contract_owner }
        );
        const dele_0 = await staking_contract.query.undelegations({
            "address": other_account.account.address
        });
        console.log(dele_0.pending_claims.pending);
        await delay(1200000);
        const staking_info_9 = await staking_contract.query.info();
        console.log("Info before secondU Second advance call => ", staking_info_9);
        await staking_contract.tx.advance_window(
            { account: contract_owner }
        );
        console.log("user_2_scrt_amount should be=> ", user_2_scrt_amount);
        const user_2_claimable = await staking_contract.query.user_claimable(
            { "address": other_account_1.account.address }
        );
        console.log(user_2_claimable);
        const user_2_claimable_amount = user_2_claimable.claimable.claimable_amount;
        console.log("(user_2_claimable_amount - user_2_scrt_amount) => ", (user_2_claimable_amount - user_2_scrt_amount));
        await staking_contract.tx.claim(
            { account: other_account_1 },
            {secret: false}
        )
        assert.isAtLeast(user_2_claimable_amount,2000000);
        assert.isAtMost((user_2_claimable_amount - user_2_scrt_amount), 600);
        // xrate after both partial withdraw
        const XR_BPW = await staking_contract.query.exchange_rate();
        let XR_after_both_partial_withdraw = parseFloat(XR_BPW.exchange_rate.rate);
        console.log("XR after both partial withdraw", XR_after_both_partial_withdraw );
        assert.isAtLeast(rate_2 , XR_after_both_partial_withdraw);
        const total_claimable = user_1_claimable_amount + user_2_claimable_amount;
        const total_deposited = 6000000;
        assert.isAtLeast(total_claimable,total_deposited);
        const staking_info_10 = await staking_contract.query.info();
        console.log("Info after both withdraw => ", staking_info_10);
    });

    it("Advance Should be called only by Admin", async () => {
        const staking_info_1 = await staking_contract.query.info();
        staking_token.instantiatedWithAddress(staking_info_1.info.token_address);
        await staking_contract.tx.add_to_whitelist(
            { account: contract_owner },
            { "address": other_account.account.address });
        const transferAmount_2 = [{ "denom": "uscrt", "amount": "4000000" }];
        await staking_contract.tx.stake(
            { account: other_account, transferAmount: transferAmount_2 }
        );
        await staking_contract.tx.claim_and_stake(
            { account: contract_owner }
        );
        const transferAmount_sescrt = "2000000"
        await staking_token.tx.send(
            { account: other_account },
            { amount: transferAmount_sescrt, recipient: staking_contract.contractAddress }
        );
        await delay(240000);
        await expect(staking_contract.tx.advance_window(
            { account: other_account }
        )).to.be.revertedWith('Only admin can call advance window');
    });
});