use cosmwasm_std::{
    HumanAddr, Uint128, BankQuery, BalanceResponse,
    Extern, Storage, Api, Querier, StdResult,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct Contract {
    pub address: HumanAddr,
    pub code_hash: String,
}

pub fn scrt_balance<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    address: HumanAddr,
) -> StdResult<Uint128> {

    let resp: BalanceResponse = deps.querier.query(
        &BankQuery::Balance {
            address,
            denom: "uscrt".to_string(),
        }
        .into(),
    )?;

    Ok(resp.amount.amount)
}

