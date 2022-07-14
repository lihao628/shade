use shade_protocol::c_std::{
    self,
    from_binary,
    to_binary,
    Api,
    Binary,
    CosmosMsg,
    Env,
    DepsMut,
    Response,
    Addr,
    Querier,
    StdError,
    StdResult,
    Storage,
    Uint128,
};
use shade_protocol::{
    snip20::helpers::{
        allowance_query,
        balance_query,
        decrease_allowance_msg,
        increase_allowance_msg,
        register_receive,
        set_viewing_key_msg,
    },
};

use shade_protocol::{
    contract_interfaces::{
        dao::treasury::{
            Allowance,
            Config,
            Flag,
            HandleAnswer,
            Manager,
            QueryAnswer,
        },
        snip20,
    },
    utils::{
        asset::Contract,
        cycle::{exceeds_cycle, parse_utc_datetime, Cycle},
        generic_response::ResponseStatus,
    },
};

use crate::{
    query,
    state::{
        allowances_r,
        allowances_w,
        asset_list_r,
        asset_list_w,
        assets_r,
        assets_w,
        config_r,
        config_w,
        managers_r,
        managers_w,
        self_address_r,
        viewing_key_r,
    },
};
use chrono::prelude::*;
use shade_protocol::contract_interfaces::dao::adapter;

pub fn receive(
    deps: DepsMut,
    env: Env,
    sender: Addr,
    _from: Addr,
    amount: Uint128,
    msg: Option<Binary>,
) -> StdResult<Response> {
    let key = sender.as_str().as_bytes();

    Ok(Response::new().set_data(to_binary(&HandleAnswer::Receive {
            status: ResponseStatus::Success,
        })?))
}

pub fn try_update_config(
    deps: DepsMut,
    env: Env,
    config: Config,
) -> StdResult<Response> {
    let cur_config = config_r(deps.storage).load()?;

    if info.sender != cur_config.admin {
        return Err(StdError::unauthorized());
    }

    config_w(deps.storage).save(&config)?;

    Ok(Response::new().set_data(to_binary(&HandleAnswer::UpdateConfig {
            status: ResponseStatus::Success,
        })?))
}

pub fn allowance_last_refresh(
    deps: Deps,
    env: &Env,
    allowance: &Allowance,
) -> StdResult<Option<DateTime<Utc>>> {
    // Parse previous refresh datetime
    let rfc3339 = match allowance {
        Allowance::Amount { last_refresh, .. } => last_refresh,
        Allowance::Portion { last_refresh, .. } => last_refresh,
    };

    DateTime::parse_from_rfc3339(&rfc3339)
        .map(|dt| Some(dt.with_timezone(&Utc)))
        .map_err(|_| StdError::generic_err(format!("Failed to parse datetime {}", rfc3339)))
}

