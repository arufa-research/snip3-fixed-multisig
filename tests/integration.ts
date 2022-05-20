import axios from "axios";
import { Wallet, SecretNetworkClient, fromUtf8 } from "secretjs";
import fs from "fs";
import assert from "assert";

let wallet_c: string;

// Returns a client with which we can interact with secret network
const initializeClientA = async (endpoint: string, chainId: string) => {
  const wallet = new Wallet(); // Use default constructor of wallet to generate random mnemonic.
  const accAddress = wallet.address;
  const client = await SecretNetworkClient.create({
    // Create a client to interact with the network
    grpcWebUrl: endpoint,
    chainId: chainId,
    wallet: wallet,
    walletAddress: accAddress,
  });

  console.log(`Initialized client A with wallet address: ${accAddress}`);
  return client;
};

const initializeClientB = async (endpoint: string, chainId: string) => {
  const wallet = new Wallet(); // Use default constructor of wallet to generate random mnemonic.
  const accAddress = wallet.address;
  const client = await SecretNetworkClient.create({
    // Create a client to interact with the network
    grpcWebUrl: endpoint,
    chainId: chainId,
    wallet: wallet,
    walletAddress: accAddress,
  });

  console.log(`Initialized client B with wallet address: ${accAddress}`);
  return client;
};

const initializeClientC = async (endpoint: string, chainId: string) => {
  const wallet = new Wallet(); // Use default constructor of wallet to generate random mnemonic.
  const accAddress = wallet.address;
  const client = await SecretNetworkClient.create({
    // Create a client to interact with the network
    grpcWebUrl: endpoint,
    chainId: chainId,
    wallet: wallet,
    walletAddress: accAddress,
  });

  console.log(`Initialized client C with wallet address: ${accAddress}`);
  return client;
};

// Stores and instantiaties a new contract in our network
const initializeContract = async (
  client_a: SecretNetworkClient,
  client_b: SecretNetworkClient,
  client_c: SecretNetworkClient,
  contractPath: string
) => {
  const wasmCode = fs.readFileSync(contractPath);
  console.log("Uploading contract");

  const uploadReceipt = await client_a.tx.compute.storeCode(
    {
      wasmByteCode: wasmCode,
      sender: client_a.address,
      source: "",
      builder: "",
    },
    {
      gasLimit: 5000000,
    }
  );

  if (uploadReceipt.code !== 0) {
    console.log(
      `Failed to get code id: ${JSON.stringify(uploadReceipt.rawLog)}`
    );
    throw new Error(`Failed to upload contract`);
  }

  const codeIdKv = uploadReceipt.jsonLog![0].events[0].attributes.find(
    (a: any) => {
      return a.key === "code_id";
    }
  );

  const codeId = Number(codeIdKv!.value);
  console.log("Contract codeId: ", codeId);

  const contractCodeHash = await client_a.query.compute.codeHash(codeId);
  console.log(`Contract hash: ${contractCodeHash}`);

  const contract = await client_a.tx.compute.instantiateContract(
    {
      sender: client_a.address,
      codeId,
      initMsg: {
        voters: [{addr: client_a.address, weight: 1},{addr: client_b.address, weight: 1},{addr: client_c.address, weight: 1}],
        threshold: { absolute_count: {weight: 2} },
        max_voting_period: {height: 1000}
      },
      codeHash: contractCodeHash,
      label: "My contract" + Math.ceil(Math.random() * 10000), // The label should be unique for every contract, add random string in order to maintain uniqueness
    },
    {
      gasLimit: 1000000,
    }
  );

  if (contract.code !== 0) {
    throw new Error(
      `Failed to instantiate the contract with the following error ${contract.rawLog}`
    );
  }

  const contractAddress = contract.arrayLog!.find(
    (log) => log.type === "message" && log.key === "contract_address"
  )!.value;

  console.log(`Contract address: ${contractAddress}`);

  var contractInfo: [string, string] = [contractCodeHash, contractAddress];
  return contractInfo;
};

const getFromFaucet = async (address: string) => {
  await axios.get(`http://localhost:5000/faucet?address=${address}`);
};

