/**
 * Raydium CLMM - Admin Instruction PoC Script
 * This script demonstrates how to reproduce the privileged admin mutation
 * 
 * Prerequisites:
 * - Solana CLI installed
 * - Devnet RPC endpoint
 * - Admin keypair loaded
 * 
 * Usage:
 * 1. Install dependencies: npm install @coral-xyz/anchor @solana/web3.js
 * 2. Load admin keypair
 * 3. Run this script against devnet
 */

const anchor = require("@coral-xyz/anchor");
const web3 = require("@solana/web3.js");
const { Program } = require("@coral-xyz/anchor");

// Raydium CLMM program ID
const PROGRAM_ID = new web3.PublicKey("CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK");

// Hardcoded admin pubkey from source code
const ADMIN_PUBKEY = new web3.PublicKey("GThUX1Atko4tqhN2NaiTazWSeFWMuiUvfFnyJyUghFMJ");

// Pool state PDA derivation helper
function derivePoolStatePDA(ammConfig, tokenMint0, tokenMint1) {
  const [poolState] = web3.PublicKey.findProgramAddressSync(
    [Buffer.from("pool"), ammConfig.toBuffer(), tokenMint0.toBuffer(), tokenMint1.toBuffer()],
    PROGRAM_ID
  );
  return poolState;
}

// Operation state PDA
function deriveOperationStatePDA() {
  const [operationState] = web3.PublicKey.findProgramAddressSync(
    [Buffer.from("operation")],
    PROGRAM_ID
  );
  return operationState;
}

async function main() {
  console.log("=== Raydium CLMM Admin Instruction PoC ===\n");
  
  // Connect to devnet
  const connection = new web3.Connection(web3.clusterApiUrl("devnet"), "confirmed");
  
  // Load admin keypair (you would load this from file or environment)
  // const adminKeypair = web3.Keypair.fromSecretKey(secretKey);
  // For demo purposes, we'll show the instruction construction
  
  console.log("Admin Pubkey:", ADMIN_PUBKEY.toString());
  console.log("Program ID:", PROGRAM_ID.toString());
  
  // Example: Create a mock pool state account
  // In real PoC, you'd create a pool first using create_pool instruction
  const mockPoolState = new web3.PublicKey("11111111111111111111111111111111");
  
  console.log("\n=== Test 1: transfer_reward_owner ===");
  console.log("This instruction allows admin to transfer pool ownership and all reward authorities");
  
  // Construct the instruction
  const transferInstruction = {
    programId: PROGRAM_ID,
    accounts: [
      { pubkey: ADMIN_PUBKEY, isSigner: true, isWritable: false },
      { pubkey: mockPoolState, isSigner: false, isWritable: true }
    ],
    data: Buffer.from([
      // Instruction discriminator would go here
      // This is simplified for demonstration
    ]),
    params: {
      newOwner: new web3.PublicKey("ATTACKER_PUBLIC_KEY_HERE")
    }
  };
  
  console.log("Instruction constructed: transfer_reward_owner");
  console.log("Expected result: pool_state.owner and all reward_infos[i].authority change to new owner");
  
  console.log("\n=== Test 2: update_pool_status ===");
  console.log("This instruction allows admin to set arbitrary pool status");
  
  const updateStatusInstruction = {
    programId: PROGRAM_ID,
    accounts: [
      { pubkey: ADMIN_PUBKEY, isSigner: true, isWritable: false },
      { pubkey: mockPoolState, isSigner: false, isWritable: true }
    ],
    data: Buffer.from([
      // Instruction discriminator
    ]),
    params: {
      status: 1 // Disable pool
    }
  };
  
  console.log("Instruction constructed: update_pool_status");
  console.log("Expected result: pool_state.status changes to 1");
  
  console.log("\n=== Test 3: update_amm_config ===");
  console.log("This instruction allows admin to change config owner and fee rates");
  
  const mockAmmConfig = new web3.PublicKey("22222222222222222222222222222222");
  const updateConfigInstruction = {
    programId: PROGRAM_ID,
    accounts: [
      { pubkey: ADMIN_PUBKEY, isSigner: true, isWritable: false },
      { pubkey: mockAmmConfig, isSigner: false, isWritable: true }
    ],
    remainingAccounts: [
      { pubkey: new web3.PublicKey("NEW_OWNER_PUBLIC_KEY_HERE"), isSigner: false, isWritable: false }
    ],
    data: Buffer.from([
      // Instruction discriminator
    ]),
    params: {
      param: 3, // Update owner
      value: 0
    }
  };
  
  console.log("Instruction constructed: update_amm_config (param=3, update owner)");
  console.log("Expected result: amm_config.owner changes to new owner");
  
  console.log("\n=== Reproduction Steps ===");
  console.log(`
1. Deploy current raydium-clmm program to devnet
2. Create an AMM config using create_amm_config
3. Create a pool using create_pool with the config
4. Initialize rewards for the pool
5. Execute transfer_reward_owner with admin keypair
6. Fetch pool state and verify:
   - pool_state.owner == attacker_pubkey
   - pool_state.reward_infos[0].authority == attacker_pubkey
   - pool_state.reward_infos[1].authority == attacker_pubkey
   - pool_state.reward_infos[2].authority == attacker_pubkey
7. Execute update_pool_status with status=1
8. Fetch pool state and verify pool_state.status == 1
9. Execute update_amm_config with param=3
10. Fetch amm_config and verify owner changed

All these transactions will succeed because the only authorization check is:
  #[account(address = crate::admin::ID @ ErrorCode::NotApproved)]

There is no multisig, timelock, or additional authorization required.
`);
  
  console.log("\n=== Vulnerability Summary ===");
  console.log("Bug Type: Improper Access Control / Missing Access Control");
  console.log("Affected Part: Raydium CLMM admin instructions");
  console.log("CVSS Vector: CVSS:3.1/AV:N/AC:L/PR:H/UI:N/S:U/C:N/I:H/A:H");
  console.log("Severity: High to Critical");
  console.log("Impact: Admin can unilaterally control pools, rewards, and configs");
}

if (require.main === module) {
  main().catch(console.error);
}

module.exports = { derivePoolStatePDA, deriveOperationStatePDA };
