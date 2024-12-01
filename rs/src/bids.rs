use anyhow::anyhow;
use sui_sdk::rpc_types::SuiObjectDataOptions;
use sui_sdk::SuiClient;

use sui_types::{
    base_types::{ObjectID, SuiAddress},
    crypto::SuiKeyPair,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{CallArg, ObjectArg, TransactionData, TransactionDataAPI},
    Identifier,
};

use crate::utils::get_random_global_state_object;

pub async fn create_tx(
    sui_client: SuiClient,
    custom_wallet: SuiKeyPair,
    bid_amount: u64,
    target_digest: &str,
) -> (TransactionData, u64) {
    let sender = SuiAddress::from(&custom_wallet.public());

    // 获取共享对象
    let shared = get_random_global_state_object().unwrap();
    let shared_object = ObjectID::from_hex_literal(&shared).unwrap();

    let object_info = sui_client
        .read_api()
        .get_object_with_options(shared_object, SuiObjectDataOptions::full_content())
        .await
        .unwrap();

    let initial_shared_version = object_info.data.unwrap().version;

    println!("111:{:?}", initial_shared_version);

    let gas_price = sui_client
        .read_api()
        .get_reference_gas_price()
        .await
        .unwrap();

    let mut tx = TransactionData::new_programmable(
        sender,
        vec![],
        ProgrammableTransactionBuilder::new().finish(),
        0,
        0,
    );

    let mut gas_budget_result: u64 = 0;

    let mut current_gas_budget = 750000;
    let max_attempts = 10000;
    let mut attempts = 0;

    while attempts < max_attempts {
        // 每次循环重新获取 Gas Coin，避免重复使用
        let gas_coin = sui_client
            .coin_read_api()
            .get_coins(sender, None, None, None)
            .await
            .unwrap()
            .data
            .into_iter()
            .find(|coin| coin.balance > bid_amount && coin.coin_object_id != shared_object)
            .ok_or(anyhow!("No valid Gas Coin found"))
            .unwrap();

        println!("Gas Coin Object ID: {:?}", gas_coin.object_ref().0);

        let pt = {
            let mut builder = ProgrammableTransactionBuilder::new();

            let bid_amount_arg = CallArg::Pure(bcs::to_bytes(&(bid_amount as u64)).unwrap());
            let fee_arg = CallArg::Object(ObjectArg::ImmOrOwnedObject(gas_coin.object_ref()));
            let shared_arg = CallArg::Object(ObjectArg::SharedObject {
                id: shared_object,
                initial_shared_version,
                mutable: true,
            });

            builder
                .move_call(
                    ObjectID::from_hex_literal(
                        "0x1889977f0fb56ae730e7bda8e8e32859ce78874458c74910d36121a81a615123",
                    )
                    .unwrap(),
                    Identifier::new("auctioneer").unwrap(),
                    Identifier::new("submit_bid").unwrap(),
                    vec![],
                    vec![shared_arg, bid_amount_arg, fee_arg],
                )
                .unwrap();

            builder.finish()
        };

        tx = TransactionData::new_programmable(
            sender,
            vec![gas_coin.object_ref()],
            pt,
            current_gas_budget,
            gas_price,
        );

        let current_digest = tx.digest();

        if attempts % 100 == 0 {
            println!("Attempt {}:", attempts + 1);
            println!("- Current gas budget: {}", current_gas_budget);
            println!("- Current digest: {}", current_digest);
            println!("- Target digest: {}", target_digest);
        }

        if current_digest.to_string() > target_digest.to_string() {
            println!("✅ Found valid digest!");
            gas_budget_result = current_gas_budget;
            break;
        }

        current_gas_budget += 1;
        attempts += 1;
    }

    (update_gas_budget(tx, gas_budget_result), gas_budget_result)
}

fn update_gas_budget(tx: TransactionData, new_gas_budget: u64) -> TransactionData {
    let kind = tx.kind().clone();
    let sender = tx.sender();
    let mut gas_data = tx.gas_data().clone();

    // 更新 gas_budget
    gas_data.budget = new_gas_budget;

    // 使用新的 gas_data 创建新的 TransactionData
    TransactionData::new_with_gas_data(kind, sender, gas_data)
}
