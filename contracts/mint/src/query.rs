use crate::{
    handle::{calculate_portion, mint_amount},
    state::{
        asset_list_r,
        asset_peg_r,
        assets_r,
        config_r,
        limit_r,
        limit_refresh_r,
        minted_r,
        native_asset_r,
        total_burned_r,
    },
};
use chrono::prelude::*;
use shade_protocol::c_std::Uint128;
use shade_protocol::c_std::{Api, DepsMut, Addr, Querier, StdError, StdResult, Storage};
use shade_protocol::contract_interfaces::mint::mint::QueryAnswer;

pub fn native_asset<S: Storage, A: Api, Q: Querier>(
    deps: Deps,
) -> StdResult<QueryAnswer> {
    Ok(QueryAnswer::NativeAsset {
        asset: native_asset_r(&deps.storage).load()?,
        peg: asset_peg_r(&deps.storage).load()?,
    })
}

pub fn supported_assets<S: Storage, A: Api, Q: Querier>(
    deps: Deps,
) -> StdResult<QueryAnswer> {
    Ok(QueryAnswer::SupportedAssets {
        assets: asset_list_r(&deps.storage).load()?,
    })
}

pub fn asset<S: Storage, A: Api, Q: Querier>(
    deps: Deps,
    contract: String,
) -> StdResult<QueryAnswer> {
    let assets = assets_r(&deps.storage);

    match assets.may_load(contract.as_bytes())? {
        Some(asset) => Ok(QueryAnswer::Asset {
            asset,
            burned: total_burned_r(&deps.storage).load(contract.as_bytes())?,
        }),
        None => Err(StdError::NotFound {
            kind: contract,
            backtrace: None,
        }),
    }
}

pub fn config<S: Storage, A: Api, Q: Querier>(deps: Deps) -> StdResult<QueryAnswer> {
    Ok(QueryAnswer::Config {
        config: config_r(&deps.storage).load()?,
    })
}

pub fn limit<S: Storage, A: Api, Q: Querier>(deps: Deps) -> StdResult<QueryAnswer> {
    Ok(QueryAnswer::Limit {
        minted: minted_r(&deps.storage).load()?,
        limit: limit_r(&deps.storage).load()?,
        last_refresh: limit_refresh_r(&deps.storage).load()?,
    })
}

pub fn mint<S: Storage, A: Api, Q: Querier>(
    deps: Deps,
    offer_asset: Addr,
    amount: Uint128,
) -> StdResult<QueryAnswer> {
    let native_asset = native_asset_r(&deps.storage).load()?;

    match assets_r(&deps.storage).may_load(offer_asset.to_string().as_bytes())? {
        Some(asset) => {
            //let fee = calculate_portion(amount, asset.fee);
            //let amount = mint_amount(deps, amount.checked_sub(fee)?, &asset, &native_asset)?;
            let amount = mint_amount(deps, amount, &asset, &native_asset)?;
            Ok(QueryAnswer::Mint {
                asset: native_asset.contract,
                amount,
            })
        }
        None => Err(StdError::NotFound {
            kind: offer_asset.to_string(),
            backtrace: None,
        }),
    }
}
