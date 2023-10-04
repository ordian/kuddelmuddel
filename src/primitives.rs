pub use crate::subxt::polkadot::runtime_types::polkadot_parachain::primitives::{
    ValidationCode, ValidationCodeHash,
};
pub use crate::subxt::polkadot::runtime_types::polkadot_primitives::v2::CandidateReceipt;
pub use ::subxt::utils::{H256, AccountId32};
pub use polkadot_node_primitives::AvailableData;
pub use polkadot_parachain_primitives::primitives::{BlockData, ValidationParams};
pub type SessionIndex = u32;
pub type ValidatorIndex = u32;
