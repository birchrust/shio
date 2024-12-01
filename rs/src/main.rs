mod bids;
mod data_listener;
mod utils;

#[tokio::main]
async fn main() {
    println!("Hello, birch!");
    let key = "";

    match data_listener::connect_to_shio_feed(key).await {
        Ok(_) => println!("Shio Feed processing finished."),
        Err(e) => eprintln!("Error: {}", e),
    }
}
