pub use crate::subxt::polkadot::runtime_types::polkadot_parachain::primitives::{
    ValidationCode, ValidationCodeHash,
};
pub use crate::subxt::polkadot::runtime_types::polkadot_primitives::v2::CandidateReceipt;
pub use ::subxt::sp_core::H256;
pub use ::subxt::sp_runtime::AccountId32;
pub use polkadot_node_primitives::AvailableData;
pub use polkadot_parachain::primitives::{BlockData, ValidationParams};
pub type SessionIndex = u32;
pub type ValidatorIndex = u32;
