use std::collections::{btree_map::Entry, BTreeMap};

use crate::SessionIndex;
use subxt::{
    sp_core::H256, sp_runtime::AccountId32, ClientBuilder, DefaultConfig, PolkadotExtrinsicParams,
};

#[subxt::subxt(runtime_metadata_path = "assets/kusama_metadata.scale")]
pub mod polkadot {}

pub async fn historical_account_keys(
    rpc_url: String,
    input: impl IntoIterator<Item = (SessionIndex, H256)>,
) -> anyhow::Result<BTreeMap<SessionIndex, Vec<AccountId32>>> {
    let api = ClientBuilder::new()
        .set_url(rpc_url)
        .build()
        .await?
        .to_runtime_api::<
            polkadot::RuntimeApi<
                DefaultConfig,
                PolkadotExtrinsicParams<DefaultConfig>,
            >
        >();

    let mut map: BTreeMap<SessionIndex, Vec<AccountId32>> = BTreeMap::new();

    for (session, block_hash) in input.into_iter() {
        if let Entry::Vacant(e) = map.entry(session) {
            // TODO: handle errors here
            let keys = api
                .storage()
                .para_session_info()
                .account_keys(&session, Some(block_hash))
                .await?;
            // TODO: handle None
            if let Some(keys) = keys {
                e.insert(keys);
            }
        }
    }

    Ok(map)
}