pub fn rebalance(
    deps: DepsMut,
    env: &Env,
    asset: Addr,
) -> StdResult<Response> {
    let naive = NaiveDateTime::from_timestamp(env.block.time.seconds() as i64, 0);
    let now: DateTime<Utc> = DateTime::from_utc(naive, Utc);

    let key = viewing_key_r(deps.storage).load()?;
    let self_address = self_address_r(deps.storage).load()?;
    let mut messages = vec![];

    let full_asset = match assets_r(deps.storage).may_load(asset.as_str().as_bytes())? {
        Some(a) => a,
        None => {
            return Err(StdError::generic_err("Not an asset"));
        }
    };
    let allowances = allowances_r(deps.storage).load(asset.as_str().as_bytes())?;

    let mut balance = balance_query(
        &deps.querier,
        self_address,
        key.clone(),
        1,
        full_asset.contract.code_hash.clone(),
        full_asset.contract.address.clone(),
    )?
    .amount;

    /*
    let unbonding = unbonding_r(deps.storage).load(&asset.as_str().as_bytes())?;
    if unbonding > balance {
        balance = Uint128::zero();
    }
    else {
        balance = (balance - unbonding)?;
    }
    */

    let mut amount_total = Uint128::zero();
    let mut out_balance = Uint128::zero();

    let mut managers = managers_r(deps.storage).load()?;

    // Fetch & sum balances
    for allowance in &allowances {
        match allowance {
            Allowance::Amount {
                spender,
                cycle,
                amount,
                last_refresh,
            } => {
                //TODO: Query allowance
                amount_total += *amount;
                let i = managers
                    .iter()
                    .position(|m| m.contract.address == *spender)
                    .unwrap();
                managers[i].balance = adapter::balance_query(
                    &deps,
                    &full_asset.contract.address.clone(),
                    managers[i].contract.clone(),
                )?;
                out_balance += managers[i].balance;
            },
            Allowance::Portion {
                spender,
                portion,
                last_refresh,
                tolerance,
            } => {
                let i = managers
                    .iter()
                    .position(|m| m.contract.address == *spender)
                    .unwrap();
                managers[i].balance = adapter::balance_query(
                    &deps,
                    &full_asset.contract.address.clone(),
                    managers[i].contract.clone(),
                )?;
                out_balance += managers[i].balance;
            }
        }
    }

    let mut portion_total = ((balance + out_balance) - amount_total)?;

    managers_w(deps.storage).save(&managers)?;
    let config = config_r(deps.storage).load()?;

    // Perform rebalance
    for allowance in allowances {
        match allowance {
            Allowance::Amount {
                spender,
                cycle,
                amount,
                last_refresh,
            } => {
                let datetime = parse_utc_datetime(&last_refresh)?;

                if exceeds_cycle(&datetime, &now, cycle) {
                    if let Some(msg) = set_allowance(
                        &deps,
                        env,
                        spender,
                        amount,
                        key.clone(),
                        full_asset.contract.clone(),
                    )? {
                        messages.push(msg);
                    }
                }
            }
            Allowance::Portion {
                spender,
                portion,
                last_refresh,
                tolerance,
            } => {
                let desired_amount = portion_total.multiply_ratio(portion, 10u128.pow(18));
                let threshold = desired_amount.multiply_ratio(tolerance, 10u128.pow(18));

                let adapter = managers
                    .clone()
                    .into_iter()
                    .find(|m| m.contract.address == spender)
                    .unwrap();

                /* NOTE: remove claiming if rebalance tx becomes too heavy
                 * alternatives:
                 *  - separate rebalance & update,
                 *  - update could do an adapter.update on all "children"
                 *  - rebalance can be unique as its not needed as an adapter
                 */
                if adapter::claimable_query(&deps, 
                                            &asset, 
                                            adapter.contract.clone()
                                    )? > Uint128::zero() {
                    messages.push(adapter::claim_msg(
                        asset.clone(),
                        adapter.contract.clone()
                    )?);
                };

                let cur_allowance = allowance_query(
                    &deps.querier,
                    env.contract.address.clone(),
                    spender.clone(),
                    key.clone(),
                    1,
                    full_asset.contract.code_hash.clone(),
                    full_asset.contract.address.clone(),
                )?
                .allowance;

                // UnderFunded
                if cur_allowance + adapter.balance < desired_amount {
                    let increase = (desired_amount - (adapter.balance + cur_allowance))?;
                    if increase < threshold {
                        continue;
                    }
                    messages.push(increase_allowance_msg(
                        spender,
                        increase,
                        None,
                        None,
                        1,
                        full_asset.contract.code_hash.clone(),
                        full_asset.contract.address.clone(),
                    )?);
                }
                // Overfunded
                else if cur_allowance + adapter.balance > desired_amount {
                    let mut decrease = ((adapter.balance + cur_allowance) - desired_amount)?;
                    if decrease < threshold {
                        continue;
                    }

                    // Remove allowance first
                    if cur_allowance > Uint128::zero() {
                        if cur_allowance < decrease {
                            messages.push(decrease_allowance_msg(
                                spender,
                                cur_allowance,
                                None,
                                None,
                                1,
                                full_asset.contract.code_hash.clone(),
                                full_asset.contract.address.clone(),
                            )?);
                            decrease = (decrease - cur_allowance)?;
                        } else {
                            messages.push(decrease_allowance_msg(
                                spender,
                                decrease,
                                None,
                                None,
                                1,
                                full_asset.contract.code_hash.clone(),
                                full_asset.contract.address.clone(),
                            )?);
                            decrease = Uint128::zero();
                        }
                    }

                    // Unbond remaining
                    if decrease > Uint128::zero() {
                        messages.push(adapter::unbond_msg(
                            asset.clone(),
                            decrease,
                            adapter.contract,
                        )?);
                    }
                }
            }
        }
    }

    Ok(Response::new().set_data(to_binary(&HandleAnswer::Rebalance {
            status: ResponseStatus::Success,
        })?))
}

