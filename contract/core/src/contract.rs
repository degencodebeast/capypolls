use cosmwasm_std::{
    entry_point, to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response, StdResult, SubMsg,
    Uint128, WasmMsg,
};
use cw2::set_contract_version;
use cw20::Cw20ExecuteMsg;

use crate::{
    error::ContractError,
    msg::{
        ConfigResponse, ExecuteMsg, InstantiateMsg, PollCountResponse, PollDetailsResponse, PollResponse,
        QueryMsg,
    },
    state::{Config, PollInfo, TempPollData, CONFIG, POLLS, POLL_COUNT, POLL_SEQUENCE, TEMP_POLL_DATA},
};

const CONTRACT_NAME: &str = "crates.io:xion-capypolls-core";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

const MIN_DURATION: u64 = 60; // 1 minute
const MAX_DURATION: u64 = 2592000; // 30 days
const MAX_PROTOCOL_FEE: u64 = 1000; // 10%

// Reply IDs
const REPLY_YES_TOKEN_INIT: u64 = 1;
const REPLY_NO_TOKEN_INIT: u64 = 2;
const REPLY_POLL_INIT: u64 = 3;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    if msg.protocol_fee > MAX_PROTOCOL_FEE {
        return Err(ContractError::InvalidFee("Protocol fee too high".to_string()));
    }

    let config = Config {
        owner: info.sender.clone(),
        initial_fee: msg.initial_fee,
        protocol_fee: msg.protocol_fee,
        poll_code_id: msg.poll_code_id,
        token_code_id: msg.token_code_id,
        denom: "uxion".to_string(),
    };

    CONFIG.save(deps.storage, &config)?;
    POLL_COUNT.save(deps.storage, &0u64)?;

    Ok(Response::new()
        .add_attribute("action", "instantiate")
        .add_attribute("owner", info.sender))
}

pub fn execute_create_poll(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    question: String,
    avatar: String,
    description: String,
    duration: u64,
    yes_token_name: String,
    yes_token_symbol: String,
    no_token_name: String,
    no_token_symbol: String,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    
    // Validate XION fee payment
    let payment = info.funds
        .iter()
        .find(|coin| coin.denom == "uxion")
        .ok_or(ContractError::NoXionPayment {})?;

    if payment.amount != config.initial_fee {
        return Err(ContractError::InvalidFeeAmount {});
    }

    // Validate duration
    if duration < MIN_DURATION || duration > MAX_DURATION {
        return Err(ContractError::InvalidDuration {
            min: MIN_DURATION,
            max: MAX_DURATION,
        });
    }

    // Save temporary data for reply handler
    let temp_data = TempPollData {
        creator: info.sender.clone(),
        question: question.clone(),
        avatar,
        description,
        yes_token: None,
        no_token: None,
        poll_addr: None,
        duration,
    };
    TEMP_POLL_DATA.save(deps.storage, &temp_data)?;

    // Create YES token
    let yes_token_init = cw20_base::msg::InstantiateMsg {
        name: yes_token_name,
        symbol: yes_token_symbol,
        decimals: 18,
        initial_balances: vec![],
        mint: Some(cw20::MinterResponse {
            minter: info.sender.to_string(),
            cap: None,
        }),
        marketing: None,
    };

    let yes_token_instantiate = SubMsg::reply_on_success(
        WasmMsg::Instantiate {
            admin: Some(info.sender.to_string()),
            code_id: config.token_code_id,
            msg: to_json_binary(&yes_token_init)?,
            funds: vec![],
            label: format!("YES Token for Poll {}", question),
        },
        REPLY_YES_TOKEN_INIT,
    );

    Ok(Response::new()
        .add_submessage(yes_token_instantiate)
        .add_attribute("action", "create_poll")
        .add_attribute("creator", info.sender))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::CreatePoll {
            question,
            avatar,
            description,
            duration,
            yes_token_name,
            yes_token_symbol,
            no_token_name,
            no_token_symbol,
        } => execute_create_poll(
            deps,
            env,
            info,
            question,
            avatar,
            description,
            duration,
            yes_token_name,
            yes_token_symbol,
            no_token_name,
            no_token_symbol,
        ),
        ExecuteMsg::UpdatePollCodeId { code_id } => {
            execute_update_poll_code_id(deps, info, code_id)
        }
        ExecuteMsg::UpdateTokenCodeId { code_id } => {
            execute_update_token_code_id(deps, info, code_id)
        }
        ExecuteMsg::SetInitialFee { new_fee } => execute_set_initial_fee(deps, info, new_fee),
        ExecuteMsg::SetProtocolFee { new_fee } => execute_set_protocol_fee(deps, info, new_fee),
        ExecuteMsg::WithdrawFees { to } => execute_withdraw_fees(deps, env, info, to),
    }
}

pub fn execute_update_poll_code_id(
    deps: DepsMut,
    info: MessageInfo,
    code_id: u64,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    let old_code_id = config.poll_code_id;
    config.poll_code_id = code_id;
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new()
        .add_attribute("action", "update_poll_code_id")
        .add_attribute("old_code_id", old_code_id.to_string())
        .add_attribute("new_code_id", code_id.to_string()))
}

pub fn execute_update_token_code_id(
    deps: DepsMut,
    info: MessageInfo,
    code_id: u64,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    let old_code_id = config.token_code_id;
    config.token_code_id = code_id;
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new()
        .add_attribute("action", "update_token_code_id")
        .add_attribute("old_code_id", old_code_id.to_string())
        .add_attribute("new_code_id", code_id.to_string()))
}

