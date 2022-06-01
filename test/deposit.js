//Todo add check for secret balance

const { expect, use, assert } = require("chai");
const { fromUtf8 } = require("@iov/encoding");
const { Contract, getAccountByName, polarChai } = require("secret-polar");

use(polarChai);

describe("Deposit Flow", () => {
  let contract_owner, other_account;
  let runTs;
  let staking_token, staking_contract;
  before(async () => {
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
  });

  const delay = ms => new Promise(res => setTimeout(res, ms));    //Utility function for delaying

  async function getViewingKey(account) {
    const other_viewing_key_data = await staking_token.tx.create_viewing_key(
      { account: account },
      { entropy: `${runTs}` }
    );
    return JSON.parse(fromUtf8(other_viewing_key_data.data)).create_viewing_key.key;
  }

  it("Should initialize contract with correct parameters", async () => {
    const staking_info_1 = await staking_contract.query.info();
    assert.equal(staking_info_1.info.admin, contract_owner.account.address);
    assert.equal(staking_info_1.info.total_staked, '0');
    assert.equal(staking_info_1.info.scrt_in_contract, '0');
    assert.equal(staking_info_1.info.sescrt_in_contract, '0');
    assert.equal(staking_info_1.info.scrt_under_withdraw, '0');
  });

  it("Deposit correct amount,send seSCRT to user, scrt remains in contract, claim_stake send to validators", async () => {
    const transferAmount_2 = [{ "denom": "uscrt", "amount": "4000000" }];
    const staking_info = await staking_contract.query.info();
    staking_token.instantiatedWithAddress(staking_info.info.token_address);
    await staking_contract.tx.add_to_whitelist({ account: contract_owner }, { "address": other_account.account.address });
    await delay(125000);
    await staking_contract.tx.stake(
      { account: other_account, transferAmount: transferAmount_2 }
    );
    const staking_info_1 = await staking_contract.query.info();
    assert.equal(staking_info_1.info.total_staked, '4000000');
    assert.equal(staking_info_1.info.scrt_in_contract, '4000000');
    assert.equal(staking_info_1.info.sescrt_in_contract, '0');
    assert.equal(staking_info_1.info.scrt_under_withdraw, '0');

    await expect(staking_token.query.balance({
      "address": other_account.account.address,
      "key": await getViewingKey(other_account),
    })).to.respondWith({ 'balance': { 'amount': '4000000' } });
    await delay(240000);
    await staking_contract.tx.claim_and_stake(
      { account: contract_owner }
    );
    const staking_info_2 = await staking_contract.query.info();
    assert.equal(staking_info_2.info.total_staked, '4000000');
    assert.equal(staking_info_2.info.scrt_in_contract, '0');
    assert.equal(staking_info_2.info.sescrt_in_contract, '0');
    assert.equal(staking_info_2.info.scrt_under_withdraw, '0');
  });

  it("Case for varying exchange rate", async () => {

    const ex_rate_1 = await staking_contract.query.exchange_rate();
    let rate_1 = parseFloat(ex_rate_1.exchange_rate.rate);
    rate_1 = rate_1 * 1000000;
    await delay(600000);
    await staking_contract.tx.claim_and_stake(
      { account: contract_owner }
    );
    const ex_rate_2 = await staking_contract.query.exchange_rate();
    let rate_2 = parseFloat(ex_rate_2.exchange_rate.rate);
    rate_2 = rate_2 * 1000000;

    assert.isAbove(rate_2, rate_1);
  });

  it("Should not deposit less than 1 scrt", async () => {
    const transferAmount_2 = [{ "denom": "uscrt", "amount": "200000" }];
    await staking_contract.tx.add_to_whitelist({ account: contract_owner }, { "address": other_account.account.address });
    await delay(125000);
    await expect(staking_contract.tx.stake({
      account: other_account,
      transferAmount: transferAmount_2
    })).to.be.revertedWith('Can only deposit a minimum of 1,000,000 uscrt (1 SCRT)');
  });

  it("Total staked must vary with initial total staked, User getting seSCRT according to x_rate", async () => {
    const transferAmount_2 = [{ "denom": "uscrt", "amount": "4000000" }];
    await delay(121000);
    const ex_rate = await staking_contract.query.exchange_rate();
    let rate = parseFloat(ex_rate.exchange_rate.rate);
    await staking_contract.tx.add_to_whitelist({ account: contract_owner }, { "address": other_account.account.address });
    await delay(125000);
    await staking_contract.tx.stake(
      { account: other_account, transferAmount: transferAmount_2 }
    );
    const exp_bal = Math.floor(4000000 / rate) + 4000000;
    const bal = await staking_token.query.balance(
      {
        "address": other_account.account.address,
        "key": await getViewingKey(other_account)
      }
    );
    let amount_bal = parseInt(bal.balance.amount);
    assert.equal(amount_bal, exp_bal);

    const staking_info_2 = await staking_contract.query.info();
    assert.notEqual(staking_info_2.info.total_staked, '8000000');
    assert.equal(staking_info_2.info.scrt_in_contract, '4000000');
  });
});