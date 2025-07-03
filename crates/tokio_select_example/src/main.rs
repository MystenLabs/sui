
use tokio::select;
use tokio::task::JoinSet;
use std::time::Duration;

#[tokio::main]
async fn main() {
    let mut operations = 0;

    let (send, mut recv) = tokio::sync::mpsc::channel(1);
    let mut count = 0;
    loop {
        // Enable this to make it work (Option 1)
        // operations += 1;
        // if operations % 30 == 0 {
        //     tokio::task::yield_now().await;  
        // }
        select! {
            // Remove this to make it work (Option 2)
            _ = std::future::ready(()) => {
                // This branch is always ready
            }

            value = recv.recv() => {
                println!("received {value:?}");
                if value.is_none() {
                    println!("Channel closed");
                    break;
                }
            }

            Ok(()) = send.send(count) => {
                println!("sent {count}");
                count += 1;
            }
        }
        
        // Enable this to make it work (Option 3)
        // tokio::time::sleep(Duration::from_millis(50)).await;
    }
    
    println!("\nTest completed. If it got stuck around 64, the bug is reproduced.");
} 