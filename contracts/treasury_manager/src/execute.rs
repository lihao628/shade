use shade_protocol::{
    admin::helpers::{validate_admin, AdminPermissions},
    c_std::{
        self,
        to_binary,
        Addr,
        Api,
        Binary,
        DepsMut,
        Env,
        MessageInfo,
        Querier,
        Response,
        StdError,
        StdResult,
        Storage,
        Uint128,
    },
    dao::{
        adapter,
        treasury_manager::{
            Action,
            Allocation,
            AllocationMeta,
            AllocationType,
            Balance,
            Config,
            Context,
            ExecuteAnswer,
            Holding,
            Metric,
            Status,
        },
    },
    snip20,
    snip20::{
        batch::{SendAction, SendFromAction},
        helpers::{
            allowance_query,
            balance_query,
            batch_send_from_msg,
            batch_send_msg,
            register_receive,
            send_msg,
            set_viewing_key_msg,
        },
    },
    utils::{asset::Contract, generic_response::ResponseStatus},
};

use std::collections::HashMap;

use crate::storage::*;

pub fn receive(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    sender: Addr,
    from: Addr,
    amount: Uint128,
    msg: Option<Binary>,
) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;
    let asset = ASSETS.load(deps.storage, info.sender.clone())?;

    METRICS.pushf(deps.storage, env.block.time, Metric {
        action: Action::FundsReceived,
        context: Context::Receive,
        timestamp: env.block.time.seconds(),
        token: info.sender.clone(),
        amount,
        user: sender,
    })?;

    // Do nothing if its an adapter (claimed funds)
    if let Some(adapter) = ALLOCATIONS
        .load(deps.storage, info.sender.clone())?
        .iter()
        .find(|a| a.contract.address == from)
    {
        println!("Manager Recv from Adapter {}", amount);
        return Ok(Response::new().set_data(to_binary(&ExecuteAnswer::Receive {
            status: ResponseStatus::Success,
        })?));
    }

    let holder = match HOLDERS.load(deps.storage)?.contains(&from) {
        true => from,

        // Default to treasury if not sent by a holder
        false => config.treasury,
    };

    // Update holdings
    HOLDING.update(deps.storage, holder, |h| -> StdResult<Holding> {
        let mut holding = h.unwrap();
        if let Some(i) = holding
            .balances
            .iter()
            .position(|b| b.token == asset.contract.address)
        {
            holding.balances[i].amount += amount;
        } else {
            holding.balances.push(Balance {
                token: asset.contract.address,
                amount,
            });
        }
        Ok(holding)
    })?;

    Ok(Response::new().set_data(to_binary(&ExecuteAnswer::Receive {
        status: ResponseStatus::Success,
    })?))
}

pub fn try_update_config(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    config: Config,
) -> StdResult<Response> {
    let cur_config = CONFIG.load(deps.storage)?;

    validate_admin(
        &deps.querier,
        AdminPermissions::TreasuryManager,
        &info.sender,
        &cur_config.admin_auth,
    )?;

    CONFIG.save(deps.storage, &config)?;

    Ok(
        Response::new().set_data(to_binary(&ExecuteAnswer::UpdateConfig {
            status: ResponseStatus::Success,
        })?),
    )
}

pub fn try_register_asset(
    deps: DepsMut,
    env: &Env,
    info: MessageInfo,
    contract: &Contract,
) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;

    validate_admin(
        &deps.querier,
        AdminPermissions::TreasuryManager,
        &info.sender,
        &config.admin_auth,
    )?;

    ASSET_LIST.update(deps.storage, |mut list| -> StdResult<Vec<Addr>> {
        list.push(contract.address.clone());
        Ok(list)
    })?;

    ASSETS.save(
        deps.storage,
        contract.address.clone(),
        &snip20::helpers::fetch_snip20(&contract, &deps.querier)?,
    )?;

    ALLOCATIONS.save(deps.storage, contract.address.clone(), &Vec::new())?;

    Ok(Response::new()
        .add_messages(vec![
            // Register contract in asset
            register_receive(env.contract.code_hash.clone(), None, &contract)?,
            // Set viewing key
            set_viewing_key_msg(VIEWING_KEY.load(deps.storage)?, None, &contract)?,
        ])
        .set_data(to_binary(&ExecuteAnswer::RegisterAsset {
            status: ResponseStatus::Success,
        })?))
}

