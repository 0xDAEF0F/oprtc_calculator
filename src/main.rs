use ethers::{
    core::types::{Address, Filter, U256},
    providers::{Http, Middleware, Provider},
};
use eyre::Result;
use std::sync::Arc;

const HTTP_URL: &str = "https://rpc.flashbots.net";
const LENDING_VAULT_ADDRESS: &str = "0xaF53431488E871D103baA0280b6360998F0F9926";
const DEPOSIT_EVENT: &str = "Deposit(address,address,uint256,uint256)";
const WITHDRAW_EVENT: &str = "Withdraw(address,address,address,uint256,uint256)";
const BLOCK_CONTRACT_DEPLOYED: i32 = 17564663;

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

    let deposit_logs = client.get_logs(&deposit_filter).await?;
    let withdraw_logs = client.get_logs(&withdraw_filter).await?;

    println!("{} deposit logs found", deposit_logs.iter().len());
    println!("{} withdraw logs found", withdraw_logs.iter().len());

    for log in deposit_logs.iter() {
        let block_number = log.block_number.unwrap();
        let owner = Address::from(log.topics[2]);
        let assets = U256::from(&log.data[..32]);
        println!(
            "owner: {} — assets: {} — block: {}",
            owner, assets, block_number
        );
    }

    for log in withdraw_logs.iter() {
        let block_number = log.block_number.unwrap();
        let owner = Address::from(log.topics[3]);
        let assets = U256::from(&log.data[..32]);
        println!(
            "owner: {} — assets: {} — block: {}",
            owner, assets, block_number
        );
    }

    Ok(())
}
