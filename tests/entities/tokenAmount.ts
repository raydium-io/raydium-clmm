
import { Token } from '@cykura/sdk-core'
import JSBI from 'jsbi'

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