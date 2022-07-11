import { Currency } from "@raydium-io/raydium-sdk";

export class Price{
    readonly baseCurrency: Currency;
    readonly quoteCurrency: Currency;
    public constructor(
        baseCurrency: Currency,
        quoteCurrency: Currency,
      ) {
        this.address = address;
        this.decimals = decimals;
        this.symbol = symbol;
        this.name = name;
      }


}