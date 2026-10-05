import { Connection, PublicKey, SYSVAR_CLOCK_PUBKEY } from "@solana/web3.js";

const UPGRADEABLE_LOADER = new PublicKey(
  "BPFLoaderUpgradeab1e11111111111111111111111"
);

async function rpcCall(endpoint: string, method: string, params: any[]) {
  const response = await (globalThis as any).fetch(endpoint, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }),
  });
  const json = await response.json();
  if (json.error) {
    throw new Error(`${method}: ${JSON.stringify(json.error)}`);
  }
  return json.result;
}

const PROGRAM_TAG = 2;
const PROGRAMDATA_TAG = 3;

/// Install the stub hook into the surfnet, upgradeable under `upgradeAuthority`.
///
/// Written straight into the ledger rather than listed in `[programs.Localnet]`: a second entry
/// there stops the repo's own program from deploying. It also lets a test pick the upgrade
/// authority, which is what the launchpad actually checks.
export async function installStubHookProgram(
  connection: Connection,
  programId: PublicKey,
  soPath: string,
  upgradeAuthority: PublicKey
): Promise<void> {
  const fs = await import("fs");
  const elf = fs.readFileSync(soPath);

  const [programdataAddress] = PublicKey.findProgramAddressSync(
    [programId.toBuffer()],
    UPGRADEABLE_LOADER
  );

  // UpgradeableLoaderState::ProgramData { slot, Some(authority) } then the ELF
  const programdata = Buffer.concat([
    u32le(PROGRAMDATA_TAG),
    Buffer.alloc(8), // slot 0
    Buffer.from([1]),
    upgradeAuthority.toBuffer(),
    elf,
  ]);
  // UpgradeableLoaderState::Program { programdata_address }
  const program = Buffer.concat([
    u32le(PROGRAM_TAG),
    programdataAddress.toBuffer(),
  ]);

  // programdata first, so the program account never points at nothing
  for (const [address, data, executable] of [
    [programdataAddress, programdata, false],
    [programId, program, true],
  ] as const) {
    await rpcCall(connection.rpcEndpoint, "surfnet_setAccount", [
      address.toBase58(),
      {
        lamports: 10 * 1e9,
        data: data.toString("hex"),
        owner: UPGRADEABLE_LOADER.toBase58(),
        executable,
        rent_epoch: 0,
      },
    ]);
  }
}

/// Move the surfnet clock forward to at least `unixSeconds`.
///
/// The surfnet lands short of the requested time by an amount that grows with the jump, so
/// aim past it and top up the shortfall until it is reached.
export async function timeTravelTo(
  connection: Connection,
  unixSeconds: number
): Promise<void> {
  let aim = unixSeconds + 60;
  for (let attempt = 0; attempt < 5; attempt++) {
    await rpcCall(connection.rpcEndpoint, "surfnet_timeTravel", [
      { absoluteTimestamp: aim * 1000 },
    ]);
    await new Promise((r) => setTimeout(r, 500));
    const now = await chainTime(connection);
    if (now >= unixSeconds) return;
    aim += unixSeconds - now + 60;
  }
  throw new Error(`surfnet clock did not reach ${unixSeconds}`);
}

/// Overwrite an account's data in place, keeping its owner; funds it for the new size.
export async function setAccountData(
  connection: Connection,
  address: PublicKey,
  data: Buffer
): Promise<void> {
  const info = await connection.getAccountInfo(address);
  if (info === null) throw new Error(`${address.toBase58()} does not exist`);
  await rpcCall(connection.rpcEndpoint, "surfnet_setAccount", [
    address.toBase58(),
    {
      lamports: await connection.getMinimumBalanceForRentExemption(data.length),
      data: data.toString("hex"),
      owner: info.owner.toBase58(),
      executable: info.executable,
      rent_epoch: 0,
    },
  ]);
}

/// The surfnet's own clock, read from the Clock sysvar the program sees; it runs ahead of the
/// host's once a test has time travelled, and ahead of `getBlockTime` right after a jump.
export async function chainTime(connection: Connection): Promise<number> {
  const info = await connection.getAccountInfo(SYSVAR_CLOCK_PUBKEY);
  if (info === null) throw new Error("Clock sysvar missing");
  // Clock { slot, epoch_start_timestamp, epoch, leader_schedule_epoch, unix_timestamp }
  return Number(info.data.readBigInt64LE(32));
}

function u32le(value: number): Buffer {
  const buf = Buffer.alloc(4);
  buf.writeUInt32LE(value);
  return buf;
}
