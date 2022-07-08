
import { Token } from '@raydium-io/raydium-sdk';
import JSBI from 'jsbi'
// import { Currency } from '../base';

export class TokenAmount{
    public readonly currency: Token
    public readonly amount: JSBI

    public constructor(
        token: Token,
        amount:  number | string | JSBI,
      ) {
        this.currency = token;
        this.amount = JSBI.BigInt(amount);
      }
}