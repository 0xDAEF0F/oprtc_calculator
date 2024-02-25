use ethers::{
    core::types::{Address, U256, U64},
    utils::parse_ether,
};
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
            .get_mut(&withdraw.address)
            .expect("user should exist");

        let rewards_accumulated = (self.total_rewards_per_share
            - user_record.rewards_per_share_snapshot)
            * user_record.shares_staked;

        user_record.rewards_accumulated += rewards_accumulated;
        user_record.shares_staked -= withdraw.shares;
        user_record.rewards_per_share_snapshot = self.total_rewards_per_share;

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

    pub fn preview_user_rewards(&self, user: Address, block_number: U64) -> U256 {
        let user_record = self.user_records.get(&user);

        if user_record.is_none() {
            return U256::from(0);
        }

        let user_record = user_record.unwrap();

        if self.total_shares_staked.is_zero() {
            let accrued_rewards = (self.total_rewards_per_share
                - user_record.rewards_per_share_snapshot)
                * user_record.shares_staked;
            let unclaimed_rewards = user_record.rewards_accumulated;
            return (accrued_rewards + unclaimed_rewards) / parse_ether("1").unwrap();
        }

        let rewards_per_block = parse_ether("1").unwrap();

        let pending_rewards =
            U256::from((block_number - self.last_accounted_block).as_u64()) * rewards_per_block;

        // increased by 1e18
        let pending_rewards_per_share_staked =
            pending_rewards * parse_ether("1").unwrap() / self.total_shares_staked;

        let user_rewards = (self.total_rewards_per_share + pending_rewards_per_share_staked
            - user_record.rewards_per_share_snapshot)
            * user_record.shares_staked;

        (user_rewards + user_record.rewards_accumulated) / parse_ether("1").unwrap()
    }

    pub fn get_all_rewards(&self, block_number: U64) -> U256 {
        let mut rewards = U256::from(0);
        for address in self.user_records.keys() {
            let reward = self.preview_user_rewards(*address, block_number);
            rewards += reward;
        }
        rewards
    }

    pub fn get_user_rewards(&self, block_number: U64) -> Vec<(Address, U256)> {
        let mut records: Vec<_> = self
            .user_records
            .keys()
            .map(|addr| {
                let rewards = self.preview_user_rewards(*addr, block_number);
                (*addr, rewards)
            })
            .filter(|(_, r)| !r.is_zero())
            .collect();

        records.sort_by_key(|&(_, num)| std::cmp::Reverse(num));

        records
    }

    fn distribute_rewards(&mut self, block_number: U64) {
        if self.last_accounted_block >= block_number || self.total_shares_staked == U256::from(0) {
            return;
        }

        let blocks_transcurred = U256::from((block_number - self.last_accounted_block).as_u64());
        let rewards_per_block = parse_ether("1").unwrap();

        let pending_rewards = blocks_transcurred * rewards_per_block;

        let pending_rewards_per_share =
            pending_rewards * parse_ether("1").unwrap() / self.total_shares_staked;

        self.last_accounted_block = block_number;
        self.total_rewards_per_share += pending_rewards_per_share;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethers::utils::{parse_units, ParseUnits};

    const BOB: &str = "0x0000000000000000000000000000000000000B0b";
    const ALICE: &str = "0x00000000000000000000000000000000000A11cE";

    fn create_events() -> Vec<Event> {
        let _one_ether: ParseUnits = parse_units("1.0", 18).unwrap();

        let evt_one = Event::Deposit(Deposit {
            address: BOB.parse().unwrap(),
            shares: parse_ether("1").unwrap(),
            block_number: U64::from(BLOCK_CONTRACT_DEPLOYED),
        });

        let evt_two = Event::Deposit(Deposit {
            address: ALICE.parse().unwrap(),
            shares: parse_ether("1").unwrap(),
            block_number: U64::from(BLOCK_CONTRACT_DEPLOYED + 100),
        });

        let events: Vec<Event> = vec![evt_one, evt_two];

        events
    }

    #[test]
    fn distributes_rewards_correctly() {
        let events = create_events();

        let mut global_state = GlobalState::new();

        global_state.process_events(events);

        let block_number = U64::from(BLOCK_CONTRACT_DEPLOYED + 100);

        let bob_rewards = global_state.preview_user_rewards(BOB.parse().unwrap(), block_number);
        let alice_rewards = global_state.preview_user_rewards(ALICE.parse().unwrap(), block_number);

        assert_eq!(bob_rewards, parse_ether("100").unwrap());
        assert_eq!(alice_rewards, parse_ether("0").unwrap());

        let all_rewards = global_state.get_all_rewards(block_number);
        assert_eq!(all_rewards, parse_ether("100").unwrap());
    }
}
