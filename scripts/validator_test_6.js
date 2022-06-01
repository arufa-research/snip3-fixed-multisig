// 2 Initially and 3 finally 1 common in both (Working)
const { Contract, getAccountByName } = require("secret-polar");
const { fromUtf8 } = require("@iov/encoding");

async function run() {
    const runTs = String(new Date());
    contract_owner = getAccountByName("admin");
    other_account = getAccountByName("account_1");

    staking_token = new Contract('staking-token');
    mock_validator_contract = new Contract('counter');
    staking_contract = new Contract('staking-contract');

    await staking_token.parseSchema();
    await mock_validator_contract.parseSchema();
    await staking_contract.parseSchema();
    await staking_token.deploy(
        contract_owner,
        {
            amount: [{ amount: "1000000", denom: "uscrt" }],
            gas: "4000000",
        }
    );
    await mock_validator_contract.deploy(
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
    await mock_validator_contract.instantiate({"count": 102}, `validator list 2 ${runTs}`, contract_owner);
    let v1 =[];
    v1.push("secretvaloper1rxnt4f04rqtz43mezgajws4ffc2f94fkg5lnq2");
    v1.push("secretvaloper1t4crj8yjgwe0dmv2fzvf4jlc6se0amz0068dn3");
    await mock_validator_contract.tx.update_list(
      {account: contract_owner},
      {list: v1}
    );
    await staking_contract.instantiate(
    {
        "token_code_id": parseInt(staking_token.codeId),
        "token_code_hash": staking_token.contractCodeHash,
        "top_validator_code_hash": mock_validator_contract.contractCodeHash.toString(),  //"959b9bafb39053dcefd72e5b7fa2ac1f6c3266bbc109a1128494e7d483e85aca",
        "top_validator_contract_addr": mock_validator_contract.contractAddress, //"secret19se4h504nlallpcp6tgtmj76el9phwj3kcsf68", //
        "label": `SE staking token deposit ${String(new Date())}`,  // label for staking token init
        "dev_address": contract_owner.account.address,
        "prng_seed": "GDShgdiu",
        "contract_viewing_key": "eyfy5ftF",
        "threshold": "100000",  // 0.1 SCRT
        // "dev_fee": 10000,   // 10%
    },
    `SE staking contract ${runTs}`,
    contract_owner
    );

    const staking_info = await staking_contract.query.info();
    staking_token.instantiatedWithAddress(staking_info.info.token_address);

    console.log(await staking_contract.query.validator_list());

    const transferAmount_2 = [{ "denom": "uscrt", "amount": "7000000" }];
    await staking_contract.tx.add_to_whitelist({account: contract_owner}, {"address": other_account.account.address});
    await staking_contract.tx.stake(
      { account: other_account, transferAmount: transferAmount_2 }
    );
    await staking_contract.tx.claim_and_stake(
      { account: contract_owner }
    );
    const xc = await staking_contract.query.info();
    console.log("ValidatorAfterDeposit: ", xc.info.validators);
    let v2 =[];
    v2.push("secretvaloper1t4crj8yjgwe0dmv2fzvf4jlc6se0amz0068dn3");
    v2.push("secretvaloper15hsa7wequq97ecqkc7jsue86jx83m2xry6773g");
    v2.push("secretvaloper1s4s3tv30zngm6528pg5ef9sgeqjctd34yph9s3");

    await mock_validator_contract.tx.update_list(
      {account: contract_owner},
      {list: v2}
    );
    console.log(await mock_validator_contract.query.get_validators(
      {top:3, oth:0, com:0}
    ));
    const re_deleg_res = await staking_contract.tx.re_delegate(
      {account: contract_owner}
    );
    console.log("re_deleg_res => ",re_deleg_res);
    const xcf = await staking_contract.query.info();
    console.log("Validator info after re-delegating: ", xcf.info.validators);  
}

module.exports = { default: run };
