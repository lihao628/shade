use crate::utils::{asset::Contract, generic_response::ResponseStatus};
use crate::c_std::{Binary, Decimal, Delegation, Addr, Uint128, Validator};

use crate::contract_interfaces::dao::adapter;

use crate::utils::{ExecuteCallback, InstantiateCallback, Query};
use cosmwasm_schema::{cw_serde};

#[cw_serde]
pub struct Config {
    pub admins: Vec<Addr>,
    //pub treasury: Addr,
    // This is the contract that will "unbond" funds
    pub owner: Addr,
    pub sscrt: Contract,
    pub validator_bounds: Option<ValidatorBounds>,
}

#[cw_serde]
pub struct ValidatorBounds {
    pub min_commission: Decimal,
    pub max_commission: Decimal,
    pub top_position: Uint128,
    pub bottom_position: Uint128,
}

#[cw_serde]
pub struct InstantiateMsg {
    pub admins: Option<Vec<Addr>>,
    pub owner: Addr,
    pub sscrt: Contract,
    pub validator_bounds: Option<ValidatorBounds>,
    pub viewing_key: String,
}

impl InstantiateCallback for InstantiateMsg {
    const BLOCK_SIZE: usize = 256;
}

#[cw_serde]
pub enum ExecuteMsg {
    Receive {
        sender: Addr,
        from: Addr,
        amount: Uint128,
        memo: Option<Binary>,
        msg: Option<Binary>,
    },
    UpdateConfig {
        config: Config,
    },
    Adapter(adapter::SubHandleMsg),
}

impl ExecuteCallback for ExecuteMsg {
    const BLOCK_SIZE: usize = 256;
}

#[cw_serde]
pub enum HandleAnswer {
    Init {
        status: ResponseStatus,
        address: Addr,
    },
    UpdateConfig {
        status: ResponseStatus,
    },
    Receive {
        status: ResponseStatus,
        validator: Validator,
    },
    /*
    Claim {
        status: ResponseStatus,
    },
    Unbond {
        status: ResponseStatus,
        delegations: Vec<Addr>,
    },
    */
}

#[cw_serde]
pub enum QueryMsg {
    Config {},
    Delegations {},
    Rewards {},
    Adapter(adapter::SubQueryMsg),
}

impl Query for QueryMsg {
    const BLOCK_SIZE: usize = 256;
}

#[cw_serde]
pub enum QueryAnswer {
    Config { config: Config },
    //Balance { amount: Uint128 },
}
