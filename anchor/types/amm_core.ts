export type AmmCore = {
  "version": "0.1.0",
  "name": "amm_core",
  "instructions": [
    {
      "name": "createAmmConfig",
      "accounts": [
        {
          "name": "owner",
          "isMut": true,
          "isSigner": true
        },
        {
          "name": "ammConfig",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "systemProgram",
          "isMut": false,
          "isSigner": false
        }
      ],
      "args": []
    },
    {
      "name": "setNewOwner",
      "accounts": [
        {
          "name": "owner",
          "isMut": false,
          "isSigner": true
        },
        {
          "name": "newOwner",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "ammConfig",
          "isMut": true,
          "isSigner": false
        }
      ],
      "args": []
    },
    {
      "name": "createFeeAccount",
      "accounts": [
        {
          "name": "owner",
          "isMut": true,
          "isSigner": true
        },
        {
          "name": "ammConfig",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "feeState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "systemProgram",
          "isMut": false,
          "isSigner": false
        }
      ],
      "args": [
        {
          "name": "fee",
          "type": "u32"
        },
        {
          "name": "tickSpacing",
          "type": "u16"
        }
      ]
    },
    {
      "name": "createPool",
      "accounts": [
        {
          "name": "poolCreator",
          "isMut": true,
          "isSigner": true
        },
        {
          "name": "poolState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tokenMint0",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "tokenMint1",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "tokenVault0",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tokenVault1",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "feeState",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "initialObservationState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tokenProgram",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "systemProgram",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "rent",
          "isMut": false,
          "isSigner": false
        }
      ],
      "args": [
        {
          "name": "sqrtPrice",
          "type": "u64"
        }
      ]
    },
    {
      "name": "increaseObservationCardinalityNext",
      "accounts": [
        {
          "name": "payer",
          "isMut": false,
          "isSigner": true
        },
        {
          "name": "poolState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "systemProgram",
          "isMut": false,
          "isSigner": false
        }
      ],
      "args": [
        {
          "name": "observationAccountBumps",
          "type": "bytes"
        }
      ]
    },
    {
      "name": "setProtocolFeeRate",
      "accounts": [
        {
          "name": "owner",
          "isMut": false,
          "isSigner": true
        },
        {
          "name": "ammConfig",
          "isMut": true,
          "isSigner": false
        }
      ],
      "args": [
        {
          "name": "feeProtocol",
          "type": "u8"
        }
      ]
    },
    {
      "name": "collectProtocolFee",
      "accounts": [
        {
          "name": "owner",
          "isMut": false,
          "isSigner": true
        },
        {
          "name": "ammConfig",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "poolState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tokenVault0",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tokenVault1",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "recipientTokenAccount0",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "recipientTokenAccount1",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tokenProgram",
          "isMut": false,
          "isSigner": false
        }
      ],
      "args": [
        {
          "name": "amount0Requested",
          "type": "u64"
        },
        {
          "name": "amount1Requested",
          "type": "u64"
        }
      ]
    },
    {
      "name": "createTickAccount",
      "accounts": [
        {
          "name": "signer",
          "isMut": true,
          "isSigner": true
        },
        {
          "name": "poolState",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "tickState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "systemProgram",
          "isMut": false,
          "isSigner": false
        }
      ],
      "args": [
        {
          "name": "tick",
          "type": "i32"
        }
      ]
    },
    {
      "name": "createBitmapAccount",
      "accounts": [
        {
          "name": "signer",
          "isMut": true,
          "isSigner": true
        },
        {
          "name": "poolState",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "bitmapState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "systemProgram",
          "isMut": false,
          "isSigner": false
        }
      ],
      "args": [
        {
          "name": "wordPos",
          "type": "i16"
        }
      ]
    },
    {
      "name": "createProcotolPosition",
      "accounts": [
        {
          "name": "signer",
          "isMut": true,
          "isSigner": true
        },
        {
          "name": "ammConfig",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "poolState",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "tickLowerState",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "tickUpperState",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "positionState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "systemProgram",
          "isMut": false,
          "isSigner": false
        }
      ],
      "args": []
    },
    {
      "name": "createPersonalPosition",
      "accounts": [
        {
          "name": "minter",
          "isMut": true,
          "isSigner": true
        },
        {
          "name": "positionNftOwner",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "ammConfig",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "positionNftMint",
          "isMut": true,
          "isSigner": true
        },
        {
          "name": "positionNftAccount",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "poolState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "protocolPositionState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tickLowerState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tickUpperState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "bitmapLowerState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "bitmapUpperState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "personalPositionState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tokenAccount0",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tokenAccount1",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tokenVault0",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tokenVault1",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "lastObservationState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "rent",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "systemProgram",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "tokenProgram",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "associatedTokenProgram",
          "isMut": false,
          "isSigner": false
        }
      ],
      "args": [
        {
          "name": "amount0Desired",
          "type": "u64"
        },
        {
          "name": "amount1Desired",
          "type": "u64"
        },
        {
          "name": "amount0Min",
          "type": "u64"
        },
        {
          "name": "amount1Min",
          "type": "u64"
        }
      ]
    },
    {
      "name": "personalPositionWithMetadata",
      "accounts": [
        {
          "name": "payer",
          "isMut": true,
          "isSigner": true
        },
        {
          "name": "ammConfig",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "nftMint",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "positionState",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "metadataAccount",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "rent",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "metadataProgram",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "tokenProgram",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "systemProgram",
          "isMut": false,
          "isSigner": false
        }
      ],
      "args": []
    },
    {
      "name": "increaseLiquidity",
      "accounts": [
        {
          "name": "payer",
          "isMut": false,
          "isSigner": true
        },
        {
          "name": "ammConfig",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "personalPositionState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "poolState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "protocolPositionState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tickLowerState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tickUpperState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "bitmapLowerState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "bitmapUpperState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tokenAccount0",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tokenAccount1",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tokenVault0",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tokenVault1",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "lastObservationState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tokenProgram",
          "isMut": false,
          "isSigner": false
        }
      ],
      "args": [
        {
          "name": "amount0Desired",
          "type": "u64"
        },
        {
          "name": "amount1Desired",
          "type": "u64"
        },
        {
          "name": "amount0Min",
          "type": "u64"
        },
        {
          "name": "amount1Min",
          "type": "u64"
        }
      ]
    },
    {
      "name": "decreaseLiquidity",
      "accounts": [
        {
          "name": "ownerOrDelegate",
          "isMut": false,
          "isSigner": true
        },
        {
          "name": "nftAccount",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "personalPositionState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "ammConfig",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "poolState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "protocolPositionState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tickLowerState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tickUpperState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "bitmapLowerState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "bitmapUpperState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "lastObservationState",
          "isMut": true,
          "isSigner": false
        }
      ],
      "args": [
        {
          "name": "liquidity",
          "type": "u64"
        },
        {
          "name": "amount0Min",
          "type": "u64"
        },
        {
          "name": "amount1Min",
          "type": "u64"
        }
      ]
    },
    {
      "name": "collectFee",
      "accounts": [
        {
          "name": "ownerOrDelegate",
          "isMut": false,
          "isSigner": true
        },
        {
          "name": "nftAccount",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "personalPositionState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "ammConfig",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "poolState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "protocolPositionState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tickLowerState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tickUpperState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "bitmapLowerState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "bitmapUpperState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "lastObservationState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tokenVault0",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tokenVault1",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "recipientTokenAccount0",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "recipientTokenAccount1",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tokenProgram",
          "isMut": false,
          "isSigner": false
        }
      ],
      "args": [
        {
          "name": "amount0Max",
          "type": "u64"
        },
        {
          "name": "amount1Max",
          "type": "u64"
        }
      ]
    },
    {
      "name": "swapBaseInSingle",
      "accounts": [
        {
          "name": "signer",
          "isMut": false,
          "isSigner": true
        },
        {
          "name": "ammConfig",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "poolState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "inputTokenAccount",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "outputTokenAccount",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "inputVault",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "outputVault",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "lastObservationState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tokenProgram",
          "isMut": false,
          "isSigner": false
        }
      ],
      "args": [
        {
          "name": "amountIn",
          "type": "u64"
        },
        {
          "name": "amountOutMinimum",
          "type": "u64"
        },
        {
          "name": "sqrtPriceLimit",
          "type": "u64"
        }
      ]
    },
    {
      "name": "swapBaseIn",
      "accounts": [
        {
          "name": "signer",
          "isMut": false,
          "isSigner": true
        },
        {
          "name": "ammConfig",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "inputTokenAccount",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tokenProgram",
          "isMut": false,
          "isSigner": false
        }
      ],
      "args": [
        {
          "name": "amountIn",
          "type": "u64"
        },
        {
          "name": "amountOutMinimum",
          "type": "u64"
        },
        {
          "name": "additionalAccountsPerPool",
          "type": "bytes"
        }
      ]
    }
  ],
  "accounts": [
    {
      "name": "ammConfig",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "bump",
            "type": "u8"
          },
          {
            "name": "owner",
            "type": "publicKey"
          },
          {
            "name": "protocolFee",
            "type": "u8"
          }
        ]
      }
    },
    {
      "name": "feeState",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "bump",
            "type": "u8"
          },
          {
            "name": "fee",
            "type": "u32"
          },
          {
            "name": "tickSpacing",
            "type": "u16"
          }
        ]
      }
    },
    {
      "name": "observationState",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "bump",
            "type": "u8"
          },
          {
            "name": "index",
            "type": "u16"
          },
          {
            "name": "blockTimestamp",
            "type": "u32"
          },
          {
            "name": "tickCumulative",
            "type": "i64"
          },
          {
            "name": "secondsPerLiquidityCumulativeX32",
            "type": "u64"
          },
          {
            "name": "initialized",
            "type": "bool"
          }
        ]
      }
    },
    {
      "name": "poolState",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "bump",
            "type": "u8"
          },
          {
            "name": "tokenMint0",
            "type": "publicKey"
          },
          {
            "name": "tokenMint1",
            "type": "publicKey"
          },
          {
            "name": "tokenVault0",
            "type": "publicKey"
          },
          {
            "name": "tokenVault1",
            "type": "publicKey"
          },
          {
            "name": "fee",
            "type": "u32"
          },
          {
            "name": "tickSpacing",
            "type": "u16"
          },
          {
            "name": "liquidity",
            "type": "u64"
          },
          {
            "name": "sqrtPrice",
            "type": "u64"
          },
          {
            "name": "tick",
            "type": "i32"
          },
          {
            "name": "observationIndex",
            "type": "u16"
          },
          {
            "name": "observationCardinality",
            "type": "u16"
          },
          {
            "name": "observationCardinalityNext",
            "type": "u16"
          },
          {
            "name": "feeGrowthGlobal0",
            "type": "u64"
          },
          {
            "name": "feeGrowthGlobal1",
            "type": "u64"
          },
          {
            "name": "protocolFeesToken0",
            "type": "u64"
          },
          {
            "name": "protocolFeesToken1",
            "type": "u64"
          }
        ]
      }
    },
    {
      "name": "procotolPositionState",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "bump",
            "type": "u8"
          },
          {
            "name": "liquidity",
            "type": "u64"
          },
          {
            "name": "feeGrowthInside0Last",
            "type": "u64"
          },
          {
            "name": "feeGrowthInside1Last",
            "type": "u64"
          },
          {
            "name": "tokensOwed0",
            "type": "u64"
          },
          {
            "name": "tokensOwed1",
            "type": "u64"
          }
        ]
      }
    },
    {
      "name": "tickBitmapState",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "bump",
            "type": "u8"
          },
          {
            "name": "wordPos",
            "type": "i16"
          },
          {
            "name": "word",
            "type": {
              "array": [
                "u64",
                4
              ]
            }
          }
        ]
      }
    },
    {
      "name": "tickState",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "bump",
            "type": "u8"
          },
          {
            "name": "tick",
            "type": "i32"
          },
          {
            "name": "liquidityNet",
            "type": "i64"
          },
          {
            "name": "liquidityGross",
            "type": "u64"
          },
          {
            "name": "feeGrowthOutside0X32",
            "type": "u64"
          },
          {
            "name": "feeGrowthOutside1X32",
            "type": "u64"
          },
          {
            "name": "tickCumulativeOutside",
            "type": "i64"
          },
          {
            "name": "secondsPerLiquidityOutsideX32",
            "type": "u64"
          },
          {
            "name": "secondsOutside",
            "type": "u32"
          }
        ]
      }
    },
    {
      "name": "personalPositionState",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "bump",
            "type": "u8"
          },
          {
            "name": "mint",
            "type": "publicKey"
          },
          {
            "name": "poolId",
            "type": "publicKey"
          },
          {
            "name": "tickLower",
            "type": "i32"
          },
          {
            "name": "tickUpper",
            "type": "i32"
          },
          {
            "name": "liquidity",
            "type": "u64"
          },
          {
            "name": "feeGrowthInside0Last",
            "type": "u64"
          },
          {
            "name": "feeGrowthInside1Last",
            "type": "u64"
          },
          {
            "name": "tokensOwed0",
            "type": "u64"
          },
          {
            "name": "tokensOwed1",
            "type": "u64"
          }
        ]
      }
    }
  ],
  "events": [
    {
      "name": "CreateConfigEvent",
      "fields": [
        {
          "name": "owner",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "protocolFee",
          "type": "u8",
          "index": false
        }
      ]
    },
    {
      "name": "OwnerChangedEvent",
      "fields": [
        {
          "name": "oldOwner",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "newOwner",
          "type": "publicKey",
          "index": false
        }
      ]
    },
    {
      "name": "SetProtocolFeeRateEvent",
      "fields": [
        {
          "name": "feeProtocolOld",
          "type": "u8",
          "index": false
        },
        {
          "name": "feeProtocol",
          "type": "u8",
          "index": false
        }
      ]
    },
    {
      "name": "CreateFeeAccountEvent",
      "fields": [
        {
          "name": "fee",
          "type": "u32",
          "index": false
        },
        {
          "name": "tickSpacing",
          "type": "u16",
          "index": false
        }
      ]
    },
    {
      "name": "IncreaseObservationCardinalityNext",
      "fields": [
        {
          "name": "observationCardinalityNextOld",
          "type": "u16",
          "index": false
        },
        {
          "name": "observationCardinalityNextNew",
          "type": "u16",
          "index": false
        }
      ]
    },
    {
      "name": "PoolCreatedEvent",
      "fields": [
        {
          "name": "tokenMint0",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "tokenMint1",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "fee",
          "type": "u32",
          "index": false
        },
        {
          "name": "tickSpacing",
          "type": "u16",
          "index": false
        },
        {
          "name": "poolState",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "sqrtPrice",
          "type": "u64",
          "index": false
        },
        {
          "name": "tick",
          "type": "i32",
          "index": false
        },
        {
          "name": "tokenVault0",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "tokenVault1",
          "type": "publicKey",
          "index": false
        }
      ]
    },
    {
      "name": "CollectProtocolFeeEvent",
      "fields": [
        {
          "name": "poolState",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "recipientTokenAccount0",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "recipientTokenAccount1",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "amount0",
          "type": "u64",
          "index": false
        },
        {
          "name": "amount1",
          "type": "u64",
          "index": false
        }
      ]
    },
    {
      "name": "SwapEvent",
      "fields": [
        {
          "name": "poolState",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "sender",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "tokenAccount0",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "tokenAccount1",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "amount0",
          "type": "i64",
          "index": false
        },
        {
          "name": "amount1",
          "type": "i64",
          "index": false
        },
        {
          "name": "sqrtPriceX32",
          "type": "u64",
          "index": false
        },
        {
          "name": "liquidity",
          "type": "u64",
          "index": false
        },
        {
          "name": "tick",
          "type": "i32",
          "index": false
        }
      ]
    },
    {
      "name": "CreatePersonalPositionEvent",
      "fields": [
        {
          "name": "poolState",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "minter",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "nftOwner",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "tickLower",
          "type": "i32",
          "index": false
        },
        {
          "name": "tickUpper",
          "type": "i32",
          "index": false
        },
        {
          "name": "liquidity",
          "type": "u64",
          "index": false
        },
        {
          "name": "depositAmount0",
          "type": "u64",
          "index": false
        },
        {
          "name": "depositAmount1",
          "type": "u64",
          "index": false
        }
      ]
    },
    {
      "name": "BurnEvent",
      "fields": [
        {
          "name": "poolState",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "owner",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "tickLower",
          "type": "i32",
          "index": false
        },
        {
          "name": "tickUpper",
          "type": "i32",
          "index": false
        },
        {
          "name": "amount",
          "type": "u64",
          "index": false
        },
        {
          "name": "amount0",
          "type": "u64",
          "index": false
        },
        {
          "name": "amount1",
          "type": "u64",
          "index": false
        }
      ]
    },
    {
      "name": "CollectFeeEvent",
      "fields": [
        {
          "name": "poolState",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "owner",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "tickLower",
          "type": "i32",
          "index": false
        },
        {
          "name": "tickUpper",
          "type": "i32",
          "index": false
        },
        {
          "name": "collectAmount0",
          "type": "u64",
          "index": false
        },
        {
          "name": "collectAmount1",
          "type": "u64",
          "index": false
        }
      ]
    },
    {
      "name": "IncreaseLiquidityEvent",
      "fields": [
        {
          "name": "positionNftMint",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "liquidity",
          "type": "u64",
          "index": false
        },
        {
          "name": "amount0",
          "type": "u64",
          "index": false
        },
        {
          "name": "amount1",
          "type": "u64",
          "index": false
        }
      ]
    },
    {
      "name": "DecreaseLiquidityEvent",
      "fields": [
        {
          "name": "positionNftMint",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "liquidity",
          "type": "u64",
          "index": false
        },
        {
          "name": "amount0",
          "type": "u64",
          "index": false
        },
        {
          "name": "amount1",
          "type": "u64",
          "index": false
        }
      ]
    },
    {
      "name": "CollectPersonalFeeEvent",
      "fields": [
        {
          "name": "positionNftMint",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "recipientTokenAccount0",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "recipientTokenAccount1",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "amount0",
          "type": "u64",
          "index": false
        },
        {
          "name": "amount1",
          "type": "u64",
          "index": false
        }
      ]
    }
  ],
  "errors": [
    {
      "code": 6000,
      "name": "LOK",
      "msg": "LOK"
    },
    {
      "code": 6001,
      "name": "ZeroMintAmount",
      "msg": "Minting amount should be greater than 0"
    },
    {
      "code": 6002,
      "name": "TickInvaildOrder",
      "msg": "The lower tick must be below the upper tick"
    },
    {
      "code": 6003,
      "name": "TickLowerOverflow",
      "msg": "The tick must be greater, or equal to the minimum tick(-221818)"
    },
    {
      "code": 6004,
      "name": "TickUpperOverflow",
      "msg": "The tick must be lesser than, or equal to the maximum tick(221818)"
    },
    {
      "code": 6005,
      "name": "TickAndSpacingNotMatch",
      "msg": "tick % tick_spacing must be zero"
    },
    {
      "code": 6006,
      "name": "SqrtPriceLimitOverflow",
      "msg": "Square root price limit overflow"
    },
    {
      "code": 6007,
      "name": "SqrtPriceX32",
      "msg": "sqrt_price_x32 out of range"
    },
    {
      "code": 6008,
      "name": "LiquiditySubValueErr",
      "msg": "Liquidity sub delta L must be smaller than before"
    },
    {
      "code": 6009,
      "name": "LiquidityAddValueErr",
      "msg": "Liquidity add delta L must be greater, or equal to before"
    },
    {
      "code": 6010,
      "name": "TransactionTooOld",
      "msg": "Transaction too old"
    },
    {
      "code": 6011,
      "name": "PriceSlippageCheck",
      "msg": "Price slippage check"
    },
    {
      "code": 6012,
      "name": "NotApproved",
      "msg": "Not approved"
    },
    {
      "code": 6013,
      "name": "TooLittleReceived",
      "msg": "Too little received"
    },
    {
      "code": 6014,
      "name": "InvaildSwapAmountSpecified",
      "msg": "Swap special amount can not be zero"
    },
    {
      "code": 6015,
      "name": "InvaildTickIndex",
      "msg": "Tick index of lower must be smaller than upper"
    },
    {
      "code": 6016,
      "name": "InvaildLiquidity",
      "msg": "Invaild liquidity when update position"
    }
  ]
};

