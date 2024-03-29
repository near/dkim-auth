use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::{env, near_bindgen, AccountId, Balance, PanicOnDefault, Promise, PublicKey};

// Define the contract structure
#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct ControlDelegator {
    controller_id: AccountId,
}

// Implement the contract structure
#[near_bindgen]
impl ControlDelegator {
    #[init]
    pub fn set_controller(controller_id: AccountId) -> Self {
        Self { controller_id }
    }

    pub fn add_key(self, public_key: PublicKey) {
        assert!(env::predecessor_account_id() == self.controller_id);
        Promise::new(env::current_account_id()).add_full_access_key(public_key);
    }

    pub fn delete_key(self, public_key: PublicKey) {
        assert!(env::predecessor_account_id() == self.controller_id);
        Promise::new(env::current_account_id()).delete_key(public_key);
    }

    pub fn transfer(self, to: AccountId, amount: Balance) {
        assert!(env::predecessor_account_id() == self.controller_id);
        Promise::new(to).transfer(amount);
    }
}
