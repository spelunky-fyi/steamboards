use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::thread;

use steamworks::Client;
use steamworks::Leaderboard;
use steamworks::LeaderboardEntry;
use steamworks::SteamError;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::io::BufWriter;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::tcp::WriteHalf;
use tokio::net::TcpListener;
use tokio::sync::mpsc::unbounded_channel;
use tokio::sync::mpsc::UnboundedSender;

pub type Error = anyhow::Error;
pub type MyResult<T> = std::result::Result<T, Error>;

#[derive(Debug)]
struct LeaderboardResult {
    handle: Handle,
    leaderboard: Leaderboard,
}

#[derive(Debug)]
struct Handle {
    response_tx: UnboundedSender<String>,
    command: Command,
}

#[derive(Debug)]
enum Command {
    Fetch(String),
    Info(String),
}

impl Command {
    fn from_string(command: &str) -> Option<Command> {
        let parts: Vec<&str> = command.trim_end().split(" ").collect();
        if parts.len() != 2 {
            return None;
        }

        match parts[0] {
            "FETCH" => Some(Command::Fetch(parts[1].into())),
            "INFO" => Some(Command::Info(parts[1].into())),
            _ => None,
        }
    }
}

fn get_error(msg: &str) -> String {
    let mut response = String::new();
    response.push_str("<response>\n");
    response.push_str(" <status>0</status>\n");
    response.push_str(&format!(" <message>{}</message>\n", msg));
    response.push_str("</response>\n");
    response
}

