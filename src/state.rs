use ethers::core::types::{Address, U256, U64};
use std::collections::HashMap;

pub const BLOCK_CONTRACT_DEPLOYED: u64 = 17564663;

#[derive(Debug)]
pub struct Deposit {
    pub address: Address,
    pub shares: U256,
    pub block_number: U64,
}

#[derive(Debug)]
pub struct Withdraw {
    pub address: Address,
    pub shares: U256,
    pub block_number: U64,
}

#[derive(Debug)]
pub struct Transfer {
    pub from: Address,
    pub to: Address,
    pub shares: U256,
    pub block_number: U64,
}

#[derive(Debug)]
pub enum Event {
    Deposit(Deposit),
    Withdrawal(Withdraw),
    Transfer(Transfer),
}

#[derive(Debug)]
struct UserRecord {
    shares_staked: U256,
    rewards_per_share_snapshot: U256,
    rewards_accumulated: U256,
}

#[derive(Debug)]
pub struct GlobalState {
    user_records: HashMap<Address, UserRecord>,
    total_shares_staked: U256,
    total_rewards_per_share: U256,
    last_accounted_block: U64,
}

impl GlobalState {
    pub fn new() -> GlobalState {
        GlobalState {
            user_records: HashMap::new(),
            total_shares_staked: U256::from(0),
            total_rewards_per_share: U256::from(0),
            last_accounted_block: U64::from(BLOCK_CONTRACT_DEPLOYED),
        }
    }

    pub fn process_events(&mut self, evts: Vec<Event>) {
        for evt in evts.into_iter() {
            match evt {
                Event::Deposit(deposit) => self.process_deposit(deposit),
                Event::Withdrawal(withdrawal) => self.process_withdraw(withdrawal),
                Event::Transfer(transfer) => self.process_transfer(transfer),
            }
        }
    }

    fn process_deposit(&mut self, deposit: Deposit) {
        self.distribute_rewards(deposit.block_number);

        if let Some(user) = self.user_records.get(&deposit.address) {
            let accrued_rewards = (self.total_rewards_per_share - user.rewards_per_share_snapshot)
                * user.shares_staked;

            let user_record = UserRecord {
                shares_staked: user.shares_staked + deposit.shares,
                rewards_accumulated: user.rewards_accumulated + accrued_rewards,
                rewards_per_share_snapshot: self.total_rewards_per_share,
            };

            self.user_records.insert(deposit.address, user_record);
        } else {
            self.user_records.insert(
                deposit.address,
                UserRecord {
                    shares_staked: deposit.shares,
                    rewards_accumulated: U256::from(0),
                    rewards_per_share_snapshot: self.total_rewards_per_share,
                },
            );
        }

        self.total_shares_staked += deposit.shares;
    }

    fn process_withdraw(&mut self, withdraw: Withdraw) {
        self.distribute_rewards(withdraw.block_number);

        let user_record = self
            .user_records
            .get(&withdraw.address)
            .expect("user should exist");

        let rewards_accumulated = (self.total_rewards_per_share
            - user_record.rewards_per_share_snapshot)
            * user_record.shares_staked;

        self.user_records.insert(
            withdraw.address,
            UserRecord {
                shares_staked: user_record.shares_staked - withdraw.shares,
                rewards_per_share_snapshot: self.total_rewards_per_share,
                rewards_accumulated: user_record.rewards_accumulated + rewards_accumulated,
            },
        );

        self.total_shares_staked -= withdraw.shares;
    }

    fn process_transfer(&mut self, transfer: Transfer) {
        let withdrawal = Withdraw {
            address: transfer.from,
            shares: transfer.shares,
            block_number: transfer.block_number,
        };

        let deposit = Deposit {
            address: transfer.to,
            shares: transfer.shares,
            block_number: transfer.block_number,
        };

        self.process_withdraw(withdrawal);
        self.process_deposit(deposit);
    }

    fn distribute_rewards(&mut self, block_number: U64) {
        if self.last_accounted_block >= block_number || self.total_shares_staked == U256::from(0) {
            return;
        }

        let blocks_transcurred = U256::from((block_number - self.last_accounted_block).as_u64());
        // 1e27
        let rewards_per_block = U256::from("1000000000000000000000000000");
        let pending_rewards: U256 = blocks_transcurred * rewards_per_block;

        let pending_rewards_per_share = pending_rewards / self.total_shares_staked;

        self.last_accounted_block = block_number;
        self.total_rewards_per_share += pending_rewards_per_share;
    }
}
