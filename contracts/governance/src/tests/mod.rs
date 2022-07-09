pub mod handle;
pub mod query;

use crate::contract::{handle, init, query};
use contract_harness::harness;
use shade_protocol::{
    c_std::{
        from_binary,
        to_binary,
        Binary,
        Env,
        HandleResponse,
        HumanAddr,
        InitResponse,
        StdError,
        StdResult,
    },
    contract_interfaces::{
        governance,
        governance::{
            assembly::{Assembly, AssemblyMsg},
            contract::AllowedContract,
            profile::Profile,
            proposal::{Proposal, ProposalMsg},
            Config,
        },
        query_auth,
    },
    fadroma::{
        core::ContractLink,
        ensemble::{ContractEnsemble, ContractHarness, MockDeps, MockEnv},
    },
    math_compat::Uint128,
    serde::Serialize,
    utils::asset::Contract,
};

pub fn init_query_auth(chain: &mut ContractEnsemble) -> StdResult<ContractLink<HumanAddr>> {
    let admin = harness::admin::init(chain, &shade_admin::admin::InitMsg {})?;

    let msg = &query_auth::InitMsg {
        admin_auth: Contract {
            address: admin.address.clone(),
            code_hash: admin.code_hash.clone(),
        },
        prng_seed: Binary::from("random".as_bytes()),
    };

    harness::query_auth::init(chain, &msg)
}

pub fn admin_only_governance() -> StdResult<(ContractEnsemble, ContractLink<HumanAddr>)> {
    let mut chain = ContractEnsemble::new(50);
    let auth = init_query_auth(&mut chain)?;

    let msg = governance::InitMsg {
        treasury: HumanAddr("treasury".to_string()),
        query_auth: Contract {
            address: auth.address,
            code_hash: auth.code_hash,
        },
        admin_members: vec![HumanAddr("admin".to_string())],
        admin_profile: Profile {
            name: "admin".to_string(),
            enabled: true,
            assembly: None,
            funding: None,
            token: None,
            cancel_deadline: 0,
        },
        public_profile: Profile {
            name: "public".to_string(),
            enabled: false,
            assembly: None,
            funding: None,
            token: None,
            cancel_deadline: 0,
        },
        funding_token: None,
        vote_token: None,
    };

    let gov = harness::governance::init(&mut chain, &msg)?;

    Ok((chain, gov))
}

pub fn gov_generic_proposal(
    chain: &mut ContractEnsemble,
    gov: &ContractLink<HumanAddr>,
    sender: &str,
    msg: governance::HandleMsg,
) -> StdResult<()> {
    gov_msg_proposal(chain, gov, sender, vec![ProposalMsg {
        target: Uint128::zero(),
        assembly_msg: Uint128::zero(),
        msg: to_binary(&vec![serde_json::to_string(&msg).unwrap()])?,
        send: vec![],
    }])
}

pub fn gov_msg_proposal(
    chain: &mut ContractEnsemble,
    gov: &ContractLink<HumanAddr>,
    sender: &str,
    msgs: Vec<ProposalMsg>,
) -> StdResult<()> {
    chain.execute(
        &governance::HandleMsg::AssemblyProposal {
            assembly: Uint128::new(1),
            title: "Title".to_string(),
            metadata: "Proposal metadata".to_string(),
            msgs: Some(msgs),
            padding: None,
        },
        MockEnv::new(sender, gov.clone()),
    )?;
    Ok(())
}

pub fn get_assembly_msgs(
    chain: &mut ContractEnsemble,
    gov: &ContractLink<HumanAddr>,
    start: Uint128,
    end: Uint128,
) -> StdResult<Vec<AssemblyMsg>> {
    let query: governance::QueryAnswer =
        chain.query(gov.address.clone(), &governance::QueryMsg::AssemblyMsgs {
            start,
            end,
        })?;

    let msgs = match query {
        governance::QueryAnswer::AssemblyMsgs { msgs } => msgs,
        _ => return Err(StdError::generic_err("Returned wrong enum")),
    };

    Ok(msgs)
}

pub fn get_contract(
    chain: &mut ContractEnsemble,
    gov: &ContractLink<HumanAddr>,
    start: Uint128,
    end: Uint128,
) -> StdResult<Vec<AllowedContract>> {
    let query: governance::QueryAnswer =
        chain.query(gov.address.clone(), &governance::QueryMsg::Contracts {
            start,
            end,
        })?;

    match query {
        governance::QueryAnswer::Contracts { contracts } => Ok(contracts),
        _ => return Err(StdError::generic_err("Returned wrong enum")),
    }
}

pub fn get_profiles(
    chain: &mut ContractEnsemble,
    gov: &ContractLink<HumanAddr>,
    start: Uint128,
    end: Uint128,
) -> StdResult<Vec<Profile>> {
    let query: governance::QueryAnswer =
        chain.query(gov.address.clone(), &governance::QueryMsg::Profiles {
            start,
            end,
        })?;

    match query {
        governance::QueryAnswer::Profiles { profiles } => Ok(profiles),
        _ => return Err(StdError::generic_err("Returned wrong enum")),
    }
}

pub fn get_assemblies(
    chain: &mut ContractEnsemble,
    gov: &ContractLink<HumanAddr>,
    start: Uint128,
    end: Uint128,
) -> StdResult<Vec<Assembly>> {
    let query: governance::QueryAnswer =
        chain.query(gov.address.clone(), &governance::QueryMsg::Assemblies {
            start,
            end,
        })?;

    match query {
        governance::QueryAnswer::Assemblies { assemblies } => Ok(assemblies),
        _ => return Err(StdError::generic_err("Returned wrong enum")),
    }
}

pub fn get_proposals(
    chain: &mut ContractEnsemble,
    gov: &ContractLink<HumanAddr>,
    start: Uint128,
    end: Uint128,
) -> StdResult<Vec<Proposal>> {
    let query: governance::QueryAnswer =
        chain.query(gov.address.clone(), &governance::QueryMsg::Proposals {
            start,
            end,
        })?;

    match query {
        governance::QueryAnswer::Proposals { props } => Ok(props),
        _ => return Err(StdError::generic_err("Returned wrong enum")),
    }
}

pub fn get_config(
    chain: &mut ContractEnsemble,
    gov: &ContractLink<HumanAddr>,
) -> StdResult<Config> {
    let query: governance::QueryAnswer =
        chain.query(gov.address.clone(), &governance::QueryMsg::Config {})?;

    match query {
        governance::QueryAnswer::Config { config } => Ok(config),
        _ => return Err(StdError::generic_err("Returned wrong enum")),
    }
}
