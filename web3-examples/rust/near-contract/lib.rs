use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::UnorderedMap;
use near_sdk::{env, near_bindgen, AccountId, Balance, Promise};

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize)]
pub struct GuestBook {
    messages: UnorderedMap<u64, Message>,
    message_count: u64,
}

#[derive(BorshDeserialize, BorshSerialize)]
pub struct Message {
    pub sender: AccountId,
    pub text: String,
    pub timestamp: u64,
    pub donation: Balance,
}

impl Default for GuestBook {
    fn default() -> Self {
        Self {
            messages: UnorderedMap::new(b"m"),
            message_count: 0,
        }
    }
}

#[near_bindgen]
impl GuestBook {
    /// Add a message to the guestbook
    #[payable]
    pub fn add_message(&mut self, text: String) {
        // Get the sender and attached deposit
        let sender = env::predecessor_account_id();
        let donation = env::attached_deposit();

        // Validate message
        assert!(text.len() > 0, "Message cannot be empty");
        assert!(text.len() <= 500, "Message too long (max 500 chars)");

        // Create message
        let message = Message {
            sender,
            text,
            timestamp: env::block_timestamp(),
            donation,
        };

        // Store message
        self.messages.insert(&self.message_count, &message);
        self.message_count += 1;

        env::log_str(&format!("Message added. Total: {}", self.message_count));
    }

    /// Get total number of messages
    pub fn get_message_count(&self) -> u64 {
        self.message_count
    }

    /// Get a specific message by ID
    pub fn get_message(&self, id: u64) -> Option<MessageView> {
        self.messages.get(&id).map(|m| MessageView {
            sender: m.sender,
            text: m.text,
            timestamp: m.timestamp,
            donation: m.donation,
        })
    }

    /// Get recent messages (last N)
    pub fn get_recent_messages(&self, count: u64) -> Vec<MessageView> {
        let start = if self.message_count > count {
            self.message_count - count
        } else {
            0
        };

        (start..self.message_count)
            .filter_map(|id| {
                self.messages.get(&id).map(|m| MessageView {
                    sender: m.sender,
                    text: m.text,
                    timestamp: m.timestamp,
                    donation: m.donation,
                })
            })
            .collect()
    }

    /// Get total donations received
    pub fn get_total_donations(&self) -> Balance {
        let mut total: Balance = 0;
        for id in 0..self.message_count {
            if let Some(message) = self.messages.get(&id) {
                total += message.donation;
            }
        }
        total
    }

    /// Withdraw donations (contract owner only)
    pub fn withdraw(&mut self, amount: Balance) -> Promise {
        // Only contract owner can withdraw
        assert_eq!(
            env::predecessor_account_id(),
            env::current_account_id(),
            "Only owner can withdraw"
        );

        Promise::new(env::predecessor_account_id()).transfer(amount)
    }
}

// View struct (doesn't need Borsh serialization)
#[derive(serde::Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct MessageView {
    pub sender: AccountId,
    pub text: String,
    pub timestamp: u64,
    pub donation: Balance,
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::VMContextBuilder;
    use near_sdk::testing_env;

    fn get_context(predecessor: AccountId) -> VMContextBuilder {
        let mut builder = VMContextBuilder::new();
        builder.predecessor_account_id(predecessor);
        builder
    }

    #[test]
    fn test_add_message() {
        let mut contract = GuestBook::default();
        let context = get_context("alice.near".parse().unwrap());
        testing_env!(context.build());

        contract.add_message("Hello NEAR!".to_string());
        assert_eq!(contract.get_message_count(), 1);

        let message = contract.get_message(0).unwrap();
        assert_eq!(message.text, "Hello NEAR!");
    }

    #[test]
    fn test_recent_messages() {
        let mut contract = GuestBook::default();
        let context = get_context("alice.near".parse().unwrap());
        testing_env!(context.build());

        contract.add_message("Message 1".to_string());
        contract.add_message("Message 2".to_string());
        contract.add_message("Message 3".to_string());

        let recent = contract.get_recent_messages(2);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].text, "Message 2");
        assert_eq!(recent[1].text, "Message 3");
    }
}