pub fn allocate(
    deps: DepsMut,
    env: &Env,
    info: MessageInfo,
    asset: Addr,
    allocation: Allocation,
) -> StdResult<Response> {
    static ONE_HUNDRED_PERCENT: u128 = 10u128.pow(18);

    let config = CONFIG.load(deps.storage)?;

    /* ADMIN ONLY */
    validate_admin(
        &deps.querier,
        AdminPermissions::TreasuryManager,
        &info.sender,
        &config.admin_auth,
    )?;

    //let asset = deps.api.addr_validate(asset.as_str())?;

    let mut apps = ALLOCATIONS
        .may_load(deps.storage, asset.clone())?
        .unwrap_or_default();

    let stale_alloc = apps
        .iter()
        .position(|a| a.contract.address == allocation.contract.address);

    match stale_alloc {
        Some(i) => {
            apps.swap_remove(i);
        }
        None => {}
    };

    apps.push(AllocationMeta {
        nick: allocation.nick,
        contract: allocation.contract,
        amount: allocation.amount,
        alloc_type: allocation.alloc_type,
        balance: Uint128::zero(),
        tolerance: allocation.tolerance,
    });

    if apps
        .iter()
        .map(|a| {
            if a.alloc_type == AllocationType::Portion {
                a.amount.u128()
            } else {
                0u128
            }
        })
        .sum::<u128>()
        > ONE_HUNDRED_PERCENT
    {
        return Err(StdError::generic_err(
            "Invalid allocation total exceeding 100%",
        ));
    }

    apps.sort_by(|a, b| match a.alloc_type {
        AllocationType::Amount => match b.alloc_type {
            AllocationType::Amount => std::cmp::Ordering::Equal,
            AllocationType::Portion => std::cmp::Ordering::Less,
        },
        AllocationType::Portion => match b.alloc_type {
            AllocationType::Amount => std::cmp::Ordering::Greater,
            AllocationType::Portion => std::cmp::Ordering::Equal,
        },
    });

    ALLOCATIONS.save(deps.storage, asset.clone(), &apps)?;

    Ok(
        Response::new().set_data(to_binary(&ExecuteAnswer::Allocate {
            status: ResponseStatus::Success,
        })?),
    )
}

pub fn claim(deps: DepsMut, env: &Env, info: MessageInfo, asset: Addr) -> StdResult<Response> {
    //let asset = deps.api.addr_validate(asset.as_str())?;

    if !ASSET_LIST.load(deps.storage)?.contains(&asset.clone()) {
        return Err(StdError::generic_err("Unrecognized asset"));
    }
    let full_asset = ASSETS.load(deps.storage, asset.clone())?;

    let config = CONFIG.load(deps.storage)?;
    let mut claimer = info.sender;

    if validate_admin(
        &deps.querier,
        AdminPermissions::TreasuryManager,
        &claimer,
        &config.admin_auth,
    )
    .is_ok()
    {
        //assert!(false, "CLAIMER TREASURY");
        claimer = config.treasury;
    }

    let holders = HOLDERS.load(deps.storage)?;

    if !holders.contains(&claimer.clone()) {
        return Err(StdError::generic_err("Unauthorized"));
    }
    let mut holding = HOLDING.load(deps.storage, claimer.clone())?;

    let unbonding_i = match holding
        .unbondings
        .iter_mut()
        .position(|u| u.token == asset.clone())
    {
        Some(i) => i,
        None => {
            return Err(StdError::generic_err(format!(
                "{} has no unbondings for {}",
                claimer.clone(),
                asset.clone()
            )));
        }
    };

    let reserves = balance_query(
        &deps.querier,
        SELF_ADDRESS.load(deps.storage)?,
        VIEWING_KEY.load(deps.storage)?,
        &full_asset.contract.clone(),
    )?;

    let mut messages = vec![];
    let mut total_claimed = Uint128::zero();

    // Claim if more funds are needed
    if holding.unbondings[unbonding_i].amount > reserves {
        //assert!(false, "reduce claim_amount {} - {}", unbonding.amount, reserves);
        let mut claim_amount = holding.unbondings[unbonding_i].amount - reserves;

        for alloc in ALLOCATIONS.load(deps.storage, asset.clone())? {
            if claim_amount == Uint128::zero() {
                break;
            }

            let claim = adapter::claimable_query(deps.querier, &asset, alloc.contract.clone())?;

            if claim > Uint128::zero() {
                messages.push(adapter::claim_msg(&asset, alloc.contract)?);
                if claim > claim_amount {
                    claim_amount = Uint128::zero();
                } else {
                    claim_amount = claim_amount - claim;
                }
                total_claimed += claim;
            }
        }
    }

    let send_amount;

    if holding.unbondings[unbonding_i].amount > reserves + total_claimed {
        send_amount = reserves + total_claimed;
    } else {
        send_amount = holding.unbondings[unbonding_i].amount;
    }
    // Adjust unbonding amount
    holding.unbondings[unbonding_i].amount = holding.unbondings[unbonding_i].amount - send_amount;
    HOLDING.save(deps.storage, claimer.clone(), &holding)?;

    METRICS.pushf(deps.storage, env.block.time, Metric {
        action: Action::SendFunds,
        context: Context::Claim,
        timestamp: env.block.time.seconds(),
        token: asset.clone(),
        amount: send_amount,
        user: claimer.clone(),
    })?;

    // Send claimed funds
    messages.push(send_msg(
        claimer.clone(),
        send_amount,
        None,
        None,
        None,
        &full_asset.contract.clone(),
    )?);

    Ok(Response::new().add_messages(messages).set_data(to_binary(
        &adapter::ExecuteAnswer::Claim {
            status: ResponseStatus::Success,
            amount: reserves + total_claimed,
        },
    )?))
}

