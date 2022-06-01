const { Contract, getAccountByName } = require("secret-polar");
const { validators } = require("./validators_pulsar.json");
const { fromUtf8 } = require("@iov/encoding");
const { addresses } = require("./whitelist.json");

async function run() {
    const runTs = String(new Date());
    const contract_owner = getAccountByName("deployer");

    const staking_token = new Contract('staking-token');
    const staking_contract = new Contract('staking-contract');

    await staking_token.parseSchema();
    await staking_contract.parseSchema();

    // deploy staking token, $seSCRT
    const staking_token_deploy_res = await staking_token.deploy(
        contract_owner
    );
    console.log(staking_token_deploy_res);

    // deploy staking contract
    const staking_contract_deploy_res = await staking_contract.deploy(
        contract_owner
    );
    console.log(staking_contract_deploy_res);
    // init staking contract
    const staking_contract_info = await staking_contract.instantiate(
        {
            "token_code_id": parseInt(staking_token.codeId),
            "token_code_hash": staking_token.contractCodeHash,
            "top_validator_code_hash": "0800fee06bb3afe8aa8adb5695fce231eee1bcae615c91f4b9c8eb31fa5ea0d4",
            "top_validator_contract_addr": "secret1agf7ap69v2wedgqnqx24dgaqp58zw8csndlh05",
            "label": `seSCRT`,  // label for staking token init
            "dev_address": "secret1hx967un8shkr0mgar8qa8y7s3dggygwtv2zwv2",
            "prng_seed": "GDShgdiuasdas",
            "contract_viewing_key": "eyfy5fdasdastF",
            "threshold": "100000",  // 0.1 SCRT
            "dev_fee": 3000,   // 3%
            "sscrt_token_contract_hash": "AF74387E276BE8874F07BEC3A87023EE49B0E7EBE08178C49D0A49C3C98ED60E",
            "sscrt_address": "secret1k0jntykt7e4g3y88ltc60czgjuqdy4c9e8fzek",
        },
        `StakeEasy-staking-contract`,
        contract_owner
    );
    console.log(staking_contract_info);

    const staking_info = await staking_contract.query.info();
    console.log(JSON.stringify(staking_info, null, 2));
    staking_token.instantiatedWithAddress(staking_info.info.token_address);
    await staking_contract.tx.add_to_whitelist({ account: contract_owner }, { "address": "secret16l280j0kxd95q7au09hx0ry7s69mjxaltu20qd" });
    await staking_contract.tx.set_white({ account: contract_owner }, { "white": true, "track": false });

}

module.exports = { default: run };