pub fn execute_set_initial_fee(
    deps: DepsMut,
    info: MessageInfo,
    new_fee: Uint128,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    let old_fee = config.initial_fee;
    config.initial_fee = new_fee;
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new()
        .add_attribute("action", "update_initial_fee")
        .add_attribute("old_fee", old_fee)
        .add_attribute("new_fee", new_fee))
}

pub fn execute_set_protocol_fee(
    deps: DepsMut,
    info: MessageInfo,
    new_fee: u64,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    if new_fee > MAX_PROTOCOL_FEE {
        return Err(ContractError::InvalidFee("Fee too high".to_string()));
    }

    let old_fee = config.protocol_fee;
    config.protocol_fee = new_fee;
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new()
        .add_attribute("action", "update_protocol_fee")
        .add_attribute("old_fee", old_fee.to_string())
        .add_attribute("new_fee", new_fee.to_string()))
}

pub fn execute_withdraw_fees(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    to: String,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    let to_addr = deps.api.addr_validate(&to)?;
    
    // Query contract's XION balance
    let balance = deps.querier.query_balance(env.contract.address, "uxion")?;
    let amount = balance.amount;

    // Send XION tokens to recipient
    let bank_msg = cosmwasm_std::BankMsg::Send {
        to_address: to_addr.to_string(),
        amount: vec![balance],
    };

    Ok(Response::new()
        .add_message(bank_msg)
        .add_attribute("action", "withdraw_fees")
        .add_attribute("to", to)
        .add_attribute("amount", amount))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetConfig {} => to_json_binary(&query_config(deps)?),
        QueryMsg::GetPollCount {} => to_json_binary(&query_poll_count(deps)?),
        QueryMsg::GetPollAt { index } => to_json_binary(&query_poll_at(deps, index)?),
        QueryMsg::IsPollFromFactory { poll_address } => {
            to_json_binary(&query_is_poll_from_factory(deps, poll_address)?)
        }
        QueryMsg::GetPollDetails { poll_address } => {
            to_json_binary(&query_poll_details(deps, poll_address)?)
        }
    }
}

fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config = CONFIG.load(deps.storage)?;
    Ok(ConfigResponse {
        owner: config.owner.to_string(),
        initial_fee: config.initial_fee,
        protocol_fee: config.protocol_fee,
        poll_code_id: config.poll_code_id,
        token_code_id: config.token_code_id,
        denom: config.denom,
    })
}

fn query_poll_count(deps: Deps) -> StdResult<PollCountResponse> {
    let count = POLL_COUNT.load(deps.storage)?;
    Ok(PollCountResponse { count })
}

fn query_poll_at(deps: Deps, index: u64) -> StdResult<PollResponse> {
    let count = POLL_COUNT.load(deps.storage)?;
    if index >= count {
        return Err(cosmwasm_std::StdError::generic_err("Index out of bounds"));
    }

    let poll_addr = POLL_SEQUENCE.load(deps.storage, index)?;
    Ok(PollResponse {
        address: poll_addr.to_string(),
    })
}

fn query_is_poll_from_factory(deps: Deps, poll_address: String) -> StdResult<bool> {
    let addr = deps.api.addr_validate(&poll_address)?;
    Ok(POLLS.has(deps.storage, &addr))
}

fn query_poll_details(deps: Deps, poll_address: String) -> StdResult<PollDetailsResponse> {
    let addr = deps.api.addr_validate(&poll_address)?;
    let poll_info = POLLS.may_load(deps.storage, &addr)?;

    Ok(PollDetailsResponse {
        exists: poll_info.is_some(),
        description: poll_info.map(|p| p.description),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info, MockApi, MockQuerier, MockStorage};
    use cosmwasm_std::{coins, Coin, OwnedDeps};

    fn setup_contract() -> OwnedDeps<MockStorage, MockApi, MockQuerier> {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            initial_fee: Uint128::new(1000000),
            protocol_fee: 100,
            poll_code_id: 1,
            token_code_id: 2,
        };
        let info = mock_info("creator", &coins(2, "token"));

        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
        deps
    }

    #[test]
    fn proper_initialization() {
        let deps = setup_contract();
        
        let config = CONFIG.load(deps.as_ref().storage).unwrap();
        assert_eq!(config.owner.as_str(), "creator");
        assert_eq!(config.initial_fee, Uint128::new(1000000));
        assert_eq!(config.protocol_fee, 100);
        assert_eq!(config.poll_code_id, 1);
        assert_eq!(config.token_code_id, 2);
        assert_eq!(config.denom, "uxion");
    }

    #[test]
    fn test_create_poll() {
        let mut deps = setup_contract();
        
        let info = mock_info(
            "creator", 
            &[Coin {
                denom: "uxion".to_string(),
                amount: Uint128::new(1000000),
            }]
        );
        
        let msg = ExecuteMsg::CreatePoll {
            question: "Test Poll?".to_string(),
            avatar: "avatar_url".to_string(),
            description: "Test Description".to_string(),
            yes_token_name: "YES".to_string(),
            yes_token_symbol: "YES".to_string(),
            no_token_name: "NO".to_string(),
            no_token_symbol: "NO".to_string(),
            duration: 1000,
        };

        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(1, res.messages.len());
    }
} 