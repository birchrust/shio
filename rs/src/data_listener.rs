use std::sync::Arc;

use sui_sdk::{SuiClient, SuiClientBuilder};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_native_tls::TlsConnector;
use tokio_tungstenite::{client_async, tungstenite::protocol::Message};

use url::Url;

use serde_json::Value;
use sui_types::crypto::SuiKeyPair;

use futures_util::{SinkExt, StreamExt};

use crate::{
    bids::create_tx,
    utils::{check_time_out, simulate_bid, submit_bid},
};

/// Shio Feed WebSocket 地址
const FEED_URL: &str = "wss://rpc.getshio.com/feed";

/// 连接到 Shio Feed 并处理消息
pub async fn connect_to_shio_feed(key: &str) -> Result<(), Box<dyn std::error::Error>> {
    let url: Url = FEED_URL.parse()?;
    let domain = url.host_str().ok_or("Missing domain")?;
    let connector = TlsConnector::from(native_tls::TlsConnector::new()?);

    let tcp_stream = TcpStream::connect((domain, 443)).await?;
    let tls_stream = connector.connect(domain, tcp_stream).await?;
    let (ws_stream, _) = client_async(url.as_str(), tls_stream)
        .await
        .expect("Failed to establish WebSocket connection");

    println!("Connected to Shio Feed");

    let (write, mut read) = ws_stream.split();
    let write = Arc::new(Mutex::new(write));
    let write_clone = Arc::clone(&write);

    let user_key = key.to_string();

    let sui_client = SuiClientBuilder::default()
        .build("https://sui-mainnet-ca-2.cosmostation.io:443/")
        .await
        .unwrap();
    // 启动消息处理器
    tokio::spawn(async move {
        while let Some(msg) = read.next().await {
            let mut write = write_clone.lock().await;
            match msg {
                Ok(Message::Text(text)) => {
                    if let Ok(json) = serde_json::from_str::<Value>(&text) {
                        if json.get("PingMessage").is_some() {
                            let pong = Message::Text(r#"{"PongMessage": {}}"#.to_string());
                            write.send(pong).await.expect("Failed to send PongMessage");
                            println!("Sent PongMessage");
                        } else {
                            data_listener(json, user_key.as_str(), &sui_client).await;
                        }
                    }
                }

                Err(e) => {
                    eprintln!("Error receiving message: {}", e);
                    break;
                }
                _ => {}
            }
        }
    });

    // 定期发送 Ping 消息以测试延迟
    loop {
        {
            let mut write = write.lock().await;
            write
                .send(Message::Ping(vec![]))
                .await
                .expect("Failed to send Ping");
        }

        // 等待 1 秒后再次发送 Ping
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
}

pub async fn data_listener(message: Value, key: &str, sui_client: &SuiClient) {
    let customer_wallet = SuiKeyPair::decode(key).unwrap();

    // Extract Messages
    if let Some(auction_started) = message.get("auctionStarted") {
        // Parsing auction data
        let tx_digest = match auction_started["txDigest"].as_str() {
            Some(digest) => digest,
            None => {
                eprintln!("Invalid or missing txDigest");
                return;
            }
        };

        let gas_price = match auction_started["gasPrice"].as_u64() {
            Some(price) => price,
            None => {
                eprintln!("Invalid or missing gasPrice");
                return;
            }
        };

        let deadline = match auction_started["deadlineTimestampMs"].as_u64() {
            Some(deadline) => deadline,
            None => {
                eprintln!("Invalid or missing deadlineTimestampMs");
                return;
            }
        };

        let is_timed_out = check_time_out(deadline).unwrap();

        // if is_timed_out {
        //     println!("Deadline passed, skipping bid.");
        //     return;
        // }

        println!("Auction Started:");
        println!("- txDigest: {}", tx_digest);
        println!("- gasPrice: {}", gas_price);

        let (tx, gas_budget_result) =
            create_tx(sui_client.clone(), customer_wallet, gas_price, tx_digest).await;
        let customer_wallet = SuiKeyPair::decode(key).unwrap();
        match simulate_bid(tx_digest, &tx).await {
            Ok(result) => {
                println!("Simulated bid result: {:?}", result);
            }
            Err(e) => {
                println!("Simulation failed: {}", e);
            }
        }

        if let Err(e) = submit_bid(tx_digest, gas_budget_result, tx, customer_wallet).await {
            eprintln!("Error generating bid: {}", e);
        }
    }
}