async function getScrtBalance(userCli: SecretNetworkClient): Promise<string> {
  let balanceResponse = await userCli.query.bank.balance({
    address: userCli.address,
    denom: "uscrt",
  });
  return balanceResponse.balance!.amount;
}

async function fillUpFromFaucet(
  client: SecretNetworkClient,
  targetBalance: Number
) {
  let balance = await getScrtBalance(client);
  while (Number(balance) < targetBalance) {
    try {
      await getFromFaucet(client.address);
    } catch (e) {
      console.error(`failed to get tokens from faucet: ${e}`);
    }
    balance = await getScrtBalance(client);
  }
  console.error(`got tokens from faucet: ${balance}`);
}

// Initialization procedure
async function initializeAndUploadContract() {
  let endpoint = "http://localhost:9091";
  let chainId = "secretdev-1";

  const client_a = await initializeClientA(endpoint, chainId);
  const client_b = await initializeClientB(endpoint, chainId);
  const client_c = await initializeClientC(endpoint, chainId);


  await fillUpFromFaucet(client_a, 100_000_000);
  await fillUpFromFaucet(client_b, 100_000_000);
  await fillUpFromFaucet(client_c, 100_000_000);


  const [contractHash, contractAddress] = await initializeContract(
    client_a,
    client_b,
    client_c,
    "contract.wasm.gz"
  );

  var clientInfo: [SecretNetworkClient, SecretNetworkClient, SecretNetworkClient, string, string] = [
    client_a,
    client_b,
    client_c,
    contractHash,
    contractAddress,
  ];
  return clientInfo;
}

async function queryProposal(
  client: SecretNetworkClient,
  contractHash: string,
  contractAddress: string,
  proposal_id: number
): Promise<String> {
  type ProposalResponse = {
    id: number,
    title: String,
    description: String,
    msgs: number[],
    status: number,
    expires: number,
    threshold: {weight: number, total_weight: number},
  };

  const proposalResponse = (await client.query.compute.queryContract({
    contractAddress: contractAddress,
    codeHash: contractHash,
    query: { proposal: { proposal_id: proposal_id} },
  })) as ProposalResponse;

  if ('err"' in proposalResponse) {
    throw new Error(
      `Query failed with the following err: ${JSON.stringify(proposalResponse)}`
    );
  }
  console.log(proposalResponse);
  return proposalResponse.title;
}

async function queryThreshold(
  client: SecretNetworkClient,
  contractHash: string,
  contractAddress: string
): Promise<number> {
  type ThresholdResponse = { absolute_count: { weight: number, total_weight: number } };

  const thresholdResponse = (await client.query.compute.queryContract({
    contractAddress: contractAddress,
    codeHash: contractHash,
    query: { threshold: {} },
  })) as ThresholdResponse;

  if ('err"' in thresholdResponse) {
    throw new Error(
      `Query failed with the following err: ${JSON.stringify(thresholdResponse)}`
    );
  }
  console.log(thresholdResponse);
  return thresholdResponse.absolute_count.total_weight;
}

async function queryListProposals(
  client: SecretNetworkClient,
  contractHash: string,
  contractAddress: string
) {

  type ProposalResponse = {
    id: number,
    title: String,
    description: String,
    msgs: number[],
    status: number,
    expires: {at_height: number},
    threshold: { absolute_count: {weight: number, total_weight: number}},
  };
  type ProposalListResponse = { proposals: ProposalResponse[] };

  const proposalListResponse = (await client.query.compute.queryContract({
    contractAddress: contractAddress,
    codeHash: contractHash,
    query: { list_proposals: {} }
  })) as ProposalListResponse;

  if ('err"' in proposalListResponse) {
    throw new Error(
      `Query failed with the following err: ${JSON.stringify(proposalListResponse)}`
    );
  }
  console.log(proposalListResponse);
}

