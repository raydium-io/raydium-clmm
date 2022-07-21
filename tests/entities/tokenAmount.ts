
import { BN } from '@project-serum/anchor';
import { Token } from '@raydium-io/raydium-sdk';

export class TokenAmount{
    public readonly currency: Token
    public readonly amount: BN

    public constructor(
        token: Token,
        amount:  number | string | BN,
      ) {
        this.currency = token;
        this.amount = new BN(amount);
      }
}