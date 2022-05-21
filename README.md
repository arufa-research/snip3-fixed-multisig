# SW3 Fixed Multisig
A Secret Network implementation of [cw3-fixed-multisig](https://github.com/CosmWasm/cw-plus/tree/main/contracts/cw3-fixed-multisig).

> This is a simple implementation of the [cw3 spec](../../packages/cw3/README.md).
It is a multisig with a fixed set of addresses created upon instatiation.
Each address may have the same weight (K of N), or some may have extra voting
power. This works much like the native Cosmos SDK multisig, except that rather
than aggregating the signatures off chain and submitting the final result,
we aggregate the approvals on-chain.
> 
> This is usable as is, and probably the most secure implementation of cw3
(as it is the simplest), but we will be adding more complex cases, such
as updating the multisig set, different voting rules for the same group
with different permissions, and even allow token-weighted voting. All through
the same client interface.

## Notes

Votes are public!

The list of voters cannot be changed after instantiation!

Modules `expiration.rs`, `math.rs`, and `threshold.rs` have been included directly instead of importing from external crates, because some modifications were needed to be compatible.

`error.rs` is not currently being used, but could be used for cleaner code. StdError::GenericErr is used throughout instead. 

Added a subtraction method for `Decimal` type in `math.rs`.

Three unit tests in `threshold.rs` have been disabled due to the presence of "Generic error: " at the beginning of the error message. 

## Differences From CW3 Fixed Multisig

All things related to contract versioning and migration have been removed as they are not compatible with Secret.

Numerous changes to storage methods since cw-storage-plus is not compatible with secret-cosmwasm-std, including iterations over storage.

`ListVotes{proposal_id, start_after, limit}` is unable to use `start_after`, and there is no use for `limit`. This query will return the full list of voters.

>## Instantiation
>
>To create the multisig, you must pass in a set of `HumanAddr` with a weight
for each one, as well as a required weight to pass a proposal. To create
a 2 of 3 multisig, pass 3 voters with weight 1 and a `required_weight` of 2.
>
>Note that 0 *is an allowed weight*. This doesn't give any voting rights, but
it does allow that key to submit proposals that can later be approved by the
voters. Any address not in the voter set cannot submit a proposal.
>
>## Execution Process
>
>First, a registered voter must submit a proposal. This also includes the
first "Yes" vote on the proposal by the proposer. The proposer can set
an expiration time for the voting process, or it defaults to the limit
provided when creating the contract (so proposals can be closed after several
days).
>
>Before the proposal has expired, any voter with non-zero weight can add their
vote. Only "Yes" votes are tallied. If enough "Yes" votes were submitted before
the proposal expiration date, the status is set to "Passed".
>
>Once a proposal is "Passed", anyone may submit an "Execute" message. This will
trigger the proposal to send all stored messages from the proposal and update
it's state to "Executed", so it cannot run again. (Note if the execution fails
for any reason - out of gas, insufficient funds, etc - the state update will
be reverted, and it will remain "Passed", so you can try again).
>
>Once a proposal has expired without passing, anyone can submit a "Close"
message to mark it closed. This has no effect beyond cleaning up the UI/database.

## Running this contract

You will need Rust 1.44.1+ with `wasm32-unknown-unknown` target installed.

You can run unit tests on this via: 

```
cargo test
```

You can run integration tests on this via Node.js and LocalSecret:

### Install Node Modules

```
cd tests/
npm install
```

### Install LocalSecret

```
docker pull ghcr.io/scrtlabs/localsecret
```

### Start LocalSecret (in a new terminal)

```
docker run -it -p 9091:9091 -p 26657:26657 -p 1317:1317 -p 5000:5000 --name localsecret ghcr.io/scrtlabs/localsecret
```

### Run Integration Tests from Root Project Directory

```
ts-node tests/integration.ts
```

Once you are happy with the content, you can compile it to wasm via:

```
RUSTFLAGS='-C link-arg=-s' cargo build --release --target wasm32-unknown-unknown --locked

wasm-opt -Oz ./target/wasm32-unknown-unknown/release/*.wasm -o ./contract.wasm

cat ./contract.wasm | gzip -9 > ./contract.wasm.gz
```
Or with docker:

```
docker run --rm -v "$(pwd)":/contract --mount type=volume,source="$(basename "$(pwd)")_cache",target=/code/target --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry enigmampc/secret-contract-optimizer
```