async function queryReverseProposals(
  client: SecretNetworkClient,
  contractHash: string,
  contractAddress: string
) {

  type ProposalResponse = {
    id: number,
    title: String,
    description: String,
    msgs: number[],
    status: number,
    expires: {at_height: number},
    threshold: { absolute_count: {weight: number, total_weight: number}},
  };
  type ProposalListResponse = { proposals: ProposalResponse[] };

  const proposalReverseResponse = (await client.query.compute.queryContract({
    contractAddress: contractAddress,
    codeHash: contractHash,
    query: { reverse_proposals: {} }
  })) as ProposalListResponse;

  if ('err"' in proposalReverseResponse) {
    throw new Error(
      `Query failed with the following err: ${JSON.stringify(proposalReverseResponse)}`
    );
  }
  console.log(proposalReverseResponse);
}

async function queryVote(
  client: SecretNetworkClient,
  contractHash: string,
  contractAddress: string,
) {
  enum Vote {
    Yes,
    No,
    Abstain,
    Veto
  }

  type VoteInfo = {
    proposal_id: number,
    voter: String,
    vote: Vote,
    weight: number,
  };
  
  type VoteResponse = { vote?: VoteInfo };

  const voteResponse = (await client.query.compute.queryContract({
    contractAddress: contractAddress,
    codeHash: contractHash,
    query: { vote: { proposal_id: 1, voter: client.address } }
  })) as VoteResponse;

  if ('err"' in voteResponse) {
    throw new Error(
      `Query failed with the following err: ${JSON.stringify(voteResponse)}`
    );
  }
  console.log(voteResponse);
}

async function queryListVotes(  client: SecretNetworkClient,
  contractHash: string,
  contractAddress: string
) {
  enum Vote {
    Yes,
    No,
    Abstain,
    Veto
  }

  type VoteInfo = {
    proposal_id: number,
    voter: String,
    vote: Vote,
    weight: number,
  };
  
  type VoteListResponse = { votes: VoteInfo[] };

  const voteListResponse = (await client.query.compute.queryContract({
    contractAddress: contractAddress,
    codeHash: contractHash,
    query: { list_votes: { proposal_id: 1 } }
  })) as VoteListResponse;

  if ('err"' in voteListResponse) {
    throw new Error(
      `Query failed with the following err: ${JSON.stringify(voteListResponse)}`
    );
  }
  console.log(voteListResponse);
}

async function queryListVoters(
  client: SecretNetworkClient,
  contractHash: string,
  contractAddress: string
) {

  type Voter = {
    addr: String,
    weight: number,
  };

  type VoterListResponse = { voters: Voter[] };

  const voterListResponse = (await client.query.compute.queryContract({
    contractAddress: contractAddress,
    codeHash: contractHash,
    query: { list_voters: {} }
  })) as VoterListResponse;

  if ('err"' in voterListResponse) {
    throw new Error(
      `Query failed with the following err: ${JSON.stringify(voterListResponse)}`
    );
  }
  console.log(voterListResponse);
}

async function handleProposal1(
  client: SecretNetworkClient,
  contractHash: string,
  contractAddress: string
) {
  const tx = await client.tx.compute.executeContract(
    {
      sender: client.address,
      contractAddress: contractAddress,
      codeHash: contractHash,
      msg: {
        propose: {
          description: "prop 1 description",
          msgs: [],
          title: "Proposal 1",
          latest: {at_height: 9999},
         },
      },
      sentFunds: [],
    },
    {
      gasLimit: 200000,
    }
  );
  console.log(tx);
  console.log(`Create Propsosal TX used ${tx.gasUsed} gas`);
}

async function handleProposal2(
  client: SecretNetworkClient,
  contractHash: string,
  contractAddress: string
) {
  const tx = await client.tx.compute.executeContract(
    {
      sender: client.address,
      contractAddress: contractAddress,
      codeHash: contractHash,
      msg: {
        propose: {
          description: "prop 2 description",
          msgs: [],
          title: "Proposal 2",
          latest: {at_height: 9999},
         },
      },
      sentFunds: [],
    },
    {
      gasLimit: 200000,
    }
  );
  console.log(tx);
  console.log(`Create Propsosal 2 TX used ${tx.gasUsed} gas`);
}

