use crate::multi::lb_pair::LbPair;
use shade_protocol::{
    c_std::{Addr, ContractInfo, StdError, StdResult, Uint128, Uint256},
    contract_interfaces::liquidity_book::lb_pair,
    lb_libraries::{
        tokens::TokenType,
        types::{ContractInstantiationInfo, StaticFeeParameters},
    },
    liquidity_book::lb_pair::{LiquidityParameters, RemoveLiquidity},
    multi_test::App,
    utils::{asset::Contract, ExecuteCallback, InstantiateCallback, MultiTestable, Query},
};

pub fn init(
    app: &mut App,
    sender: &str,
    factory_contract_info: ContractInfo,
    token_x: TokenType,
    token_y: TokenType,
    bin_step: u16,
    pair_parameters: StaticFeeParameters,
    active_id: u32,
    lb_token_implementation: ContractInstantiationInfo,
    viewing_key: String,
    pair_name: String,
    entropy: String,
) -> StdResult<Contract> {
    let lb_pair = Contract::from(
        match (lb_pair::InstantiateMsg {
            factory: factory_contract_info,
            token_x,
            token_y,
            bin_step,
            pair_parameters,
            active_id,
            lb_token_implementation,
            viewing_key,
            pair_name,
            entropy,
        }
        .test_init(
            LbPair::default(),
            app,
            Addr::unchecked(sender),
            "lb_pair",
            &[],
        )) {
            Ok(contract_info) => contract_info,
            Err(e) => return Err(StdError::generic_err(e.to_string())),
        },
    );
    Ok(lb_pair)
}

pub fn add_liquidity(
    app: &mut App,
    sender: &str,
    lb_pair: &ContractInfo,
    liquidity_parameters: LiquidityParameters,
) -> StdResult<()> {
    match (lb_pair::ExecuteMsg::AddLiquidity {
        liquidity_parameters,
    }
    .test_exec(lb_pair, app, Addr::unchecked(sender), &[]))
    {
        Ok(_) => Ok(()),
        Err(e) => return Err(StdError::generic_err(e.root_cause().to_string())),
    }
}

pub fn remove_liquidity(
    app: &mut App,
    sender: &str,
    lb_pair: &ContractInfo,
    liquidity_parameters: RemoveLiquidity,
) -> StdResult<()> {
    match (lb_pair::ExecuteMsg::RemoveLiquidity {
        remove_liquidity_params: liquidity_parameters,
    }
    .test_exec(lb_pair, app, Addr::unchecked(sender), &[]))
    {
        Ok(_) => Ok(()),
        Err(e) => return Err(StdError::generic_err(e.root_cause().to_string())),
    }
}

pub fn swap(
    app: &mut App,
    sender: &str,
    lb_pair: &ContractInfo,
    swap_for_y: bool,
    to: Addr,
    amount_received: Uint128,
) -> StdResult<()> {
    match (lb_pair::ExecuteMsg::Swap {
        swap_for_y,
        to,
        amount_received,
    }
    .test_exec(lb_pair, app, Addr::unchecked(sender), &[]))
    {
        Ok(_) => Ok(()),
        Err(e) => return Err(StdError::generic_err(e.root_cause().to_string())),
    }
}

pub fn lb_token_query(app: &App, lb_pair: &ContractInfo) -> StdResult<ContractInfo> {
    let res = lb_pair::QueryMsg::GetLbToken {}.test_query(lb_pair, app)?;
    let lb_pair::LbTokenResponse { lb_token } = res;
    Ok(lb_token)
}

pub fn bin_query(app: &App, lb_pair: &ContractInfo, id: u32) -> StdResult<(u128, u128)> {
    let res = lb_pair::QueryMsg::GetBin { id }.test_query(lb_pair, app)?;
    let lb_pair::BinResponse {
        bin_reserve_x,
        bin_reserve_y,
    } = res;
    Ok((bin_reserve_x, bin_reserve_y))
}