fn steam_worker(rx_steam_worker: Receiver<Handle>) {
    let (client, single) = Client::init().unwrap();
    let user_stats = client.user_stats();

    // Channel to send leaderboard result to.
    let (tx_leaderboard, rx_leaderboard) = mpsc::channel::<LeaderboardResult>();

    loop {
        // Check for messages from clients
        match rx_steam_worker.try_recv() {
            Ok(handle) => {
                println!("Received Message: {:?}", handle.command);
                let date = match handle.command {
                    Command::Fetch(ref date) | Command::Info(ref date) => date,
                };
                let tx_leaderboard = tx_leaderboard.clone();
                user_stats.find_leaderboard(
                    &format!("{} DAILY", date),
                    move |result: Result<Option<Leaderboard>, SteamError>| {
                        let leaderboard = match result {
                            Ok(leaderboard) => leaderboard,
                            Err(err) => {
                                dbg!(err);
                                return;
                            }
                        };

                        match leaderboard {
                            Some(leaderboard) => {
                                tx_leaderboard
                                    .send(LeaderboardResult {
                                        handle: handle,
                                        leaderboard: leaderboard,
                                    })
                                    .unwrap();
                            }
                            None => {
                                println!("No leaderboard found...");
                                handle
                                    .response_tx
                                    .send(get_error("Leaderboard NOT FOUND"))
                                    .unwrap();
                            }
                        };
                    },
                );
            }
            // Nothing to do.
            Err(_) => {}
        }

        // Check for responses from GetLeaderboards
        match rx_leaderboard.try_recv() {
            Ok(leaderboard_result) => {
                let leaderboard = leaderboard_result.leaderboard.clone();
                match leaderboard_result.handle.command {
                    Command::Info(date) => {
                        let mut response = String::new();
                        let name = user_stats.get_leaderboard_name(&leaderboard);
                        let count = user_stats.get_leaderboard_entry_count(&leaderboard);
                        response.push_str("<leaderboard>\n");
                        response.push_str(&format!(
                            " <url>http://mossranking.com/xml/{}.xml</url>\n",
                            leaderboard.raw()
                        ));
                        response.push_str(&format!(
                            " <lbid>{}</lbid>\n",
                            leaderboard_result.leaderboard.raw()
                        ));
                        response.push_str(&format!(" <name>{}</name>\n", name));
                        response.push_str(&format!(" <display_name>{}</display_name>\n", name));
                        response.push_str(&format!(" <entries>{}</entries>\n", count));
                        response.push_str(" <sortmethod>2</sortmethod>\n");
                        response.push_str(" <displaytype>1</displaytype>\n");
                        response.push_str("</leaderboard>\n");
                        match leaderboard_result.handle.response_tx.send(response) {
                            Ok(_) => {}
                            Err(err) => {
                                dbg!("Failed to send message!");
                            }
                        };
                    }
                    Command::Fetch(ref date) => {
                        let response_tx = leaderboard_result.handle.response_tx.clone();
                        let name = user_stats.get_leaderboard_name(&leaderboard);
                        let count = user_stats.get_leaderboard_entry_count(&leaderboard);
                        user_stats.download_leaderboard_entries(
                            &leaderboard_result.leaderboard.clone(),
                            steamworks::LeaderboardDataRequest::Global,
                            0,
                            9999,
                            1024,
                            move |result: Result<Vec<LeaderboardEntry>, SteamError>| {
                                let mut response = String::new();

                                response.push_str("<response>\n");
                                response.push_str(&format!("<appID>{}</appID>\n", 239350));
                                response.push_str(&format!(
                                    "<leaderboardID>{}</leaderboardID>\n",
                                    leaderboard.raw()
                                ));
                                response.push_str(&format!("<name>{}</name>\n", name));
                                response.push_str(&format!("<entryStart>{}</entryStart>\n", 0));
                                response.push_str(&format!("<entryEnd>{}</entryEnd>\n", count));
                                response
                                    .push_str(&format!("<resultCount>{}</resultCount>\n", count));
                                response.push_str(" <entries>\n");

                                for entry in result.unwrap() {
                                    response.push_str("  <entry>\n");
                                    response.push_str(&format!(
                                        "   <steamid>{}</steamid>\n",
                                        entry.user.raw()
                                    ));
                                    response
                                        .push_str(&format!("   <score>{}</score>\n", entry.score));
                                    response.push_str(&format!(
                                        "   <rank>{}</rank>\n",
                                        entry.global_rank
                                    ));
                                    response.push_str("   <ugcid>-1</ugcid>\n");
                                    response.push_str(&format!(
                                        "   <details>{:02x}000000{:02x}000000</details>\n",
                                        entry.details[0], entry.details[1]
                                    ));
                                    response.push_str("  </entry>\n");
                                }
                                response.push_str(" </entries>\n");
                                response.push_str("</response>\n");

                                match response_tx.send(response) {
                                    Ok(_) => {}
                                    Err(err) => {
                                        dbg!("Failed to send message!");
                                    }
                                };
                            },
                        );
                    }
                }
            }
            // Nothing to do.
            Err(_) => {}
        }

        single.run_callbacks();
        ::std::thread::sleep(::std::time::Duration::from_millis(50));
    }
}

#[tokio::main]
async fn main() -> MyResult<()> {
    let listener = TcpListener::bind("127.0.0.1:6142").await?;
    let (tx_steam_worker, rx_steam_worker) = mpsc::channel();

    thread::spawn(move || {
        steam_worker(rx_steam_worker);
    });

    loop {
        let (socket, _) = listener.accept().await.unwrap();
        let tx_steam_worker = tx_steam_worker.clone();
        // A new task is spawned for each inbound socket. The socket is
        // moved to the new task and processed there.
        tokio::spawn(async move {
            let (socket_read, socket_write) = socket.into_split();

            let mut reader = BufReader::new(socket_read);
            let mut writer = BufWriter::new(socket_write);
            let mut line = String::new();
            reader.read_line(&mut line).await;

            let command = match Command::from_string(&line) {
                Some(command) => command,
                None => {
                    writer.write_all("Invalid Command!".as_bytes()).await;
                    writer.flush().await;
                    return;
                }
            };

            let (response_tx, mut response_rx) = unbounded_channel();
            tx_steam_worker.send(Handle {
                response_tx: response_tx,
                command: command,
            });

            println!("Waiting for steam worker...");
            match response_rx.recv().await {
                Some(message) => {
                    writer.write_all(message.as_bytes()).await;
                    writer.flush().await;
                    println!("Done sending message!");
                }
                None => println!("Went away!"),
            };
            println!("bye!");
        });
    }
    Ok(())
}