async function handleProposal3(
  client: SecretNetworkClient,
  contractHash: string,
  contractAddress: string
) {
  const tx = await client.tx.compute.executeContract(
    {
      sender: client.address,
      contractAddress: contractAddress,
      codeHash: contractHash,
      msg: {
        propose: {
          description: "prop 3 description",
          msgs: [],
          title: "Proposal 3",
          latest: {at_height: 9999},
         },
      },
      sentFunds: [],
    },
    {
      gasLimit: 200000,
    }
  );
  console.log(tx);
  console.log(`Create Propsosal 3 TX used ${tx.gasUsed} gas`);
}

async function handleVoteYes(
  client: SecretNetworkClient,
  contractHash: string,
  contractAddress: string
) {
  const tx = await client.tx.compute.executeContract(
    {
      sender: client.address,
      contractAddress: contractAddress,
      codeHash: contractHash,
      msg: {
        vote: {
          proposal_id: 1,
          vote: "yes",
         },
      },
      sentFunds: [],
    },
    {
      gasLimit: 200000,
    }
  );
  console.log(tx);
  console.log(`Vote TX used ${tx.gasUsed} gas`);
}

async function handleExecute(
  client: SecretNetworkClient,
  contractHash: string,
  contractAddress: string
) {
  const tx = await client.tx.compute.executeContract(
    {
      sender: client.address,
      contractAddress: contractAddress,
      codeHash: contractHash,
      msg: {
        execute: {
          proposal_id: 1
         },
      },
      sentFunds: [],
    },
    {
      gasLimit: 200000,
    }
  );
  console.log(tx);
  console.log(`Execute Propsosal TX used ${tx.gasUsed} gas`);
}

async function handleClose(
  client: SecretNetworkClient,
  contractHash: string,
  contractAddress: string
) {
  const tx = await client.tx.compute.executeContract(
    {
      sender: client.address,
      contractAddress: contractAddress,
      codeHash: contractHash,
      msg: {
        close: {
          proposal_id: 1
         },
      },
      sentFunds: [],
    },
    {
      gasLimit: 200000,
    }
  );
  console.log(tx);
  console.log(`Close Propsosal TX used ${tx.gasUsed} gas`);
}

async function create_proposals(
  client: SecretNetworkClient,
  contractHash: string,
  contractAddress: string
) {
  await handleProposal1(client, contractHash, contractAddress);
  await handleProposal2(client, contractHash, contractAddress);
  await handleProposal3(client, contractHash, contractAddress);
  // let tx1 = await queryProposal(client, contractHash, contractAddress, 1);
  // let tx2 = await queryProposal(client, contractHash, contractAddress, 2);
  // let tx3 = await queryProposal(client, contractHash, contractAddress, 3);
}

async function runTestFunction(
  tester: (
    client: SecretNetworkClient,
    contractHash: string,
    contractAddress: string
  ) => void,
  client: SecretNetworkClient,
  contractHash: string,
  contractAddress: string
) {
  console.log(`Testing ${tester.name}`);
  await tester(client, contractHash, contractAddress);
  console.log(`[SUCCESS] ${tester.name}`);
}

(async () => {
  const [client_a, client_b, client_c, contractHash, contractAddress] =
    await initializeAndUploadContract();

  await runTestFunction(
    queryThreshold,
    client_a,
    contractHash,
    contractAddress
  );
  await runTestFunction(
    create_proposals,
    client_a,
    contractHash,
    contractAddress
  );
  await runTestFunction(
    queryListProposals,
    client_a,
    contractHash,
    contractAddress
  );
  await runTestFunction(
    queryReverseProposals,
    client_a,
    contractHash,
    contractAddress
  );
  await runTestFunction(
    handleVoteYes,
    client_c, // c votes "yes"
    contractHash,
    contractAddress
  );
  await runTestFunction(
    queryListVotes,
    client_a,
    contractHash,
    contractAddress
  );
  await runTestFunction(
    queryListVoters,
    client_a,
    contractHash,
    contractAddress
  );
  await runTestFunction(
    queryVote,
    client_a,
    contractHash,
    contractAddress
  );
})();