export const IDL: AmmCore = {
  "version": "0.1.0",
  "name": "amm_core",
  "instructions": [
    {
      "name": "createAmmConfig",
      "accounts": [
        {
          "name": "owner",
          "isMut": true,
          "isSigner": true
        },
        {
          "name": "ammConfig",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "systemProgram",
          "isMut": false,
          "isSigner": false
        }
      ],
      "args": []
    },
    {
      "name": "setNewOwner",
      "accounts": [
        {
          "name": "owner",
          "isMut": false,
          "isSigner": true
        },
        {
          "name": "newOwner",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "ammConfig",
          "isMut": true,
          "isSigner": false
        }
      ],
      "args": []
    },
    {
      "name": "createFeeAccount",
      "accounts": [
        {
          "name": "owner",
          "isMut": true,
          "isSigner": true
        },
        {
          "name": "ammConfig",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "feeState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "systemProgram",
          "isMut": false,
          "isSigner": false
        }
      ],
      "args": [
        {
          "name": "fee",
          "type": "u32"
        },
        {
          "name": "tickSpacing",
          "type": "u16"
        }
      ]
    },
    {
      "name": "createPool",
      "accounts": [
        {
          "name": "poolCreator",
          "isMut": true,
          "isSigner": true
        },
        {
          "name": "poolState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tokenMint0",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "tokenMint1",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "tokenVault0",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tokenVault1",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "feeState",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "initialObservationState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tokenProgram",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "systemProgram",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "rent",
          "isMut": false,
          "isSigner": false
        }
      ],
      "args": [
        {
          "name": "sqrtPrice",
          "type": "u64"
        }
      ]
    },
    {
      "name": "increaseObservationCardinalityNext",
      "accounts": [
        {
          "name": "payer",
          "isMut": false,
          "isSigner": true
        },
        {
          "name": "poolState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "systemProgram",
          "isMut": false,
          "isSigner": false
        }
      ],
      "args": [
        {
          "name": "observationAccountBumps",
          "type": "bytes"
        }
      ]
    },
    {
      "name": "setProtocolFeeRate",
      "accounts": [
        {
          "name": "owner",
          "isMut": false,
          "isSigner": true
        },
        {
          "name": "ammConfig",
          "isMut": true,
          "isSigner": false
        }
      ],
      "args": [
        {
          "name": "feeProtocol",
          "type": "u8"
        }
      ]
    },
    {
      "name": "collectProtocolFee",
      "accounts": [
        {
          "name": "owner",
          "isMut": false,
          "isSigner": true
        },
        {
          "name": "ammConfig",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "poolState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tokenVault0",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tokenVault1",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "recipientTokenAccount0",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "recipientTokenAccount1",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tokenProgram",
          "isMut": false,
          "isSigner": false
        }
      ],
      "args": [
        {
          "name": "amount0Requested",
          "type": "u64"
        },
        {
          "name": "amount1Requested",
          "type": "u64"
        }
      ]
    },
    {
      "name": "createTickAccount",
      "accounts": [
        {
          "name": "signer",
          "isMut": true,
          "isSigner": true
        },
        {
          "name": "poolState",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "tickState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "systemProgram",
          "isMut": false,
          "isSigner": false
        }
      ],
      "args": [
        {
          "name": "tick",
          "type": "i32"
        }
      ]
    },
    {
      "name": "createBitmapAccount",
      "accounts": [
        {
          "name": "signer",
          "isMut": true,
          "isSigner": true
        },
        {
          "name": "poolState",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "bitmapState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "systemProgram",
          "isMut": false,
          "isSigner": false
        }
      ],
      "args": [
        {
          "name": "wordPos",
          "type": "i16"
        }
      ]
    },
    {
      "name": "createProcotolPosition",
      "accounts": [
        {
          "name": "signer",
          "isMut": true,
          "isSigner": true
        },
        {
          "name": "ammConfig",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "poolState",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "tickLowerState",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "tickUpperState",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "positionState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "systemProgram",
          "isMut": false,
          "isSigner": false
        }
      ],
      "args": []
    },
    {
      "name": "createPersonalPosition",
      "accounts": [
        {
          "name": "minter",
          "isMut": true,
          "isSigner": true
        },
        {
          "name": "positionNftOwner",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "ammConfig",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "positionNftMint",
          "isMut": true,
          "isSigner": true
        },
        {
          "name": "positionNftAccount",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "poolState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "protocolPositionState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tickLowerState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tickUpperState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "bitmapLowerState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "bitmapUpperState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "personalPositionState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tokenAccount0",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tokenAccount1",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tokenVault0",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tokenVault1",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "lastObservationState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "rent",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "systemProgram",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "tokenProgram",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "associatedTokenProgram",
          "isMut": false,
          "isSigner": false
        }
      ],
      "args": [
        {
          "name": "amount0Desired",
          "type": "u64"
        },
        {
          "name": "amount1Desired",
          "type": "u64"
        },
        {
          "name": "amount0Min",
          "type": "u64"
        },
        {
          "name": "amount1Min",
          "type": "u64"
        }
      ]
    },
    {
      "name": "personalPositionWithMetadata",
      "accounts": [
        {
          "name": "payer",
          "isMut": true,
          "isSigner": true
        },
        {
          "name": "ammConfig",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "nftMint",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "positionState",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "metadataAccount",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "rent",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "metadataProgram",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "tokenProgram",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "systemProgram",
          "isMut": false,
          "isSigner": false
        }
      ],
      "args": []
    },
    {
      "name": "increaseLiquidity",
      "accounts": [
        {
          "name": "payer",
          "isMut": false,
          "isSigner": true
        },
        {
          "name": "ammConfig",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "personalPositionState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "poolState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "protocolPositionState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tickLowerState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tickUpperState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "bitmapLowerState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "bitmapUpperState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tokenAccount0",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tokenAccount1",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tokenVault0",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tokenVault1",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "lastObservationState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tokenProgram",
          "isMut": false,
          "isSigner": false
        }
      ],
      "args": [
        {
          "name": "amount0Desired",
          "type": "u64"
        },
        {
          "name": "amount1Desired",
          "type": "u64"
        },
        {
          "name": "amount0Min",
          "type": "u64"
        },
        {
          "name": "amount1Min",
          "type": "u64"
        }
      ]
    },
    {
      "name": "decreaseLiquidity",
      "accounts": [
        {
          "name": "ownerOrDelegate",
          "isMut": false,
          "isSigner": true
        },
        {
          "name": "nftAccount",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "personalPositionState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "ammConfig",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "poolState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "protocolPositionState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tickLowerState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tickUpperState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "bitmapLowerState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "bitmapUpperState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "lastObservationState",
          "isMut": true,
          "isSigner": false
        }
      ],
      "args": [
        {
          "name": "liquidity",
          "type": "u64"
        },
        {
          "name": "amount0Min",
          "type": "u64"
        },
        {
          "name": "amount1Min",
          "type": "u64"
        }
      ]
    },
    {
      "name": "collectFee",
      "accounts": [
        {
          "name": "ownerOrDelegate",
          "isMut": false,
          "isSigner": true
        },
        {
          "name": "nftAccount",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "personalPositionState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "ammConfig",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "poolState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "protocolPositionState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tickLowerState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tickUpperState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "bitmapLowerState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "bitmapUpperState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "lastObservationState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tokenVault0",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tokenVault1",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "recipientTokenAccount0",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "recipientTokenAccount1",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tokenProgram",
          "isMut": false,
          "isSigner": false
        }
      ],
      "args": [
        {
          "name": "amount0Max",
          "type": "u64"
        },
        {
          "name": "amount1Max",
          "type": "u64"
        }
      ]
    },
    {
      "name": "swapBaseInSingle",
      "accounts": [
        {
          "name": "signer",
          "isMut": false,
          "isSigner": true
        },
        {
          "name": "ammConfig",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "poolState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "inputTokenAccount",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "outputTokenAccount",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "inputVault",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "outputVault",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "lastObservationState",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tokenProgram",
          "isMut": false,
          "isSigner": false
        }
      ],
      "args": [
        {
          "name": "amountIn",
          "type": "u64"
        },
        {
          "name": "amountOutMinimum",
          "type": "u64"
        },
        {
          "name": "sqrtPriceLimit",
          "type": "u64"
        }
      ]
    },
    {
      "name": "swapBaseIn",
      "accounts": [
        {
          "name": "signer",
          "isMut": false,
          "isSigner": true
        },
        {
          "name": "ammConfig",
          "isMut": false,
          "isSigner": false
        },
        {
          "name": "inputTokenAccount",
          "isMut": true,
          "isSigner": false
        },
        {
          "name": "tokenProgram",
          "isMut": false,
          "isSigner": false
        }
      ],
      "args": [
        {
          "name": "amountIn",
          "type": "u64"
        },
        {
          "name": "amountOutMinimum",
          "type": "u64"
        },
        {
          "name": "additionalAccountsPerPool",
          "type": "bytes"
        }
      ]
    }
  ],
  "accounts": [
    {
      "name": "ammConfig",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "bump",
            "type": "u8"
          },
          {
            "name": "owner",
            "type": "publicKey"
          },
          {
            "name": "protocolFee",
            "type": "u8"
          }
        ]
      }
    },
    {
      "name": "feeState",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "bump",
            "type": "u8"
          },
          {
            "name": "fee",
            "type": "u32"
          },
          {
            "name": "tickSpacing",
            "type": "u16"
          }
        ]
      }
    },
    {
      "name": "observationState",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "bump",
            "type": "u8"
          },
          {
            "name": "index",
            "type": "u16"
          },
          {
            "name": "blockTimestamp",
            "type": "u32"
          },
          {
            "name": "tickCumulative",
            "type": "i64"
          },
          {
            "name": "secondsPerLiquidityCumulativeX32",
            "type": "u64"
          },
          {
            "name": "initialized",
            "type": "bool"
          }
        ]
      }
    },
    {
      "name": "poolState",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "bump",
            "type": "u8"
          },
          {
            "name": "tokenMint0",
            "type": "publicKey"
          },
          {
            "name": "tokenMint1",
            "type": "publicKey"
          },
          {
            "name": "tokenVault0",
            "type": "publicKey"
          },
          {
            "name": "tokenVault1",
            "type": "publicKey"
          },
          {
            "name": "fee",
            "type": "u32"
          },
          {
            "name": "tickSpacing",
            "type": "u16"
          },
          {
            "name": "liquidity",
            "type": "u64"
          },
          {
            "name": "sqrtPrice",
            "type": "u64"
          },
          {
            "name": "tick",
            "type": "i32"
          },
          {
            "name": "observationIndex",
            "type": "u16"
          },
          {
            "name": "observationCardinality",
            "type": "u16"
          },
          {
            "name": "observationCardinalityNext",
            "type": "u16"
          },
          {
            "name": "feeGrowthGlobal0",
            "type": "u64"
          },
          {
            "name": "feeGrowthGlobal1",
            "type": "u64"
          },
          {
            "name": "protocolFeesToken0",
            "type": "u64"
          },
          {
            "name": "protocolFeesToken1",
            "type": "u64"
          }
        ]
      }
    },
    {
      "name": "procotolPositionState",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "bump",
            "type": "u8"
          },
          {
            "name": "liquidity",
            "type": "u64"
          },
          {
            "name": "feeGrowthInside0Last",
            "type": "u64"
          },
          {
            "name": "feeGrowthInside1Last",
            "type": "u64"
          },
          {
            "name": "tokensOwed0",
            "type": "u64"
          },
          {
            "name": "tokensOwed1",
            "type": "u64"
          }
        ]
      }
    },
    {
      "name": "tickBitmapState",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "bump",
            "type": "u8"
          },
          {
            "name": "wordPos",
            "type": "i16"
          },
          {
            "name": "word",
            "type": {
              "array": [
                "u64",
                4
              ]
            }
          }
        ]
      }
    },
    {
      "name": "tickState",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "bump",
            "type": "u8"
          },
          {
            "name": "tick",
            "type": "i32"
          },
          {
            "name": "liquidityNet",
            "type": "i64"
          },
          {
            "name": "liquidityGross",
            "type": "u64"
          },
          {
            "name": "feeGrowthOutside0X32",
            "type": "u64"
          },
          {
            "name": "feeGrowthOutside1X32",
            "type": "u64"
          },
          {
            "name": "tickCumulativeOutside",
            "type": "i64"
          },
          {
            "name": "secondsPerLiquidityOutsideX32",
            "type": "u64"
          },
          {
            "name": "secondsOutside",
            "type": "u32"
          }
        ]
      }
    },
    {
      "name": "personalPositionState",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "bump",
            "type": "u8"
          },
          {
            "name": "mint",
            "type": "publicKey"
          },
          {
            "name": "poolId",
            "type": "publicKey"
          },
          {
            "name": "tickLower",
            "type": "i32"
          },
          {
            "name": "tickUpper",
            "type": "i32"
          },
          {
            "name": "liquidity",
            "type": "u64"
          },
          {
            "name": "feeGrowthInside0Last",
            "type": "u64"
          },
          {
            "name": "feeGrowthInside1Last",
            "type": "u64"
          },
          {
            "name": "tokensOwed0",
            "type": "u64"
          },
          {
            "name": "tokensOwed1",
            "type": "u64"
          }
        ]
      }
    }
  ],
  "events": [
    {
      "name": "CreateConfigEvent",
      "fields": [
        {
          "name": "owner",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "protocolFee",
          "type": "u8",
          "index": false
        }
      ]
    },
    {
      "name": "OwnerChangedEvent",
      "fields": [
        {
          "name": "oldOwner",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "newOwner",
          "type": "publicKey",
          "index": false
        }
      ]
    },
    {
      "name": "SetProtocolFeeRateEvent",
      "fields": [
        {
          "name": "feeProtocolOld",
          "type": "u8",
          "index": false
        },
        {
          "name": "feeProtocol",
          "type": "u8",
          "index": false
        }
      ]
    },
    {
      "name": "CreateFeeAccountEvent",
      "fields": [
        {
          "name": "fee",
          "type": "u32",
          "index": false
        },
        {
          "name": "tickSpacing",
          "type": "u16",
          "index": false
        }
      ]
    },
    {
      "name": "IncreaseObservationCardinalityNext",
      "fields": [
        {
          "name": "observationCardinalityNextOld",
          "type": "u16",
          "index": false
        },
        {
          "name": "observationCardinalityNextNew",
          "type": "u16",
          "index": false
        }
      ]
    },
    {
      "name": "PoolCreatedEvent",
      "fields": [
        {
          "name": "tokenMint0",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "tokenMint1",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "fee",
          "type": "u32",
          "index": false
        },
        {
          "name": "tickSpacing",
          "type": "u16",
          "index": false
        },
        {
          "name": "poolState",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "sqrtPrice",
          "type": "u64",
          "index": false
        },
        {
          "name": "tick",
          "type": "i32",
          "index": false
        },
        {
          "name": "tokenVault0",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "tokenVault1",
          "type": "publicKey",
          "index": false
        }
      ]
    },
    {
      "name": "CollectProtocolFeeEvent",
      "fields": [
        {
          "name": "poolState",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "recipientTokenAccount0",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "recipientTokenAccount1",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "amount0",
          "type": "u64",
          "index": false
        },
        {
          "name": "amount1",
          "type": "u64",
          "index": false
        }
      ]
    },
    {
      "name": "SwapEvent",
      "fields": [
        {
          "name": "poolState",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "sender",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "tokenAccount0",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "tokenAccount1",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "amount0",
          "type": "i64",
          "index": false
        },
        {
          "name": "amount1",
          "type": "i64",
          "index": false
        },
        {
          "name": "sqrtPriceX32",
          "type": "u64",
          "index": false
        },
        {
          "name": "liquidity",
          "type": "u64",
          "index": false
        },
        {
          "name": "tick",
          "type": "i32",
          "index": false
        }
      ]
    },
    {
      "name": "CreatePersonalPositionEvent",
      "fields": [
        {
          "name": "poolState",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "minter",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "nftOwner",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "tickLower",
          "type": "i32",
          "index": false
        },
        {
          "name": "tickUpper",
          "type": "i32",
          "index": false
        },
        {
          "name": "liquidity",
          "type": "u64",
          "index": false
        },
        {
          "name": "depositAmount0",
          "type": "u64",
          "index": false
        },
        {
          "name": "depositAmount1",
          "type": "u64",
          "index": false
        }
      ]
    },
    {
      "name": "BurnEvent",
      "fields": [
        {
          "name": "poolState",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "owner",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "tickLower",
          "type": "i32",
          "index": false
        },
        {
          "name": "tickUpper",
          "type": "i32",
          "index": false
        },
        {
          "name": "amount",
          "type": "u64",
          "index": false
        },
        {
          "name": "amount0",
          "type": "u64",
          "index": false
        },
        {
          "name": "amount1",
          "type": "u64",
          "index": false
        }
      ]
    },
    {
      "name": "CollectFeeEvent",
      "fields": [
        {
          "name": "poolState",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "owner",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "tickLower",
          "type": "i32",
          "index": false
        },
        {
          "name": "tickUpper",
          "type": "i32",
          "index": false
        },
        {
          "name": "collectAmount0",
          "type": "u64",
          "index": false
        },
        {
          "name": "collectAmount1",
          "type": "u64",
          "index": false
        }
      ]
    },
    {
      "name": "IncreaseLiquidityEvent",
      "fields": [
        {
          "name": "positionNftMint",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "liquidity",
          "type": "u64",
          "index": false
        },
        {
          "name": "amount0",
          "type": "u64",
          "index": false
        },
        {
          "name": "amount1",
          "type": "u64",
          "index": false
        }
      ]
    },
    {
      "name": "DecreaseLiquidityEvent",
      "fields": [
        {
          "name": "positionNftMint",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "liquidity",
          "type": "u64",
          "index": false
        },
        {
          "name": "amount0",
          "type": "u64",
          "index": false
        },
        {
          "name": "amount1",
          "type": "u64",
          "index": false
        }
      ]
    },
    {
      "name": "CollectPersonalFeeEvent",
      "fields": [
        {
          "name": "positionNftMint",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "recipientTokenAccount0",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "recipientTokenAccount1",
          "type": "publicKey",
          "index": false
        },
        {
          "name": "amount0",
          "type": "u64",
          "index": false
        },
        {
          "name": "amount1",
          "type": "u64",
          "index": false
        }
      ]
    }
  ],
  "errors": [
    {
      "code": 6000,
      "name": "LOK",
      "msg": "LOK"
    },
    {
      "code": 6001,
      "name": "ZeroMintAmount",
      "msg": "Minting amount should be greater than 0"
    },
    {
      "code": 6002,
      "name": "TickInvaildOrder",
      "msg": "The lower tick must be below the upper tick"
    },
    {
      "code": 6003,
      "name": "TickLowerOverflow",
      "msg": "The tick must be greater, or equal to the minimum tick(-221818)"
    },
    {
      "code": 6004,
      "name": "TickUpperOverflow",
      "msg": "The tick must be lesser than, or equal to the maximum tick(221818)"
    },
    {
      "code": 6005,
      "name": "TickAndSpacingNotMatch",
      "msg": "tick % tick_spacing must be zero"
    },
    {
      "code": 6006,
      "name": "SqrtPriceLimitOverflow",
      "msg": "Square root price limit overflow"
    },
    {
      "code": 6007,
      "name": "SqrtPriceX32",
      "msg": "sqrt_price_x32 out of range"
    },
    {
      "code": 6008,
      "name": "LiquiditySubValueErr",
      "msg": "Liquidity sub delta L must be smaller than before"
    },
    {
      "code": 6009,
      "name": "LiquidityAddValueErr",
      "msg": "Liquidity add delta L must be greater, or equal to before"
    },
    {
      "code": 6010,
      "name": "TransactionTooOld",
      "msg": "Transaction too old"
    },
    {
      "code": 6011,
      "name": "PriceSlippageCheck",
      "msg": "Price slippage check"
    },
    {
      "code": 6012,
      "name": "NotApproved",
      "msg": "Not approved"
    },
    {
      "code": 6013,
      "name": "TooLittleReceived",
      "msg": "Too little received"
    },
    {
      "code": 6014,
      "name": "InvaildSwapAmountSpecified",
      "msg": "Swap special amount can not be zero"
    },
    {
      "code": 6015,
      "name": "InvaildTickIndex",
      "msg": "Tick index of lower must be smaller than upper"
    },
    {
      "code": 6016,
      "name": "InvaildLiquidity",
      "msg": "Invaild liquidity when update position"
    }
  ]
};