pub fn swap_in_query(
    app: &App,
    lb_pair: &ContractInfo,
    amount_out: Uint128,
    swap_for_y: bool,
) -> StdResult<(Uint128, Uint128, Uint128)> {
    let res = lb_pair::QueryMsg::GetSwapIn {
        amount_out,
        swap_for_y,
    }
    .test_query(lb_pair, app)?;
    let lb_pair::SwapInResponse {
        amount_in,
        amount_out_left,
        fee,
    } = res;
    Ok((amount_in, amount_out_left, fee))
}

pub fn swap_out_query(
    app: &App,
    lb_pair: &ContractInfo,
    amount_in: Uint128,
    swap_for_y: bool,
) -> StdResult<(Uint128, Uint128, Uint128)> {
    let res = lb_pair::QueryMsg::GetSwapOut {
        swap_for_y,
        amount_in,
    }
    .test_query(lb_pair, app)?;
    let lb_pair::SwapOutResponse {
        amount_out,
        amount_in_left,
        fee,
    } = res;
    Ok((amount_out, amount_in_left, fee))
}

pub fn query_static_fee_params(
    app: &App,
    lb_pair: &ContractInfo,
) -> StdResult<(u16, u16, u16, u16, u32, u16, u32)> {
    let res = lb_pair::QueryMsg::GetStaticFeeParameters {}.test_query(lb_pair, app)?;
    let lb_pair::StaticFeeParametersResponse {
        base_factor,
        filter_period,
        decay_period,
        reduction_factor,
        variable_fee_control,
        protocol_share,
        max_volatility_accumulator,
    } = res;
    Ok((
        base_factor,
        filter_period,
        decay_period,
        reduction_factor,
        variable_fee_control,
        protocol_share,
        max_volatility_accumulator,
    ))
}

pub fn query_variable_fee_params(
    app: &App,
    lb_pair: &ContractInfo,
) -> StdResult<(u32, u32, u32, u64)> {
    let res = lb_pair::QueryMsg::GetVariableFeeParameters {}.test_query(lb_pair, app)?;
    let lb_pair::VariableFeeParametersResponse {
        volatility_accumulator,
        volatility_reference,
        id_reference,
        time_of_last_update,
    } = res;
    Ok((
        volatility_accumulator,
        volatility_reference,
        id_reference,
        time_of_last_update,
    ))
}

pub fn query_factory(app: &App, lb_pair: &ContractInfo) -> StdResult<Addr> {
    let res = lb_pair::QueryMsg::GetFactory {}.test_query(lb_pair, app)?;
    let lb_pair::FactoryResponse { factory } = res;
    Ok(factory)
}

pub fn query_token_x(app: &App, lb_pair: &ContractInfo) -> StdResult<TokenType> {
    let res = lb_pair::QueryMsg::GetTokenX {}.test_query(lb_pair, app)?;
    let lb_pair::TokenXResponse { token_x } = res;
    Ok(token_x)
}

pub fn query_token_y(app: &App, lb_pair: &ContractInfo) -> StdResult<TokenType> {
    let res = lb_pair::QueryMsg::GetTokenY {}.test_query(lb_pair, app)?;
    let lb_pair::TokenYResponse { token_y } = res;
    Ok(token_y)
}

pub fn query_bin_step(app: &App, lb_pair: &ContractInfo) -> StdResult<u16> {
    let res = lb_pair::QueryMsg::GetBinStep {}.test_query(lb_pair, app)?;
    let lb_pair::BinStepResponse { bin_step } = res;
    Ok(bin_step)
}

pub fn query_reserves(app: &App, lb_pair: &ContractInfo) -> StdResult<(u128, u128)> {
    let res = lb_pair::QueryMsg::GetReserves {}.test_query(lb_pair, app)?;
    let lb_pair::ReservesResponse {
        reserve_x,
        reserve_y,
    } = res;
    Ok((reserve_x, reserve_y))
}

pub fn query_active_id(app: &App, lb_pair: &ContractInfo) -> StdResult<u32> {
    let res = lb_pair::QueryMsg::GetActiveId {}.test_query(lb_pair, app)?;
    let lb_pair::ActiveIdResponse { active_id } = res;
    Ok(active_id)
}

