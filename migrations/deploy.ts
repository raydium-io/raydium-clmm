// Migrations are an early feature. Currently, they're nothing more than this
// single deploy script that's invoked from the CLI, injecting a provider
// configured from the workspace's Anchor.toml.

import anchor from "@project-serum/anchor";

module.exports = async function (provider) {
  // Configure client to use the provider.
  console.log('in migration')
  anchor.setProvider(provider);

  // Add your deploy script here.
  // const { connection, wallet } = anchor.getProvider()
  // console.log('wallet key', wallet.publicKey.toString())
}