pub fn update(deps: DepsMut, env: &Env, info: MessageInfo, asset: Addr) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;

    let full_asset = ASSETS.load(deps.storage, asset.clone())?;

    let mut allocations = ALLOCATIONS.load(deps.storage, asset.clone())?;
    println!("354 {:?}", allocations[0]);

    // Build metadata
    let mut amount_total = Uint128::zero();
    let mut portion_total = Uint128::zero();

    let mut metadata: HashMap<Addr, (Uint128, Uint128)> = HashMap::new();
    let mut messages = vec![];

    for i in 0..allocations.len() {
        let claimable =
            adapter::claimable_query(deps.querier, &asset, allocations[i].contract.clone())?;

        if !claimable.is_zero() {
            messages.push(adapter::claim_msg(&asset, allocations[i].contract.clone())?);
        }

        let unbonding =
            adapter::unbonding_query(deps.querier, &asset, allocations[i].contract.clone())?;

        let balance = adapter::balance_query(
            deps.querier,
            &full_asset.contract.address,
            allocations[i].contract.clone(),
        )?;

        allocations[i].balance = balance;

        metadata.insert(
            allocations[i].contract.address.clone(),
            (allocations[i].balance, unbonding),
        );

        println!("{}", allocations[i].amount);
        match allocations[i].alloc_type {
            AllocationType::Amount => amount_total += balance,
            AllocationType::Portion => {
                println!("PORTION {}", balance);
                portion_total += balance;
            }
        };
    }
    println!(
        "373 at: {}, pt: {}",
        amount_total.u128(),
        portion_total.u128(),
    );

    let mut holder_unbonding = Uint128::zero();
    let mut holder_principal = Uint128::zero();

    // Withold holder unbondings
    for h in HOLDERS.load(deps.storage)? {
        let holding = HOLDING.load(deps.storage, h)?;
        if let Some(u) = holding.unbondings.iter().find(|u| u.token == asset) {
            holder_unbonding += u.amount;
        }
        if let Some(b) = holding.balances.iter().find(|u| u.token == asset) {
            holder_principal += b.amount;
        }
    }

    // Batch send_from actions
    let mut send_from_actions = vec![];
    let mut send_actions = vec![];

    let key = VIEWING_KEY.load(deps.storage)?;

    // Available treasury allowance
    let mut allowance = allowance_query(
        &deps.querier,
        config.treasury.clone(),
        env.contract.address.clone(),
        key.clone(),
        1,
        &full_asset.contract.clone(),
    )?
    .allowance;
    println!("410 allowance {}", allowance.u128());

    // Available balance
    let token_balance = balance_query(
        &deps.querier,
        SELF_ADDRESS.load(deps.storage)?,
        key.clone(),
        &full_asset.contract.clone(),
    )?;
    println!("419 balance {}", token_balance.u128());

    let total = amount_total + portion_total + token_balance + allowance - holder_unbonding;
    println!(
        "TOTAL {} at {} pt {} tb {} allow {} hunb {}",
        total, amount_total, portion_total, token_balance, allowance, holder_unbonding
    );

    let mut allowance_used = Uint128::zero();
    let mut balance_used = Uint128::zero();


      allocations.sort_by(|a, b| match a.alloc_type {
          AllocationType::Amount => match b.alloc_type {
              AllocationType::Amount => std::cmp::Ordering::Equal,
              AllocationType::Portion => std::cmp::Ordering::Less,
          },
          AllocationType::Portion => match b.alloc_type {
              AllocationType::Amount => std::cmp::Ordering::Greater,
              AllocationType::Portion => std::cmp::Ordering::Equal,
          },
      });
      println!("440 allocations {:?}", allocations);
  
    let mut amount_sending_out = Uint128::zero();
    for adapter in allocations.clone() {
        println!("ADAPTER REBALANCE {}", adapter.nick.unwrap());
        println!("445 total {}", total.u128());
        println!("446 adapter.amount {}", adapter.amount);
        let desired_amount = match adapter.alloc_type {
            AllocationType::Amount => {
                amount_sending_out += adapter.amount;
                adapter.amount
            }
            AllocationType::Portion => {
                if total > amount_sending_out {
                    adapter
                        .amount
                        .multiply_ratio(total - amount_sending_out, 10u128.pow(18))
                } else {
                    Uint128::zero()
                }
            }
        };
        let threshold = desired_amount.multiply_ratio(adapter.tolerance, 10u128.pow(18));
        println!("437 desired_amount {}", desired_amount);

        let (adapter_balance, unbonding) = metadata[&adapter.contract.address];
        let mut available = adapter_balance - unbonding;

        // Under Funded -- send balance then allowance
        if available < desired_amount {
            let mut desired_input = desired_amount - available;
            if desired_input <= threshold {
                continue;
            }

            // Fully covered by balance
            if desired_input < available {
                send_actions.push(SendAction {
                    recipient: adapter.contract.address.clone().to_string(),
                    recipient_code_hash: Some(adapter.contract.code_hash.clone()),
                    amount: desired_input,
                    msg: None,
                    memo: None,
                });
                METRICS.pushf(deps.storage, env.block.time, Metric {
                    action: Action::SendFunds,
                    context: Context::Update,
                    timestamp: env.block.time.seconds(),
                    token: asset.clone(),
                    amount: desired_input,
                    user: adapter.contract.address.clone(),
                })?;

                available = available - desired_input;
                balance_used += desired_input;
                continue;
            }
            // Send all balance
            else if !available.is_zero() {
                send_actions.push(SendAction {
                    recipient: adapter.contract.address.clone().to_string(),
                    recipient_code_hash: Some(adapter.contract.code_hash.clone()),
                    amount: available,
                    msg: None,
                    memo: None,
                });
                METRICS.pushf(deps.storage, env.block.time, Metric {
                    action: Action::SendFunds,
                    context: Context::Update,
                    timestamp: env.block.time.seconds(),
                    token: asset.clone(),
                    amount: available,
                    user: adapter.contract.address.clone(),
                })?;

                desired_input = desired_input - available;
                available = Uint128::zero();
                //                break;
            }

            if !allowance.is_zero() {
                // Fully covered by allowance
                if desired_input < allowance {
                    send_from_actions.push(SendFromAction {
                        owner: config.treasury.clone().to_string(),
                        recipient: adapter.contract.address.clone().to_string(),
                        recipient_code_hash: Some(adapter.contract.code_hash.clone()),
                        amount: desired_input,
                        msg: None,
                        memo: None,
                    });
                    METRICS.pushf(deps.storage, env.block.time, Metric {
                        action: Action::SendFundsFrom,
                        context: Context::Update,
                        timestamp: env.block.time.seconds(),
                        token: asset.clone(),
                        amount: desired_input,
                        user: adapter.contract.address.clone(),
                    })?;

                    allowance_used += desired_input;
                    allowance = allowance - desired_input;
                    continue;
                }
                // Send all allowance
                else if !allowance.is_zero() {
                    send_from_actions.push(SendFromAction {
                        owner: config.treasury.clone().to_string(),
                        recipient: adapter.contract.address.clone().to_string(),
                        recipient_code_hash: Some(adapter.contract.code_hash.clone()),
                        amount: allowance,
                        msg: None,
                        memo: None,
                    });
                    METRICS.pushf(deps.storage, env.block.time, Metric {
                        action: Action::SendFundsFrom,
                        context: Context::Update,
                        timestamp: env.block.time.seconds(),
                        token: asset.clone(),
                        amount: allowance,
                        user: adapter.contract.address.clone(),
                    })?;

                    allowance_used += allowance;
                    //desired_input = desired_input - allowance;
                    allowance = Uint128::zero();
                    //break;
                }
            }
        }
        // Over funded -- unbond
        else if available > desired_amount {
            let desired_output = available - desired_amount;
            if desired_output <= threshold {
                continue;
            }
            messages.push(adapter::unbond_msg(
                &asset.clone(),
                desired_output,
                adapter.contract.clone(),
            )?);
            METRICS.pushf(deps.storage, env.block.time, Metric {
                action: Action::Unbond,
                context: Context::Update,
                timestamp: env.block.time.seconds(),
                token: asset.clone(),
                amount: desired_output,
                user: adapter.contract.address.clone(),
            })?;
        }
    }

    // Credit treasury balance with allowance used
    HOLDING.update(
        deps.storage,
        config.treasury.clone(),
        |h| -> StdResult<Holding> {
            let mut holding = h.unwrap();
            if let Some(i) = holding
                .balances
                .iter()
                .position(|u| u.token == asset.clone())
            {
                holding.balances[i].amount += allowance_used;
            } else {
                holding.balances.push(Balance {
                    token: asset.clone(),
                    amount: allowance_used,
                });
            }
            Ok(holding)
        },
    )?;

    // Determine Gainz & Losses & credit to treasury
    holder_principal += allowance_used;
    if total - allowance > holder_principal {
        let gains = (total - allowance) - holder_principal;
        println!("Gainzz {}", gains);

        // debit gains to treasury
        let mut holding = HOLDING.load(deps.storage, config.treasury.clone())?;
        if let Some(i) = holding.balances.iter().position(|u| u.token == asset) {
            holding.balances[i].amount += gains;
        }
        METRICS.pushf(deps.storage, env.block.time, Metric {
            action: Action::RealizeGains,
            context: Context::Update,
            timestamp: env.block.time.seconds(),
            token: asset.clone(),
            amount: gains,
            user: config.treasury.clone(),
        })?;
        HOLDING.save(deps.storage, config.treasury.clone(), &holding)?;
    } else if total - allowance < holder_principal {
        let losses = holder_principal - (total - allowance);
        println!("lossez {}", losses);

        // credit losses to treasury
        let mut holding = HOLDING.load(deps.storage, config.treasury.clone())?;
        if let Some(i) = holding.balances.iter().position(|u| u.token == asset) {
            holding.balances[i].amount -= losses;
        }

        METRICS.pushf(deps.storage, env.block.time, Metric {
            action: Action::RealizeGains,
            context: Context::Update,
            timestamp: env.block.time.seconds(),
            token: asset.clone(),
            amount: losses,
            user: config.treasury.clone(),
        })?;
        HOLDING.save(deps.storage, config.treasury.clone(), &holding)?;
    }

    if !send_actions.is_empty() {
        messages.push(batch_send_msg(
            send_actions,
            None,
            &full_asset.contract.clone(),
        )?);
    }

    if !send_from_actions.is_empty() {
        messages.push(batch_send_from_msg(
            send_from_actions,
            None,
            &full_asset.contract.clone(),
        )?);
    }

    Ok(Response::new().add_messages(messages).set_data(to_binary(
        &adapter::ExecuteAnswer::Update {
            status: ResponseStatus::Success,
        },
    )?))
}