pub fn query_bin(app: &App, lb_pair: &ContractInfo, id: u32) -> StdResult<(u128, u128)> {
    let res = lb_pair::QueryMsg::GetBin { id }.test_query(lb_pair, app)?;
    let lb_pair::BinResponse {
        bin_reserve_x,
        bin_reserve_y,
    } = res;
    Ok((bin_reserve_x, bin_reserve_y))
}

pub fn query_next_non_empty_bin(
    app: &App,
    lb_pair: &ContractInfo,
    swap_for_y: bool,
    id: u32,
) -> StdResult<u32> {
    let res = lb_pair::QueryMsg::GetNextNonEmptyBin { swap_for_y, id }.test_query(lb_pair, app)?;
    let lb_pair::NextNonEmptyBinResponse { next_id } = res;
    Ok(next_id)
}

pub fn query_protocol_fees(app: &App, lb_pair: &ContractInfo) -> StdResult<(u128, u128)> {
    let res = lb_pair::QueryMsg::GetProtocolFees {}.test_query(lb_pair, app)?;
    let lb_pair::ProtocolFeesResponse {
        protocol_fee_x,
        protocol_fee_y,
    } = res;
    Ok((protocol_fee_x, protocol_fee_y))
}

pub fn query_oracle_parameters(
    app: &App,
    lb_pair: &ContractInfo,
) -> StdResult<(u8, u16, u16, u64, u64)> {
    let res = lb_pair::QueryMsg::GetOracleParameters {}.test_query(lb_pair, app)?;
    let lb_pair::OracleParametersResponse {
        sample_lifetime,
        size,
        active_size,
        last_updated,
        first_timestamp,
    } = res;
    Ok((
        sample_lifetime,
        size,
        active_size,
        last_updated,
        first_timestamp,
    ))
}

pub fn query_oracle_sample_at(
    app: &App,
    lb_pair: &ContractInfo,
    look_up_timestamp: u64,
) -> StdResult<(u64, u64, u64)> {
    let res =
        lb_pair::QueryMsg::GetOracleSampleAt { look_up_timestamp }.test_query(lb_pair, app)?;
    let lb_pair::OracleSampleAtResponse {
        cumulative_id,
        cumulative_volatility,
        cumulative_bin_crossed,
    } = res;
    Ok((cumulative_id, cumulative_volatility, cumulative_bin_crossed))
}

pub fn query_price_from_id(app: &App, lb_pair: &ContractInfo, id: u32) -> StdResult<Uint256> {
    let res = lb_pair::QueryMsg::GetPriceFromId { id }.test_query(lb_pair, app)?;
    let lb_pair::PriceFromIdResponse { price } = res;
    Ok(price)
}

pub fn query_id_from_price(app: &App, lb_pair: &ContractInfo, price: Uint256) -> StdResult<u32> {
    let res = lb_pair::QueryMsg::GetIdFromPrice { price }.test_query(lb_pair, app)?;
    let lb_pair::IdFromPriceResponse { id } = res;
    Ok(id)
}

pub fn query_swap_out(
    app: &App,
    lb_pair: &ContractInfo,
    amount_in: Uint128,
    swap_for_y: bool,
) -> StdResult<(Uint128, Uint128, Uint128)> {
    let res = lb_pair::QueryMsg::GetSwapOut {
        amount_in,
        swap_for_y,
    }
    .test_query(lb_pair, app)?;
    let lb_pair::SwapOutResponse {
        amount_out,
        amount_in_left,
        fee,
    } = res;
    Ok((amount_out, amount_in_left, fee))
}

pub fn query_swap_in(
    app: &App,
    lb_pair: &ContractInfo,
    amount_out: Uint128,
    swap_for_y: bool,
) -> StdResult<(Uint128, Uint128, Uint128)> {
    let res = lb_pair::QueryMsg::GetSwapIn {
        amount_out,
        swap_for_y,
    }
    .test_query(lb_pair, app)?;
    let lb_pair::SwapInResponse {
        amount_in,
        amount_out_left,
        fee,
    } = res;
    Ok((amount_in, amount_out_left, fee))
}
