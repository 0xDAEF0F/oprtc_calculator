use crate::state::{Deposit, Event, GlobalState, Transfer, Withdraw, BLOCK_CONTRACT_DEPLOYED};
use ethers::{
    core::types::{Address, Filter, U256},
    providers::{Http, Middleware, Provider},
    utils::{format_ether, parse_ether},
};
use eyre::Result;
use std::sync::Arc;
mod state;

const HTTP_URL: &str = "https://rpc.flashbots.net";
const LENDING_VAULT_ADDRESS: &str = "0xaF53431488E871D103baA0280b6360998F0F9926";
const DEPOSIT_EVENT: &str = "Deposit(address,address,uint256,uint256)";
const WITHDRAW_EVENT: &str = "Withdraw(address,address,address,uint256,uint256)";
const TRANSFER_EVENT: &str = "Transfer(address,address,uint256)";

#[tokio::main]
async fn main() -> Result<()> {
    let provider = Provider::<Http>::try_from(HTTP_URL)?;
    let client = Arc::new(provider);

    let deposit_filter = Filter::new()
        .address(LENDING_VAULT_ADDRESS.parse::<Address>()?)
        .event(DEPOSIT_EVENT)
        .from_block(BLOCK_CONTRACT_DEPLOYED);

    let withdraw_filter = Filter::new()
        .address(LENDING_VAULT_ADDRESS.parse::<Address>()?)
        .event(WITHDRAW_EVENT)
        .from_block(BLOCK_CONTRACT_DEPLOYED);

    let transfer_filter = Filter::new()
        .address(LENDING_VAULT_ADDRESS.parse::<Address>()?)
        .event(TRANSFER_EVENT)
        .from_block(BLOCK_CONTRACT_DEPLOYED);

    let deposit_logs = client
        .get_logs(&deposit_filter)
        .await?
        .into_iter()
        .map(|log| {
            Event::Deposit(Deposit {
                address: Address::from(log.topics[2]),
                block_number: log.block_number.unwrap(),
                shares: U256::from(&log.data[32..]),
            })
        });

    let withdraw_logs = client
        .get_logs(&withdraw_filter)
        .await?
        .into_iter()
        .map(|log| {
            Event::Withdrawal(Withdraw {
                address: Address::from(log.topics[3]),
                block_number: log.block_number.unwrap(),
                shares: U256::from(&log.data[32..]),
            })
        });

    let transfer_logs = client
        .get_logs(&transfer_filter)
        .await?
        .into_iter()
        .flat_map(|log| {
            let from = Address::from(log.topics[1]);
            let to = Address::from(log.topics[2]);

            if from.is_zero() || to.is_zero() {
                vec![]
            } else {
                vec![Event::Transfer(Transfer {
                    from,
                    to,
                    shares: U256::from(&log.data[..]),
                    block_number: log.block_number.unwrap(),
                })]
            }
        });

    let mut all_events: Vec<Event> = deposit_logs
        .chain(withdraw_logs)
        .chain(transfer_logs)
        .collect();

    all_events.sort_by(|a, b| {
        let block_a = match a {
            Event::Deposit(e) => e.block_number,
            Event::Withdrawal(e) => e.block_number,
            Event::Transfer(e) => e.block_number,
        };
        let block_b = match b {
            Event::Deposit(e) => e.block_number,
            Event::Withdrawal(e) => e.block_number,
            Event::Transfer(e) => e.block_number,
        };

        block_a.cmp(&block_b)
    });

    let mut global_state = GlobalState::new();
    global_state.process_events(all_events);

    let curr_block_number = client.get_block_number().await?;

    let total_rewards_expected = U256::from((curr_block_number - BLOCK_CONTRACT_DEPLOYED).as_u64())
        * parse_ether("1").unwrap();
    let total_rewards = global_state.get_all_rewards(curr_block_number);

    let total_rewards_expected = format_ether(total_rewards_expected);
    let total_rewards_given = format_ether(total_rewards);

    println!("total_rewards_expected: {}", total_rewards_expected);
    println!("total_rewards_given: {}", total_rewards_given);

    let all_user_rewards = global_state.get_user_rewards(curr_block_number);

    let total_rewards_given: f64 = total_rewards_given.parse().unwrap();
    let mut max_pct: f64 = 0.0;
    for (addr, rewards) in all_user_rewards {
        let rewards: f64 = format_ether(rewards).parse().unwrap();
        let pct = rewards * 100.0 / total_rewards_given;
        max_pct += pct;
        println!("{} — {}", addr, pct);
    }

    println!("Total %: {}", max_pct);

    Ok(())
}