pub fn unbond(
    deps: DepsMut,
    env: &Env,
    info: MessageInfo,
    asset: Addr,
    amount: Uint128,
) -> StdResult<Response> {
    println!("ITS THE MANAGER BITCH");
    let config = CONFIG.load(deps.storage)?;
    //let asset = deps.api.addr_validate(asset.as_str())?;
    let mut unbonder = info.sender.clone();
    let holders = HOLDERS.load(deps.storage)?;

    // admin unbonds on behalf of treasury
    if validate_admin(
        &deps.querier,
        AdminPermissions::TreasuryManager,
        &unbonder,
        &config.admin_auth,
    )
    .is_ok()
    {
        unbonder = config.treasury.clone();
    }
    // Only a holder can unbond otherwise
    else if !holders.contains(&unbonder.clone()) {
        return Err(StdError::generic_err("unauthorized"));
    }

    let mut holding = HOLDING.load(deps.storage, unbonder.clone())?;

    if holding.status != Status::Active {
        return Err(StdError::generic_err("Inactive Holding"));
    }

    let balance_i = match holding
        .balances
        .iter()
        .position(|h| h.token == asset.clone())
    {
        Some(i) => i,
        None => {
            return Err(StdError::generic_err(format!(
                "Cannot unbond, holder has no holdings of {}",
                asset.clone()
            )));
        }
    };

    // Check balance exceeds unbond amount
    if holding.balances[balance_i].amount < amount {
        return Err(StdError::generic_err("Not enough funds to unbond"));
    } else {
        // Reduce balance by unbonding
        holding.balances[balance_i].amount = holding.balances[balance_i].amount - amount;
    }

    // Add unbonding
    if let Some(u) = holding
        .unbondings
        .iter()
        .position(|h| h.token == asset.clone())
    {
        holding.unbondings[u].amount += amount;
    } else {
        holding.unbondings.push(Balance {
            token: asset.clone(),
            amount,
        });
    }

    HOLDING.save(deps.storage, unbonder.clone(), &holding)?;

    let mut unbond_amount = amount;

    // get other holders unbonding amount to hold
    let mut other_unbondings = Uint128::zero();

    for h in holders {
        if h == unbonder.clone() {
            continue;
        }
        let other_holding = HOLDING.load(deps.storage, h)?;
        if let Some(u) = other_holding
            .unbondings
            .iter()
            .find(|u| u.token == asset.clone())
        {
            other_unbondings += u.amount;
        }
    }

    let full_asset = ASSETS.load(deps.storage, asset.clone())?;

    // Reserves to be sent immediately
    let mut reserves = balance_query(
        &deps.querier,
        SELF_ADDRESS.load(deps.storage)?,
        VIEWING_KEY.load(deps.storage)?,
        &full_asset.contract.clone(),
    )?;

    println!("MANAGER UNBOND {} RES {}", amount, reserves);

    // Remove pending unbondings from reserves
    if reserves > other_unbondings {
        reserves = reserves - other_unbondings;
    } else {
        reserves = Uint128::zero();
    }

    let mut messages = vec![];

    println!(
        "TREASU:RY MAN UNBOND HERE \t \t unbond amount: {}, reseresves: {}",
        amount, reserves
    );
    // Send available reserves to unbonder
    if !reserves.is_zero() {
        if reserves < unbond_amount {
            //TODO: this should include 'reserves' unbonded from adapters
            messages.push(send_msg(
                unbonder.clone(),
                reserves,
                None,
                None,
                None,
                &full_asset.contract.clone(),
            )?);
            METRICS.pushf(deps.storage, env.block.time, Metric {
                action: Action::SendFunds,
                context: Context::Unbond,
                timestamp: env.block.time.seconds(),
                token: asset.clone(),
                amount: reserves,
                user: unbonder.clone(),
            })?;
            unbond_amount = unbond_amount - reserves;

            // Reflect sent funds in unbondings
            HOLDING.update(deps.storage, unbonder, |h| -> StdResult<Holding> {
                let mut holding = h.unwrap();
                if let Some(i) = holding.unbondings.iter().position(|u| u.token == asset) {
                    holding.unbondings[i].amount = holding.unbondings[i].amount - reserves;
                } else {
                    return Err(StdError::generic_err(
                        "Failed to get unbonding, shouldn't happen",
                    ));
                }
                Ok(holding)
            })?;
        } else {
            // reserves can cover unbond
            messages.push(send_msg(
                unbonder.clone(),
                amount,
                None,
                None,
                None,
                &full_asset.contract.clone(),
            )?);
            METRICS.pushf(deps.storage, env.block.time, Metric {
                action: Action::SendFunds,
                context: Context::Unbond,
                timestamp: env.block.time.seconds(),
                token: asset.clone(),
                amount,
                user: unbonder.clone(),
            })?;
            unbond_amount = unbond_amount - amount;

            // Reflect sent funds in unbondings
            HOLDING.update(deps.storage, unbonder, |h| {
                let mut holder = h.unwrap();
                if let Some(i) = holder.unbondings.iter().position(|u| u.token == asset) {
                    holder.unbondings[i].amount = holder.unbondings[i].amount - amount;
                } else {
                    return Err(StdError::generic_err(
                        "Failed to get unbonding, shouldn't happen",
                    ));
                }
                Ok(holder)
            })?;

            return Ok(Response::new().add_messages(messages).set_data(to_binary(
                &adapter::ExecuteAnswer::Unbond {
                    status: ResponseStatus::Success,
                    amount,
                },
            )?));
        }
    }

    let full_asset = ASSETS.load(deps.storage, asset.clone())?;

    let mut allocations = ALLOCATIONS.load(deps.storage, asset.clone())?;

    // Build metadata
    let mut amount_total = Uint128::zero();
    let mut portion_total = Uint128::zero();

    // Gather adapter outstanding amounts
    for i in 0..allocations.len() {
        allocations[i].balance = adapter::balance_query(
            deps.querier,
            &full_asset.contract.address,
            allocations[i].contract.clone(),
        )?;

        match allocations[i].alloc_type {
            AllocationType::Amount => amount_total += allocations[i].balance,
            AllocationType::Portion => portion_total += allocations[i].balance,
        };
    }

    /*let allowance = allowance_query(
        &deps.querier,
        config.treasury.clone(),
        env.contract.address.clone(),
        VIEWING_KEY.load(deps.storage)?,
        1,
        &full_asset.contract.clone(),
    )?
    .allowance;

    let total = portion_total + allowance;*/

    let mut alloc_meta = vec![];
    let mut tot_unbond_available = Uint128::zero();

    for allocation in allocations.clone() {
        let bal = adapter::unbondable_query(deps.querier, &asset, allocation.contract.clone())?;

        alloc_meta.push(AllocationMeta {
            nick: allocation.nick,
            contract: allocation.contract,
            amount: allocation.amount,
            alloc_type: allocation.alloc_type,
            balance: bal,
            tolerance: Uint128::zero(),
        });
        tot_unbond_available += bal;
    }

    if unbond_amount == tot_unbond_available {
        for a in alloc_meta.clone() {
            messages.push(adapter::unbond_msg(
                &full_asset.contract.address.clone(),
                a.balance.clone(),
                a.contract.clone(),
            )?);
        }
        return Ok(Response::new().add_messages(messages).set_data(to_binary(
            &adapter::ExecuteAnswer::Unbond {
                status: ResponseStatus::Success,
                amount,
            },
        )?));
    }

    let mut total_amount_unbonding = Uint128::zero();

    let mut unbond_amounts = vec![];

    let portions = alloc_meta
        .clone()
        .into_iter()
        .filter(|a| a.alloc_type == AllocationType::Portion)
        .collect::<Vec<AllocationMeta>>();
    let amounts = alloc_meta
        .clone()
        .into_iter()
        .filter(|a| a.alloc_type == AllocationType::Amount)
        .collect::<Vec<AllocationMeta>>();

    for meta in amounts.clone() {
        if meta.balance > meta.amount {
            total_amount_unbonding += meta.balance - meta.amount;
            unbond_amounts.push(meta.balance - meta.amount);
        } else {
            unbond_amounts.push(Uint128::zero())
        }
    }

    if unbond_amount == total_amount_unbonding {
        println!(
            "885 UNBOND \t \t unbond_amount: {}, unbond_amounts: {:?}",
            unbond_amount, unbond_amounts
        );
        for (i, meta) in amounts.clone().iter().enumerate() {
            messages.push(adapter::unbond_msg(
                &full_asset.contract.address.clone(),
                unbond_amounts[i],
                meta.contract.clone(),
            )?);
        }
        return Ok(Response::new().add_messages(messages).set_data(to_binary(
            &adapter::ExecuteAnswer::Unbond {
                status: ResponseStatus::Success,
                amount,
            },
        )?));
    } else if unbond_amount < total_amount_unbonding {
        let mut modified_total_amount_unbonding = Uint128::zero();
        for (i, meta) in amounts.clone().iter().enumerate() {
            unbond_amounts[i] =
                unbond_amount.multiply_ratio(unbond_amounts[i], total_amount_unbonding);
            modified_total_amount_unbonding += unbond_amounts[i];
            if i == amounts.len() - 1 && modified_total_amount_unbonding < unbond_amount {
                unbond_amounts[i] += Uint128::new(1);
            }
            messages.push(adapter::unbond_msg(
                &full_asset.contract.address.clone(),
                unbond_amounts[i],
                meta.contract.clone(),
            )?);
        }
        println!(
            "921 UNBOND \t \t unbond_amount: {}, unbond_amounts: {:?}",
            unbond_amount, unbond_amounts
        );
        return Ok(Response::new().add_messages(messages).set_data(to_binary(
            &adapter::ExecuteAnswer::Unbond {
                status: ResponseStatus::Success,
                amount,
            },
        )?));
    }

    // if portion total > unbond - tot, we know the portion adapters can cover the rest
    println!(
        "{} {}",
        unbond_amount - total_amount_unbonding,
        portion_total
    );
    if unbond_amount - total_amount_unbonding < portion_total {
        for (i, meta) in amounts.clone().iter().enumerate() {
            if !unbond_amounts[i].is_zero() {
                messages.push(adapter::unbond_msg(
                    &full_asset.contract.address.clone(),
                    unbond_amounts[i],
                    meta.contract.clone(),
                )?);
            }
        }
        let amount_adapt_tot_unbonding = total_amount_unbonding;
        for (i, meta) in portions.clone().iter().enumerate() {
            let unbond_from_portion = (unbond_amount - amount_adapt_tot_unbonding)
                .multiply_ratio(meta.balance, portion_total);
            unbond_amounts.push(unbond_from_portion);
            total_amount_unbonding += unbond_from_portion;
            println!("unbond from portion: {}", unbond_from_portion);
            if i == portions.len() - 1 && total_amount_unbonding < unbond_amount {
                messages.push(adapter::unbond_msg(
                    &full_asset.contract.address.clone(),
                    unbond_from_portion + Uint128::new(1),
                    meta.contract.clone(),
                )?);
            } else if !unbond_from_portion.is_zero() {
                messages.push(adapter::unbond_msg(
                    &full_asset.contract.address.clone(),
                    unbond_from_portion,
                    meta.contract.clone(),
                )?);
            }
        }
        println!(
            "969 UNBOND \t \t unbond_amount: {}, unbond_amounts: {:?}",
            unbond_amount, unbond_amounts
        );
        return Ok(Response::new().add_messages(messages).set_data(to_binary(
            &adapter::ExecuteAnswer::Unbond {
                status: ResponseStatus::Success,
                amount,
            },
        )?));
    } else {
        // Otherwise we need to unbond everything from the portion adapters and go back to the
        // amount adapters
        for meta in portions {
            //TODO Unobond from poriton adapters
            unbond_amounts.push(meta.balance);
            messages.push(adapter::unbond_msg(
                &full_asset.contract.address,
                meta.balance,
                meta.contract,
            )?);
            total_amount_unbonding += meta.balance;
        }
        if total_amount_unbonding == unbond_amount {
            for (i, meta) in amounts.clone().iter().enumerate() {
                if !unbond_amounts[i].is_zero() {
                    messages.push(adapter::unbond_msg(
                        &full_asset.contract.address,
                        unbond_amounts[i].clone(),
                        meta.contract.clone(),
                    )?);
                }
            }
            println!(
                "914 UNBOND \t \t unbond_amount: {}, unbond_amounts: {:?}",
                unbond_amount, unbond_amounts
            );
            return Ok(Response::new().add_messages(messages).set_data(to_binary(
                &adapter::ExecuteAnswer::Unbond {
                    status: ResponseStatus::Success,
                    amount,
                },
            )?));
        } else {
            let mut amount_alloc = Uint128::zero();
            for meta in amounts.clone() {
                amount_alloc += meta.amount;
            }
            println!("{} {}", amount_alloc, total_amount_unbonding);
            let mut modified_total_amount_unbonding = total_amount_unbonding;
            for (i, meta) in amounts.iter().enumerate() {
                unbond_amounts[i] += (unbond_amount - total_amount_unbonding)
                    .multiply_ratio(meta.amount, amount_alloc);

                modified_total_amount_unbonding += meta.balance;
                if i == amounts.len() - 1
                    && modified_total_amount_unbonding < unbond_amount
                    && unbond_amount - modified_total_amount_unbonding
                        < meta.balance - unbond_amounts[i]
                {
                    unbond_amounts[i] += unbond_amount - total_amount_unbonding;
                }
                messages.push(adapter::unbond_msg(
                    &full_asset.contract.address,
                    unbond_amounts[i],
                    meta.contract.clone(),
                )?);
            }
            println!(
                "928 UNBOND \t \t unbond_amount: {}, unbond_amounts: {:?}",
                unbond_amount, unbond_amounts
            );
            return Ok(Response::new().add_messages(messages).set_data(to_binary(
                &adapter::ExecuteAnswer::Unbond {
                    status: ResponseStatus::Success,
                    amount,
                },
            )?));
        }
    }
}

