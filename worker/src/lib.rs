use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::U128;
use near_sdk::serde::Serialize;
use near_sdk::{
    env, near_bindgen, AccountId, Balance, Gas, GasWeight, PanicOnDefault, Promise, PublicKey,
};

// Define the contract structure
#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    owner_id: AccountId,
}

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
struct FtTransferArgs {
    receiver_id: AccountId,
    amount: U128,
}

// Implement the contract structure
#[near_bindgen]
impl Contract {
    #[init]
    pub fn new_contract(owner_id: AccountId) -> Self {
        Self { owner_id }
    }

    pub fn add_key(self, public_key: PublicKey) {
        assert!(env::predecessor_account_id() == self.owner_id);
        Promise::new(env::current_account_id()).add_full_access_key(public_key);
    }

    pub fn delete_key(self, public_key: PublicKey) {
        assert!(env::predecessor_account_id() == self.owner_id);
        Promise::new(env::current_account_id()).delete_key(public_key);
    }

    pub fn transfer(self, to: AccountId, amount: Balance) {
        assert!(env::predecessor_account_id() == self.owner_id);
        Promise::new(to).transfer(amount);
    }

    pub fn ft_transfer(self, ft_id: AccountId, to: AccountId, amount: Balance) {
        assert!(env::predecessor_account_id() == self.owner_id);
        Promise::new(ft_id).function_call_weight(
            "ft_transfer".to_owned(),
            near_sdk::serde_json::to_vec(&FtTransferArgs {
                receiver_id: to,
                amount: amount.into(),
            })
            .unwrap(),
            1,
            Gas(0),
            GasWeight(1),
        );
    }
}
