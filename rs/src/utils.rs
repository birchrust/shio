use anyhow::Result;
use fastcrypto::hash::HashFunction;
use fastcrypto::traits::Signer;
use rand::seq::SliceRandom;
use reqwest::Client;
use serde_json::{json, Value};
use shared_crypto::intent::{Intent, IntentMessage};
use std::error::Error;
use sui_sdk::SuiClientBuilder;
use sui_types::{
    base_types::SuiAddress,
    crypto::{EncodeDecodeBase64, SuiKeyPair},
    transaction::TransactionData,
};

pub struct GasAdjustmentResult {
    pub gas_budget: u64,
    pub digest: String,
}

const SHIO_RPC_URL: &str = "https://rpc.getshio.com";

pub fn get_random_global_state_object() -> Result<String> {
    let global_state_objects = vec![
        "0xc32ce42eac951759666cbc993646b72387ec2708a2917c2c6fb7d21f00108c18",
        "0x0289acae0edcdf1fe3aedc2e886bc23064d41c359e0179a18152a64d1c1c2b3e",
        "0xc56db634d02511e66d7ca1254312b71c60d64dc44bf67ea46b922c52d8aebba6",
        "0x828eb6b3354ad68a23dd792313a16a0d888b7ea4fdb884bb22bd569f8e61319e",
        "0x81538ef2909a3e0dd3d7f38bcbee191509bae4e8666272938ced295672e2ee8d",
        "0xac8ce2033571140509788337c8a1f3aa8941a320ecd7047acda310d39cad9e03",
        "0xef6bf4952968d25d3e79f7e4db1dc38f2e9d99d61ad38f3829acb4100fe6383a",
        "0xfce73f3c32c3f56ddb924a04cabd44dd870b72954bbe7c3d7767c3b8c25c4326",
        "0xbfdb691b8cc0b3c3a3b7a654f6682f3e53b164d9ee00b9582cdb4d0a353440a9",
        "0xc2559d5c52ae04837ddf943a8c2cd53a5a0b512cee615d30d3abe25aa339465e",
    ];
    let shared_object_id = global_state_objects
        .choose(&mut rand::thread_rng())
        .ok_or_else(|| anyhow::anyhow!("Failed to select a shared object"))?;

    Ok(shared_object_id.to_string())
}

pub async fn simulate_bid(
    tx_digest: &str,
    tx: &TransactionData,
) -> Result<(), Box<dyn std::error::Error>> {
    // 将 TransactionData 序列化为 BCS 字节流
    let serialized_data = bcs::to_bytes(&tx)?;
    let base64_encoded_data = base64::encode(&serialized_data);

    let response = send_rpc_request_test(tx_digest, &base64_encoded_data).await?;

    println!("Your reponse:{:?}", response);

    Ok(())
}

pub async fn submit_bid(
    opp_tx_digest: &str, // Opportunity transaction digest
    bid_amount: u64,     // Bid amount in MIST
    tx_data: TransactionData,
    wallet: SuiKeyPair,
) -> Result<Value, Box<dyn Error>> {
    // 将 TransactionData 序列化为 BCS 字节流
    let serialized_data = bcs::to_bytes(&tx_data)?;
    let tx_data_base64 = base64::encode(&serialized_data);

    // 签名
    let intent_msg = IntentMessage::new(Intent::sui_transaction(), tx_data);
    let raw_tx = bcs::to_bytes(&intent_msg).expect("bcs should not fail");
    let mut hasher = sui_types::crypto::DefaultHash::default();

    hasher.update(raw_tx.clone());
    let digest = hasher.finalize().digest;

    let sui_sig = wallet.sign(&digest).encode_base64();

    // 构建 JSON-RPC 请求体
    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "shio_submitBid",
        "params": [
            opp_tx_digest,
            bid_amount,
            tx_data_base64,
            sui_sig
        ]
    });

    println!(
        "Full Request: {}",
        serde_json::to_string_pretty(&request).unwrap()
    );

    // 发送 POST 请求
    let client = Client::new();
    let response = client.post(SHIO_RPC_URL).json(&request).send().await?;

    if !response.status().is_success() {
        return Err(format!("Failed to send request: HTTP Status {}", response.status()).into());
    }

    // 解析返回的 JSON 数据
    let response_json: Value = response.json().await?;
    if response_json.get("error").is_some() {
        Err(format!("RPC returned an error: {}", response_json["error"]).into())
    } else {
        Ok(response_json)
    }
}

pub fn check_time_out(deadline: u64) -> Result<bool, Box<dyn Error>> {
    let now = chrono::Utc::now().timestamp_millis() as u64;

    if now >= deadline {
        println!("Deadline passed, skipping bid.");
        return Ok(true);
    }

    Ok(false)
}

pub async fn send_rpc_request_test(
    tx_digest: &str,
    tx_data_base64: &str,
) -> Result<Value, Box<dyn Error>> {
    let client = Client::new();

    // 构建 JSON-RPC 请求体
    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "shio_simulateBid",
        "params": [
            tx_digest,
            tx_data_base64
        ]
    });

    // 发送 POST 请求
    let response = client.post(SHIO_RPC_URL).json(&request).send().await?;

    if !response.status().is_success() {
        return Err(format!("Failed to send request: HTTP Status {}", response.status()).into());
    }

    // 解析返回的 JSON 数据
    let response_json: Value = response.json().await?;
    if response_json.get("error").is_some() {
        Err(format!("RPC returned an error: {}", response_json["error"]).into())
    } else {
        Ok(response_json)
    }
}