pub fn add_holder(
    deps: DepsMut,
    env: &Env,
    info: MessageInfo,
    holder: Addr,
) -> StdResult<Response> {
    validate_admin(
        &deps.querier,
        AdminPermissions::TreasuryManager,
        &info.sender,
        &CONFIG.load(deps.storage)?.admin_auth,
    )?;

    //let holder = deps.api.addr_validate(holder.as_str())?;

    HOLDERS.update(deps.storage, |mut h| {
        if h.contains(&holder.clone()) {
            return Err(StdError::generic_err("Holding already exists"));
        }
        h.push(holder.clone());
        Ok(h)
    })?;

    HOLDING.save(deps.storage, holder, &Holding {
        balances: Vec::new(),
        unbondings: Vec::new(),
        status: Status::Active,
    })?;

    Ok(
        Response::new().set_data(to_binary(&ExecuteAnswer::AddHolder {
            status: ResponseStatus::Success,
        })?),
    )
}

pub fn remove_holder(
    deps: DepsMut,
    env: &Env,
    info: MessageInfo,
    holder: Addr,
) -> StdResult<Response> {
    // TODO: unbond all or move all funds to treasury?
    // Should probably disallow fully deleting holders, just freeze/transfer
    validate_admin(
        &deps.querier,
        AdminPermissions::TreasuryManager,
        &info.sender,
        &CONFIG.load(deps.storage)?.admin_auth,
    )?;

    //let holder = deps.api.addr_validate(holder.as_str())?;

    if let Some(mut holding) = HOLDING.may_load(deps.storage, holder.clone())? {
        holding.status = Status::Closed;
        HOLDING.save(deps.storage, holder, &holding)?;
    } else {
        return Err(StdError::generic_err("Not an authorized holder"));
    }

    Ok(
        Response::new().set_data(to_binary(&ExecuteAnswer::RemoveHolder {
            status: ResponseStatus::Success,
        })?),
    )
}
