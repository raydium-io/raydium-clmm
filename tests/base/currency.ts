import {web3} from "@project-serum/anchor"

export class Currency {
    public readonly address: web3.PublicKey;

    public readonly is_native: boolean;
    public readonly decimals: number;

    constructor(mint:  web3.PublicKey, decimals: number, symbol?: string, name?: string){
        this.address = mint
        this.decimals = decimals
    }

    public equals(other: Currency): boolean{
        return this.address.equals(other.address)
    }
}