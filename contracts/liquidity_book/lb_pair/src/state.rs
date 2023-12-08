use shade_protocol::{
    c_std::{Addr, ContractInfo, Storage, Timestamp, Uint128, Uint256},
    cosmwasm_schema::cw_serde,
    lb_libraries::{
        math::tree_math::TreeUint24,
        oracle_helper::Oracle,
        pair_parameter_helper::PairParameters,
        types::{Bytes32, ContractInstantiationInfo},
        viewing_keys::ViewingKey,
    },
    liquidity_book::lb_pair::{RewardsDistribution, RewardsDistributionAlgorithm},
    secret_storage_plus::{AppendStore, Bincode2, Item, Map},
    storage::{singleton, singleton_read, ReadonlySingleton, Singleton},
    swap::core::TokenType,
    Contract,
};

pub const CONFIG: Item<State, Bincode2> = Item::new("config");
pub const CONTRACT_STATUS: Item<ContractStatus, Bincode2> = Item::new("contract_status");
pub const BIN_MAP: Map<u32, Bytes32> = Map::new("bins"); //?
pub const BIN_TREE: Item<TreeUint24, Bincode2> = Item::new("bin_tree"); //?
pub const ORACLE: Item<Oracle, Bincode2> = Item::new("oracle"); //?
pub static EPHEMERAL_STORAGE_KEY: &[u8] = b"ephemeral_storage";
pub const FEE_APPEND_STORE: AppendStore<FeeLog, Bincode2> = AppendStore::new("fee_logs"); //?
pub const REWARDS_STATS_STORE: Map<u64, RewardStats> = Map::new("rewards_stats"); //?
pub const REWARDS_DISTRIBUTION: Map<u64, RewardsDistribution> = Map::new("rewards_distribution"); //?
pub const FEE_MAP_TREE: Map<u64, TreeUint24, Bincode2> = Map::new("fee_tree"); //?
pub const FEE_MAP: Map<u32, Uint256> = Map::new("fee_map"); //?

pub const STAKING_CONTRACT_IMPL: Item<ContractInstantiationInfo, Bincode2> =
    Item::new("staking_contract_impl");

#[cw_serde]
pub struct RewardStats {
    pub cumm_value: Uint256,
    pub cumm_value_mul_bin_id: Uint256,
    pub rewards_distribution_algorithm: RewardsDistributionAlgorithm,
}

#[cw_serde]
pub struct FeeLog {
    pub is_token_x: bool,
    pub fee: Uint128,
    pub bin_id: u32,
    pub timestamp: Timestamp,
    pub last_rewards_epoch_id: u64,
}

#[cw_serde]
pub enum ContractStatus {
    Active,         // allows all operations
    FreezeAll,      // blocks everything except admin-protected config changes
    LpWithdrawOnly, // blocks everything except LP withdraws and admin-protected config changes
}

#[cw_serde]
pub struct State {
    pub creator: Addr,
    pub factory: ContractInfo,
    pub token_x: TokenType,
    pub token_y: TokenType,
    pub bin_step: u16,
    pub viewing_key: ViewingKey,
    pub pair_parameters: PairParameters,
    pub reserves: Bytes32,
    pub protocol_fees: Bytes32,
    pub lb_token: ContractInfo,
    pub staking_contract: ContractInfo,
    pub protocol_fees_recipient: Addr,
    pub admin_auth: Contract,
    pub last_swap_timestamp: Timestamp,
    pub rewards_epoch_id: u64,
    pub base_rewards_bins: Option<u32>,
    pub toggle_distributions_algorithm: bool,
}

pub fn ephemeral_storage_w(storage: &mut dyn Storage) -> Singleton<NextTokenKey> {
    singleton(storage, EPHEMERAL_STORAGE_KEY)
}

pub fn ephemeral_storage_r(storage: &dyn Storage) -> ReadonlySingleton<NextTokenKey> {
    singleton_read(storage, EPHEMERAL_STORAGE_KEY)
}

#[cw_serde]
pub struct NextTokenKey {
    pub lb_token_code_hash: String,
    pub staking_contract: ContractInstantiationInfo,
    pub token_x_symbol: String,
    pub token_y_symbol: String,
}

// NOTE: These types are not used, since we are encoding the values into a U256.

// #[cw_serde]
// #[derive(Default)]
// pub struct PairParameters {
//     pub base_factor: u16,
//     pub filter_period: u16,
//     pub decay_period: u16,
//     pub reduction_factor: u16,
//     pub variable_fee_control: u32,
//     pub protocol_share: u16,
//     pub max_volatility_accumulator: u32,
//     pub volatility_accumulator: u32,
//     pub volatility_reference: u32,
//     pub index_reference: u32,
//     pub time_of_last_update: u64,
//     pub oracle_id: u16,
//     pub active_id: u32,
// }

// #[cw_serde]
// #[derive(Default)]
// pub struct Oracle {
//     pub oracle_length: u16,
//     pub cumulative_id: u64,
//     pub cumulative_volatility_accumulator: u64,
//     pub cumulative_bin_crossed: u64,
//     pub sample_lifetime: u8,
//     pub sample_creation_timestamp: u64,
// }