pub fn set_allowance(
    deps: Deps,
    env: &Env,
    spender: Addr,
    amount: Uint128,
    key: String,
    asset: Contract,
) -> StdResult<Option<CosmosMsg>> {
    let cur_allowance = allowance_query(
        &deps.querier,
        env.contract.address.clone(),
        spender.clone(),
        key,
        1,
        asset.code_hash.clone(),
        asset.address.clone(),
    )?;

    match amount.cmp(&cur_allowance.allowance) {
        // Decrease Allowance
        std::cmp::Ordering::Less => Ok(Some(decrease_allowance_msg(
            spender.clone(),
            (cur_allowance.allowance - amount)?,
            None,
            None,
            1,
            asset.code_hash.clone(),
            asset.address.clone(),
        )?)),
        // Increase Allowance
        std::cmp::Ordering::Greater => Ok(Some(increase_allowance_msg(
            spender.clone(),
            (amount - cur_allowance.allowance)?,
            None,
            None,
            1,
            asset.code_hash.clone(),
            asset.address.clone(),
        )?)),
        _ => Ok(None),
    }
}

pub fn try_register_asset(
    deps: DepsMut,
    env: &Env,
    contract: &Contract,
    reserves: Option<Uint128>,
) -> StdResult<Response> {
    let config = config_r(deps.storage).load()?;

    if info.sender != config.admin {
        return Err(StdError::unauthorized());
    }

    asset_list_w(deps.storage).update(|mut list| {
        list.push(contract.address.clone());
        Ok(list)
    })?;

    assets_w(deps.storage).save(
        contract.address.to_string().as_bytes(),
        &snip20::helpers::fetch_snip20(contract, &deps.querier)?,
    )?;

    allowances_w(deps.storage).save(contract.address.as_str().as_bytes(), &Vec::new())?;

    Ok(Response {
        messages: vec![
            // Register contract in asset
            register_receive(
                env.contract_code_hash.clone(),
                None,
                contract
            )?,
            // Set viewing key
            set_viewing_key_msg(
                viewing_key_r(deps.storage).load()?,
                None,
                256,
                contract.code_hash.clone(),
                contract.address.clone(),
            )?,
        ],
        log: vec![],
        data: Some(to_binary(&HandleAnswer::RegisterAsset {
            status: ResponseStatus::Success,
        })?))
}

pub fn register_manager(
    deps: DepsMut,
    env: &Env,
    contract: &mut Contract,
) -> StdResult<Response> {
    let config = config_r(deps.storage).load()?;

    if info.sender != config.admin {
        return Err(StdError::unauthorized());
    }

    managers_w(deps.storage).update(|mut adapters| {
        if adapters
            .iter()
            .map(|m| m.contract.clone())
            .collect::<Vec<_>>()
            .contains(&contract)
        {
            return Err(StdError::generic_err("Manager already registered"));
        }
        adapters.push(Manager {
            contract: contract.clone(),
            balance: Uint128::zero(),
            desired: Uint128::zero(),
        });
        Ok(adapters)
    })?;

    Ok(Response::new().set_data(to_binary(&HandleAnswer::RegisterAsset {
            status: ResponseStatus::Success,
        })?))
}

// extract contract address if any
fn allowance_address(allowance: &Allowance) -> Option<&Addr> {
    match allowance {
        Allowance::Amount { spender, .. } => Some(&spender),
        Allowance::Portion { spender, .. } => Some(&spender),
        _ => None,
    }
}

// extract allowanceaiton portion
fn allowance_portion(allowance: &Allowance) -> Uint128 {
    match allowance {
        Allowance::Portion { portion, .. } => *portion,
        Allowance::Amount { .. } => Uint128::zero(),
    }
}

fn allowance_amount(allowance: &Allowance) -> Uint128 {
    match allowance {
        Allowance::Amount { amount, .. } => *amount,
        Allowance::Portion { .. } => Uint128::zero(),
    }
}

pub fn allowance(
    deps: DepsMut,
    env: &Env,
    asset: Addr,
    allowance: Allowance,
) -> StdResult<Response> {
    static ONE_HUNDRED_PERCENT: u128 = 10u128.pow(18);

    let config = config_r(deps.storage).load()?;

    /* ADMIN ONLY */
    if info.sender != config.admin {
        return Err(StdError::unauthorized());
    }

    let adapters = managers_r(deps.storage).load()?;

    // Disallow Portion on non-adapters
    match allowance {
        Allowance::Portion { ref spender, .. } => {
            if adapters
                .clone()
                .into_iter()
                .find(|m| m.contract.address == *spender)
                .is_none()
            {
                return Err(StdError::generic_err("Portion allowances to adapters only"));
            }
        }
        _ => {}
    };

    let key = asset.as_str().as_bytes();

    let mut apps = allowances_r(deps.storage)
        .may_load(key)?
        .unwrap_or_default();

    let allow_address = allowance_address(&allowance);

    // find any old allowances with the same contract address & sum current allowances in one loop.
    // saves looping twice in the worst case
    // TODO: Remove Reserves if this would be one of those
    let (stale_allowance, cur_allowance_portion) = apps.iter().enumerate().fold(
        (None, 0u128),
        |(stale_allowance, cur_allowances), (idx, a)| {
            if stale_allowance.is_none() && allowance_address(a) == allow_address {
                (Some(idx), cur_allowances)
            } else {
                (
                    stale_allowance,
                    cur_allowances + allowance_portion(a).u128(),
                )
            }
        },
    );

    if let Some(old_allowance_idx) = stale_allowance {
        apps.remove(old_allowance_idx);
    }

    let new_allowance_portion = allowance_portion(&allowance).u128();

    if cur_allowance_portion + new_allowance_portion > ONE_HUNDRED_PERCENT {
        return Err(StdError::generic_err(
            "Invalid allowance total exceeding 100%",
        ));
    }

    // Zero the last-refresh
    let datetime: DateTime<Utc> = DateTime::from_utc(NaiveDateTime::from_timestamp(0, 0), Utc);

    let spender = match allowance {
        Allowance::Portion {
            spender,
            portion,
            last_refresh,
            tolerance,
        } => {
            apps.push(Allowance::Portion {
                spender: spender.clone(),
                portion: portion.clone(),
                last_refresh: datetime.to_rfc3339(),
                tolerance,
            });
            spender
        }
        Allowance::Amount {
            spender,
            cycle,
            amount,
            last_refresh,
        } => {
            apps.push(Allowance::Amount {
                spender: spender.clone(),
                cycle: cycle.clone(),
                amount: amount.clone(),
                last_refresh: datetime.to_rfc3339(),
            });
            spender
        }
    };

    allowances_w(deps.storage).save(key, &apps)?;

    Ok(Response::new().set_data(to_binary(&HandleAnswer::Allowance {
            status: ResponseStatus::Success,
        })?))
}

pub fn claim(
    deps: DepsMut,
    env: &Env,
    asset: Addr,
) -> StdResult<Response> {

    let key = asset.as_str().as_bytes();

    let managers = managers_r(deps.storage).load()?;
    let allowances = allowances_r(deps.storage).load(&key)?;

    let mut messages = vec![];

    let mut claimed = Uint128::zero();

    for allowance in allowances {
        match allowance {
            Allowance::Amount { .. } => {}
            Allowance::Portion { spender, .. } => {
                if let Some(manager) = managers.iter().find(|m| m.contract.address == spender) {
                    let claimable =
                        adapter::claimable_query(&deps, &asset, manager.contract.clone())?;

                    if claimable > Uint128::zero() {
                        messages.push(adapter::claim_msg(asset.clone(), manager.contract.clone())?);
                        claimed += claimable;
                    }
                }
            }
        }
    }

    Ok(Response::new().set_data(to_binary(&adapter::HandleAnswer::Claim {
            status: ResponseStatus::Success,
            amount: claimed,
        })?))
}

pub fn unbond(
    deps: DepsMut,
    env: &Env,
    asset: Addr,
    amount: Uint128,
) -> StdResult<Response> {
    /*
    if info.sender != config_r(deps.storage).load()?.admin {
        return Err(StdError::unauthorized());
    }
    */

    let managers = managers_r(deps.storage).load()?;

    let mut messages = vec![];

    let mut unbond_amount = amount;
    let mut unbonded = Uint128::zero();

    for allowance in allowances_r(deps.storage).load(asset.as_str().as_bytes())? {
        match allowance {
            Allowance::Amount { .. } => {}
            Allowance::Portion { spender, .. } => {
                if let Some(manager) = managers.iter().find(|m| m.contract.address == spender) {
                    let unbondable = adapter::unbondable_query(&deps, &asset.clone(), manager.contract.clone())?;

                    if unbondable > unbond_amount {
                        messages.push(
                            adapter::unbond_msg(
                                asset.clone(),
                                unbond_amount,
                                manager.contract.clone(),
                            )?
                        );
                        unbond_amount = Uint128::zero();
                        unbonded = unbond_amount;
                    }
                    else {
                        messages.push(
                            adapter::unbond_msg(
                                asset.clone(),
                                unbondable,
                                manager.contract.clone(),
                            )?
                        );
                        unbond_amount = (unbond_amount - unbondable)?;
                        unbonded = unbonded + unbondable;
                    }
                }
            }
        }

        if unbond_amount == Uint128::zero() {
            break;
        }
    }

    // TODO: Shouldn't be an error, need to log somehow
    if unbond_amount > Uint128::zero() {
        return Err(StdError::generic_err(format!(
            "Failed to fully unbond {}, {} available",
            amount,
            (amount - unbond_amount)?
        )));
    }

    Ok(Response::new().set_data(to_binary(&adapter::HandleAnswer::Claim {
            status: ResponseStatus::Success,
            amount,
        })?))
}
