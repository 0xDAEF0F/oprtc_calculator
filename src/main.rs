use ethers::{
    core::types::{Address, Filter, U256, U64},
    providers::{Http, Middleware, Provider},
};
use eyre::Result;
use std::collections::HashMap;
use std::sync::Arc;

const HTTP_URL: &str = "https://rpc.flashbots.net";
const LENDING_VAULT_ADDRESS: &str = "0xaF53431488E871D103baA0280b6360998F0F9926";
const DEPOSIT_EVENT: &str = "Deposit(address,address,uint256,uint256)";
const WITHDRAW_EVENT: &str = "Withdraw(address,address,address,uint256,uint256)";
const BLOCK_CONTRACT_DEPLOYED: i32 = 17564663;

#[derive(Debug)]
enum DepositWithdrawalEvent {
    Deposit(Event),
    Withdrawal(Event),
}

#[derive(Debug)]
struct Event {
    address: Address,
    shares: U256,
    block_number: U64,
}

#[derive(Debug)]
struct UserRecord {
    shares_staked: U256,
    rewards_per_share_snapshot: U256,
    rewards_accumulated: U256,
}

#[derive(Debug)]
struct GlobalState<'a> {
    user_records: &'a mut HashMap<Address, UserRecord>,
    total_shares_staked: U256,
    total_rewards_per_share: U256,
    last_accounted_block: U64,
}

impl GlobalState<'_> {
    pub fn process_events(&mut self, evts: Vec<DepositWithdrawalEvent>) {
        for evt in evts.into_iter() {
            match evt {
                DepositWithdrawalEvent::Deposit(e) => self.process_deposit(e),
                DepositWithdrawalEvent::Withdrawal(e) => self.process_withdraw(e),
            }
        }
    }

    fn process_deposit(&mut self, evt: Event) {
        if self.total_shares_staked != U256::from(0) {
            self.update_rewards(evt.block_number)
        }

        if let Some(user) = self.user_records.get(&evt.address) {
            let accrued_rewards = (self.total_rewards_per_share - user.rewards_per_share_snapshot)
                * user.shares_staked;

            let user_record = UserRecord {
                shares_staked: user.shares_staked + evt.shares,
                rewards_accumulated: user.rewards_accumulated + accrued_rewards,
                rewards_per_share_snapshot: self.total_rewards_per_share,
            };

            self.user_records.insert(evt.address, user_record);
        } else {
            self.user_records.insert(
                evt.address,
                UserRecord {
                    shares_staked: evt.shares,
                    rewards_accumulated: U256::from(0),
                    rewards_per_share_snapshot: self.total_rewards_per_share,
                },
            );
        }

        self.total_shares_staked += evt.shares;
    }

    fn process_withdraw(&mut self, evt: Event) {
        if self.total_shares_staked != U256::from(0) {
            self.update_rewards(evt.block_number)
        }

        let user_record = self
            .user_records
            .get(&evt.address)
            .expect("user should exist");

        let rewards_accumulated = (self.total_rewards_per_share
            - user_record.rewards_per_share_snapshot)
            * user_record.shares_staked;

        self.user_records.insert(
            evt.address,
            UserRecord {
                shares_staked: user_record.shares_staked - evt.shares,
                rewards_per_share_snapshot: self.total_rewards_per_share,
                rewards_accumulated: user_record.rewards_accumulated + rewards_accumulated,
            },
        );

        self.total_shares_staked -= evt.shares;
    }

    fn update_rewards(&mut self, block_number: U64) {
        assert!(block_number >= self.last_accounted_block);
        assert!(self.total_shares_staked != 0.into());

        let blocks_transcurred = U256::from((block_number - self.last_accounted_block).as_u64());
        // 1e27
        let rewards_per_block = U256::from("1000000000000000000000000000");
        let pending_rewards: U256 = blocks_transcurred * rewards_per_block;

        let pending_rewards_per_share = pending_rewards / self.total_shares_staked;

        self.last_accounted_block = block_number;
        self.total_rewards_per_share += pending_rewards_per_share;
    }
}

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

    let deposit_logs = client
        .get_logs(&deposit_filter)
        .await?
        .into_iter()
        .map(|log| {
            DepositWithdrawalEvent::Deposit(Event {
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
            DepositWithdrawalEvent::Withdrawal(Event {
                address: Address::from(log.topics[3]),
                block_number: log.block_number.unwrap(),
                shares: U256::from(&log.data[32..]),
            })
        });

    let mut a: Vec<DepositWithdrawalEvent> = deposit_logs.chain(withdraw_logs).collect();
    let mut hm: HashMap<Address, UserRecord> = HashMap::new();
    let mut gs = GlobalState {
        total_shares_staked: U256::from(0),
        total_rewards_per_share: U256::from(0),
        last_accounted_block: U64::from(17564663),
        user_records: &mut hm,
    };

    a.sort_by(|a, b| {
        let block_a = match a {
            DepositWithdrawalEvent::Deposit(event) => event.block_number,
            DepositWithdrawalEvent::Withdrawal(event) => event.block_number,
        };
        let block_b = match b {
            DepositWithdrawalEvent::Deposit(event) => event.block_number,
            DepositWithdrawalEvent::Withdrawal(event) => event.block_number,
        };

        block_a.cmp(&block_b)
    });

    gs.process_events(a);

    println!("{:?}", gs);

    Ok(())
}
