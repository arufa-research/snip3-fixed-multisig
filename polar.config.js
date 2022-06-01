const accounts = [
  {
    name: 'admin',
    address: 'secret19deq5xnfzums2xuqxfzkgg3ztrhfemmeay6vzm',
    mnemonic: 'track whisper what teach scheme fox twist atom drive chunk clog resist unknown month special toast evidence wonder major joke design gain insect lift'
  },
  {
    name: 'account_1',
    address: 'secret1ddfphwwzqtkp8uhcsc53xdu24y9gks2kug45zv',
    mnemonic: 'sorry object nation also century glove small tired parrot avocado pulp purchase'
  },
  {
    name: 'account_2',
    address: 'secret1mg2um4yztjq098kt2fvg7uee0fzzsm3nh75pmm',
    mnemonic: 'plug pair three fox resemble cute pig glad rhythm solve puppy place tag improve render monitor coral survey proof snake enrich feature ticket young'
  }
];

const networks = {
  localnet: {
    endpoint: 'http://localhost:1337/'
  },
  // Pulsar-2
  testnet: {
    endpoint: 'http://testnet.securesecrets.org:1317/',
    chainId: 'pulsar-2',
    trustNode: true,
    keyringBackend: 'test',
    accounts: accounts,
    fees: {
      upload: {
          amount: [{ amount: "500000", denom: "uscrt" }],
          gas: "4000000",
      },
      init: {
          amount: [{ amount: "125000", denom: "uscrt" }],
          gas: "500000",
      },
      exec: {
        amount: [{ amount: "125000", denom: "uscrt" }],
        gas: "500000",
    },
    }
  },
  development: {
    endpoint: 'tcp://0.0.0.0:26656',
    nodeId: '115aa0a629f5d70dd1d464bc7e42799e00f4edae',
    chainId: 'enigma-pub-testnet-3',
    keyringBackend: 'test',
    types: {}
  },
  // Supernova Testnet
  supernova: {
    endpoint: 'http://bootstrap.supernova.enigma.co:1317',
    chainId: 'supernova-2',
    trustNode: true,
    keyringBackend: 'test',
    accounts: accounts,
    types: {},
    fees: {
      upload: {
          amount: [{ amount: "500000", denom: "uscrt" }],
          gas: "2000000",
      },
      init: {
          amount: [{ amount: "125000", denom: "uscrt" }],
          gas: "500000",
      },
    }
  }
};

module.exports = {
  networks: {
    default: networks.testnet,
    localnet: networks.localnet,
    development: networks.development
  },
  mocha: {
    timeout: 6000000
  },
  rust: {
    version: "nightly-2020-12-31",
  }
};